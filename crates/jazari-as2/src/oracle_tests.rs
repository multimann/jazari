//! Comparison with the Flash 8 IDE.
//!
//! `tests/flash8/` holds two probe documents that the real Flash 8 IDE
//! compiled (see `gen_probe.py` and `gen_probe2.py` there): their sources,
//! the published SWFs and the IDE's Output panel. Every frame script and
//! instance script is compiled here and compared with what Flash 8 did:
//! the same bytecode up to constant pools, push grouping and number types,
//! the same errors on the same lines, and the same handler records.

use crate::tests::normalized;
use crate::{Compiler, CompilerOptions, Diagnostic, Origin};
use std::collections::BTreeMap;

const PROBE1_SWF: &[u8] = include_bytes!("../tests/flash8/as2probe.swf");
const PROBE1_SOURCES: &str = include_str!("../tests/flash8/probe-sources.json");
const PROBE1_OUTPUT: &str = include_str!("../tests/flash8/as2probe-output.txt");
const PROBE2_SWF: &[u8] = include_bytes!("../tests/flash8/as2probe2.swf");
const PROBE2_SOURCES: &str = include_str!("../tests/flash8/probe2-sources.json");
const PROBE2_OUTPUT: &str = include_str!("../tests/flash8/as2probe2-output.txt");

// ---- a JSON reader for the probe sources (strings, arrays and objects only)

#[derive(Debug, Clone)]
enum Json {
    Str(String),
    Arr(Vec<Json>),
    Obj(Vec<(String, Json)>),
}

impl Json {
    fn parse(text: &str) -> Json {
        let mut chars = text.chars().peekable();
        let value = Self::value(&mut chars);
        assert!(chars.all(char::is_whitespace), "trailing JSON");
        value
    }

    fn skip_ws(chars: &mut std::iter::Peekable<std::str::Chars<'_>>) {
        while chars.peek().is_some_and(|c| c.is_whitespace()) {
            chars.next();
        }
    }

    fn value(chars: &mut std::iter::Peekable<std::str::Chars<'_>>) -> Json {
        Self::skip_ws(chars);
        match chars.next() {
            Some('"') => Json::Str(Self::string(chars)),
            Some('[') => {
                let mut items = Vec::new();
                loop {
                    Self::skip_ws(chars);
                    if chars.peek() == Some(&']') {
                        chars.next();
                        break;
                    }
                    items.push(Self::value(chars));
                    Self::skip_ws(chars);
                    match chars.next() {
                        Some(',') => {}
                        Some(']') => break,
                        other => panic!("bad JSON array: {other:?}"),
                    }
                }
                Json::Arr(items)
            }
            Some('{') => {
                let mut fields = Vec::new();
                loop {
                    Self::skip_ws(chars);
                    match chars.next() {
                        Some('}') => break,
                        Some('"') => {}
                        other => panic!("bad JSON object: {other:?}"),
                    }
                    let key = Self::string(chars);
                    Self::skip_ws(chars);
                    assert_eq!(chars.next(), Some(':'));
                    fields.push((key, Self::value(chars)));
                    Self::skip_ws(chars);
                    match chars.next() {
                        Some(',') => {}
                        Some('}') => break,
                        other => panic!("bad JSON object: {other:?}"),
                    }
                }
                Json::Obj(fields)
            }
            other => panic!("unsupported JSON value: {other:?}"),
        }
    }

    fn string(chars: &mut std::iter::Peekable<std::str::Chars<'_>>) -> String {
        let mut out = String::new();
        loop {
            match chars.next().expect("unterminated JSON string") {
                '"' => return out,
                '\\' => match chars.next().expect("JSON escape") {
                    'n' => out.push('\n'),
                    't' => out.push('\t'),
                    'r' => out.push('\r'),
                    'b' => out.push('\u{8}'),
                    'f' => out.push('\u{c}'),
                    'u' => {
                        let hex: String = chars.by_ref().take(4).collect();
                        let unit = u32::from_str_radix(&hex, 16).expect("hex");
                        out.push(char::from_u32(unit).expect("no surrogates in the probes"));
                    }
                    other => out.push(other),
                },
                c => out.push(c),
            }
        }
    }

    fn field(&self, name: &str) -> &Json {
        let Json::Obj(fields) = self else {
            panic!("not an object")
        };
        &fields.iter().find(|(k, _)| k == name).expect("field").1
    }

    fn items(&self) -> &[Json] {
        let Json::Arr(items) = self else {
            panic!("not an array")
        };
        items
    }

    fn text(&self) -> &str {
        let Json::Str(s) = self else {
            panic!("not a string")
        };
        s
    }
}

// ---- reading what Flash 8 produced

/// The handler records Flash 8 wrote for one instance.
#[derive(Debug, Default)]
struct FlashInstance {
    /// (condition bits, actions) of a DefineButton2.
    buttons: Vec<(u16, Vec<u8>)>,
    /// (event bits, key code, actions) of a PlaceObject2.
    clips: Vec<(u32, Option<u8>, Vec<u8>)>,
}

struct FlashProbe {
    /// DoAction payloads by 0-based frame.
    frames: BTreeMap<usize, Vec<u8>>,
    instances: BTreeMap<String, FlashInstance>,
}

fn read_probe(swf_bytes: &[u8]) -> FlashProbe {
    let buf = swf::decompress_swf(swf_bytes).expect("valid SWF");
    let parsed = swf::parse_swf(&buf).expect("valid SWF");
    let mut frames = BTreeMap::new();
    let mut buttons: BTreeMap<u16, Vec<(u16, Vec<u8>)>> = BTreeMap::new();
    let mut instances = BTreeMap::new();
    let mut frame = 0;
    for tag in &parsed.tags {
        match tag {
            swf::Tag::DoAction(data) => {
                frames.insert(frame, data.to_vec());
            }
            swf::Tag::ShowFrame => frame += 1,
            swf::Tag::DefineButton2(button) => {
                let actions = button
                    .actions
                    .iter()
                    .map(|a| (a.conditions.bits(), a.action_data.to_vec()))
                    .collect();
                buttons.insert(button.id, actions);
            }
            swf::Tag::PlaceObject(place) => {
                let (Some(name), swf::PlaceObjectAction::Place(id)) = (place.name, &place.action)
                else {
                    continue;
                };
                let mut instance = FlashInstance {
                    buttons: buttons.get(id).cloned().unwrap_or_default(),
                    ..FlashInstance::default()
                };
                if let Some(actions) = &place.clip_actions {
                    instance.clips = actions
                        .records
                        .iter()
                        .map(|a| (a.events.bits(), a.key_code, a.action_data.to_vec()))
                        .collect();
                }
                instances.insert(name.to_str_lossy(swf::UTF_8).into_owned(), instance);
            }
            _ => {}
        }
    }
    FlashProbe { frames, instances }
}

/// An error from the Output panel: layer, 1-based frame, line, message.
#[derive(Debug, Clone, PartialEq, Eq)]
struct FlashError {
    layer: String,
    frame: u32,
    line: u32,
    message: String,
}

fn flash_errors(output: &str) -> Vec<FlashError> {
    output
        .lines()
        .filter_map(|line| {
            let rest = line
                .trim_start_matches('\u{feff}')
                .strip_prefix("**Error** Scene=Scene 1, layer=")?;
            let (layer, rest) = rest.split_once(", frame=")?;
            let (frame, rest) = rest.split_once(":Line ")?;
            let (number, message) = rest.split_once(": ")?;
            Some(FlashError {
                layer: layer.to_owned(),
                frame: frame.parse().ok()?,
                line: number.parse().ok()?,
                message: message.trim_end().to_owned(),
            })
        })
        .collect()
}

/// Messages the front-end copies from Flash 8 word for word.
const FLASH_MESSAGES: &[&str] = &[
    "Left side of assignment operator must be variable or property.",
    "Mouse events are permitted only for button instances",
    "Clip events are permitted only for movie clip instances",
    "Unexpected '@' encountered",
    "Statement must appear within on handler",
    "Statement must appear within on/onClipEvent handler",
    "')' expected",
    "Duplicate in event list",
    "Invalid key code",
    "Invalid mouse event specified.",
    "Statement block must be terminated by '}'",
    "on handlers may not nest within other on handlers",
    "'{' expected",
    "Mouse event expected",
    "Invalid movie clip event specified.",
];

fn first_error(diagnostics: &[Diagnostic]) -> (u32, &str) {
    let first = diagnostics.iter().min_by_key(|d| d.line).expect("an error");
    (first.line, first.message.as_str())
}

fn check_error(what: &str, ours: &[Diagnostic], flash: &FlashError) {
    let (line, message) = first_error(ours);
    assert_eq!(
        line, flash.line,
        "{what}: line of {message:?} vs Flash {flash:?}"
    );
    if FLASH_MESSAGES.contains(&flash.message.as_str()) {
        assert_eq!(message, flash.message, "{what}");
    }
}

#[test]
fn probe_frames_match_flash_8() {
    let sources = Json::parse(PROBE1_SOURCES);
    let flash = read_probe(PROBE1_SWF);
    let errors = flash_errors(PROBE1_OUTPUT);
    let compiler = Compiler::new(CompilerOptions::default());
    let mut compared = 0;
    for (index, source) in sources.field("frames").items().iter().enumerate() {
        let src = source.text();
        let ours = compiler.compile_frame(src, &Origin::default());
        let flash_error = errors
            .iter()
            .find(|e| e.layer == "actions" && e.frame as usize == index + 1);
        let flash_actions = &flash.frames[&index];
        match (flash_error, ours) {
            (Some(flash_error), Err(ours)) => check_error(src, &ours, flash_error),
            (Some(flash_error), Ok(_)) => {
                // Rascal is more lenient here: Flash 8 wants a `;` or a line
                // break after `do ... while (...)`.
                assert_eq!(
                    src, "do { } while (false) trace(\"after\");\n",
                    "{flash_error:?}"
                );
                assert_eq!(flash_error.message, "Syntax error.");
            }
            (None, Err(ours)) => panic!("{src:?}: Flash 8 compiles it, we fail: {ours:?}"),
            (None, Ok(ours)) => {
                let (ours, theirs) = (normalized(&ours), normalized(flash_actions));
                if src.contains("xC3") {
                    // Flash 8 reads \xNN through the system ANSI code page
                    // (Windows-1256 on the probe machine): \xC3 is U+0623.
                    assert!(theirs.contains(&"0:Push \"s\", \"\u{623}\u{a9}\"".to_owned()));
                    assert!(ours.contains(&"0:Push \"s\", \"\u{c3}\u{a9}\"".to_owned()));
                } else if src.contains("uD83D") {
                    // Flash 8 writes each surrogate as its own UTF-8 sequence
                    // (CESU-8); we write the real character.
                    assert!(
                        flash_actions
                            .windows(6)
                            .any(|w| w == b"\xed\xa0\xbd\xed\xb8\x80")
                    );
                    assert!(ours.contains(&"0:Push \"s\", \"\u{1f600}\"".to_owned()));
                } else {
                    assert_eq!(ours, theirs, "{src:?}");
                    compared += 1;
                }
            }
        }
    }
    assert_eq!(compared, 15, "frames compared action by action");
}

#[test]
fn probe_instances_match_flash_8() {
    let compiler = Compiler::new(CompilerOptions::default());
    let mut compared = 0;
    for (sources, swf_bytes, output) in [
        (PROBE1_SOURCES, PROBE1_SWF, PROBE1_OUTPUT),
        (PROBE2_SOURCES, PROBE2_SWF, PROBE2_OUTPUT),
    ] {
        let sources = Json::parse(sources);
        let flash = read_probe(swf_bytes);
        let errors = flash_errors(output);
        for instance in sources.field("instances").items() {
            let [kind, name, src] = instance.items() else {
                panic!("bad instance entry");
            };
            let (kind, name, src) = (kind.text(), name.text(), src.text());
            let flash_error = errors.iter().find(|e| e.layer == format!("L_{name}"));
            let flash_records = &flash.instances[name];
            if kind == "button" {
                let ours = compiler.compile_button_handlers(src, &Origin::default());
                match (flash_error, ours) {
                    (Some(flash_error), Err(ours)) => check_error(name, &ours, flash_error),
                    (None, Err(ours)) if name == "b_arabic_key" => {
                        // Flash 8 silently truncates U+0645 to key code 69.
                        assert_eq!(flash_records.buttons[0].0 >> 9, 69);
                        assert!(ours[0].message.contains("U+0645"), "{ours:?}");
                    }
                    (flash_error, ours) => {
                        assert!(
                            flash_error.is_none(),
                            "{name}: Flash 8 fails: {flash_error:?}"
                        );
                        let ours = ours.unwrap_or_else(|e| panic!("{name}: {e:?}"));
                        let ours: Vec<(u16, Vec<String>)> = ours
                            .iter()
                            .map(|h| (h.conditions.bits(), normalized(&h.actions)))
                            .collect();
                        let theirs: Vec<(u16, Vec<String>)> = flash_records
                            .buttons
                            .iter()
                            .map(|(bits, actions)| (*bits, normalized(actions)))
                            .collect();
                        assert_eq!(ours, theirs, "{name}");
                        compared += 1;
                    }
                }
            } else {
                let ours = compiler.compile_clip_handlers(src, &Origin::default());
                match (flash_error, ours) {
                    (Some(flash_error), Err(ours)) => check_error(name, &ours, flash_error),
                    (None, Err(ours)) if name == "m_clip_ev_in_on" => {
                        // Flash 8 makes `on (load)` a v2 component listener.
                        assert!(ours[0].message.contains("component event listener"));
                    }
                    (flash_error, ours) => {
                        assert!(
                            flash_error.is_none(),
                            "{name}: Flash 8 fails: {flash_error:?}"
                        );
                        let ours = ours.unwrap_or_else(|e| panic!("{name}: {e:?}"));
                        let mut ours: Vec<(u32, Option<u8>, Vec<String>)> = ours
                            .iter()
                            .map(|h| (h.events.bits(), h.key_code, normalized(&h.actions)))
                            .collect();
                        if name == "mc_nested" {
                            // Flash 8 drops the test of `if (true)`; Rascal
                            // keeps it. Same behaviour, three more actions.
                            let actions = &mut ours[0].2;
                            let test = ["0:Push true", "0:Not", "0:If +4"];
                            let at = actions
                                .windows(3)
                                .position(|w| w == test)
                                .expect("the if (true) test");
                            actions.drain(at..at + 3);
                        }
                        let theirs: Vec<(u32, Option<u8>, Vec<String>)> = flash_records
                            .clips
                            .iter()
                            .map(|(bits, key, actions)| (*bits, *key, normalized(actions)))
                            .collect();
                        assert_eq!(ours, theirs, "{name}");
                        compared += 1;
                    }
                }
            }
        }
    }
    assert_eq!(compared, 23, "instances compared record by record");
}

#[test]
fn flash_output_parser_reads_every_error() {
    assert_eq!(flash_errors(PROBE1_OUTPUT).len(), 13);
    assert_eq!(flash_errors(PROBE2_OUTPUT).len(), 15);
}
