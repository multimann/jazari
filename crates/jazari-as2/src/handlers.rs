//! Splits instance scripts into `on(...)` and `onClipEvent(...)` handlers and
//! maps their events to SWF flags.
//!
//! Rascal does not know these blocks: it compiles `on (press) { ... }` as a
//! call to a function named `on` followed by a plain block, without any error.
//! So the front-end finds the blocks itself and compiles each body on its own.
//!
//! Event names, messages and the empty-handler rules follow what the Flash 8
//! IDE does (checked by publishing probe documents with it):
//!
//! - event names and `<Key>` names are case-insensitive, single-character
//!   keys are not (`"a"` is 97, `"A"` is 65);
//! - `onClipEvent` takes exactly one event, `on` a list without duplicates
//!   and with at most one `keyPress`;
//! - `dragOver` and `dragOut` set two button conditions each;
//! - code outside the handlers, even a stray `;`, is an error.

use crate::scan::{Token, TokenKind, matching_close};
use std::ops::Range;
use swf::{ButtonActionCondition as Cond, ClipEventFlag as Clip};

/// What an instance script is attached to.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum InstanceKind {
    /// A button instance: `on(...)` only.
    Button,
    /// A movie clip instance: `onClipEvent(...)` and `on(...)`.
    MovieClip,
}

/// The keyword that opens a handler.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Keyword {
    /// `on (...)`.
    On,
    /// `onClipEvent (...)`.
    OnClipEvent,
}

impl Keyword {
    fn from_ident(text: &str) -> Option<Self> {
        if text.eq_ignore_ascii_case("on") {
            Some(Keyword::On)
        } else if text.eq_ignore_ascii_case("onClipEvent") {
            Some(Keyword::OnClipEvent)
        } else {
            None
        }
    }
}

/// An error with the byte offset it points at.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Problem {
    pub(crate) offset: usize,
    pub(crate) message: String,
}

impl Problem {
    fn new(offset: usize, message: impl Into<String>) -> Self {
        Self {
            offset,
            message: message.into(),
        }
    }
}

/// One event in a handler's list.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Event {
    pub(crate) name: String,
    pub(crate) offset: usize,
    /// The key string of `keyPress "<Key>"`, unquoted, with its offset.
    pub(crate) key: Option<(String, usize)>,
}

/// A handler block found in an instance script.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Handler {
    pub(crate) keyword: Keyword,
    pub(crate) offset: usize,
    pub(crate) events: Vec<Event>,
    /// Bytes between the braces.
    pub(crate) body: Range<usize>,
}

/// Splits an instance script into handlers. Problems in one handler do not
/// stop the others from being found, so all of them can be reported.
pub(crate) fn split(
    src: &str,
    tokens: &[Token],
    kind: InstanceKind,
) -> (Vec<Handler>, Vec<Problem>) {
    let mut handlers = Vec::new();
    let mut problems = Vec::new();
    let mut i = 0;
    while i < tokens.len() {
        let token = tokens[i];
        let keyword = (token.kind == TokenKind::Ident)
            .then(|| Keyword::from_ident(token.text(src)))
            .flatten();
        let Some(keyword) = keyword else {
            problems.push(Problem::new(
                token.start,
                match kind {
                    InstanceKind::Button => "Statement must appear within on handler",
                    InstanceKind::MovieClip => {
                        "Statement must appear within on/onClipEvent handler"
                    }
                },
            ));
            i = skip_to_next_handler(src, tokens, i + 1);
            continue;
        };
        match parse_handler(src, tokens, i, keyword) {
            Ok((handler, next)) => {
                if let Some(nested) =
                    find_handler_syntax(src, &tokens[i + 1..next.saturating_sub(1)])
                {
                    problems.push(Problem::new(
                        nested.offset,
                        "on handlers may not nest within other on handlers",
                    ));
                }
                handlers.push(handler);
                i = next;
            }
            Err((problem, next)) => {
                problems.push(problem);
                i = skip_to_next_handler(src, tokens, next.max(i + 1));
            }
        }
    }
    (handlers, problems)
}

/// Index of the next top-level token that can start a handler.
fn skip_to_next_handler(src: &str, tokens: &[Token], from: usize) -> usize {
    let mut depth = 0usize;
    for (i, token) in tokens.iter().enumerate().skip(from) {
        match token.kind {
            TokenKind::Punct(b'(' | b'[' | b'{') => depth += 1,
            TokenKind::Punct(b')' | b']' | b'}') => depth = depth.saturating_sub(1),
            TokenKind::Ident if depth == 0 && Keyword::from_ident(token.text(src)).is_some() => {
                return i;
            }
            _ => {}
        }
    }
    tokens.len()
}

/// Parses the handler whose keyword is `tokens[at]`. Returns it with the
/// index of the token after its closing brace, or a problem with the index to
/// resume from.
fn parse_handler(
    src: &str,
    tokens: &[Token],
    at: usize,
    keyword: Keyword,
) -> Result<(Handler, usize), (Problem, usize)> {
    let offset = tokens[at].start;
    let end_offset = src.len();
    let open = at + 1;
    match tokens.get(open) {
        Some(t) if t.is(b'(') => {}
        Some(t) => return Err((Problem::new(t.start, "'(' expected"), open)),
        None => return Err((Problem::new(end_offset, "'(' expected"), open)),
    }
    let close = matching_close(tokens, open).ok_or_else(|| {
        (
            Problem::new(tokens[open].start, "')' expected"),
            tokens.len(),
        )
    })?;
    let events = parse_events(src, &tokens[open + 1..close], tokens[close].start, keyword)
        .map_err(|problem| (problem, close + 1))?;

    let brace = close + 1;
    match tokens.get(brace) {
        Some(t) if t.is(b'{') => {}
        Some(t) => return Err((Problem::new(t.start, "'{' expected"), brace)),
        None => return Err((Problem::new(end_offset, "'{' expected"), brace)),
    }
    let Some(end) = matching_close(tokens, brace) else {
        return Err((
            Problem::new(offset, "Statement block must be terminated by '}'"),
            tokens.len(),
        ));
    };
    let handler = Handler {
        keyword,
        offset,
        events,
        body: tokens[brace].end..tokens[end].start,
    };
    Ok((handler, end + 1))
}

/// Parses `name, name, keyPress "<Key>"` (the tokens between the parentheses).
fn parse_events(
    src: &str,
    tokens: &[Token],
    close_offset: usize,
    keyword: Keyword,
) -> Result<Vec<Event>, Problem> {
    let expected = match keyword {
        Keyword::On => "Mouse event expected",
        Keyword::OnClipEvent => "Clip event expected",
    };
    let mut events: Vec<Event> = Vec::new();
    let mut i = 0;
    loop {
        let Some(name) = tokens.get(i) else {
            return Err(Problem::new(close_offset, expected));
        };
        if name.kind != TokenKind::Ident {
            return Err(Problem::new(name.start, expected));
        }
        let mut event = Event {
            name: name.text(src).to_owned(),
            offset: name.start,
            key: None,
        };
        i += 1;
        if event.name.eq_ignore_ascii_case("keyPress") {
            match tokens.get(i) {
                Some(t) if t.kind == TokenKind::Str => {
                    event.key = Some((unquote(t.text(src)), t.start));
                    i += 1;
                }
                Some(t) => return Err(Problem::new(t.start, "Invalid key code")),
                None => return Err(Problem::new(close_offset, "Invalid key code")),
            }
        }
        let duplicate = events.iter().any(|e| {
            e.name.eq_ignore_ascii_case(&event.name) || (e.key.is_some() && event.key.is_some())
        });
        if duplicate {
            return Err(Problem::new(event.offset, "Duplicate in event list"));
        }
        events.push(event);
        match tokens.get(i) {
            None => return Ok(events),
            Some(t) if t.is(b',') && keyword == Keyword::On => i += 1,
            Some(t) => return Err(Problem::new(t.start, "')' expected")),
        }
    }
}

/// The text of a string literal token without its quotes; `\\`, `\"` and
/// `\'` are unescaped.
fn unquote(literal: &str) -> String {
    let inner = literal
        .get(1..literal.len().saturating_sub(1).max(1))
        .unwrap_or_default();
    let mut out = String::with_capacity(inner.len());
    let mut chars = inner.chars();
    while let Some(c) = chars.next() {
        if c == '\\' {
            if let Some(next) = chars.next() {
                out.push(next);
            }
        } else {
            out.push(c);
        }
    }
    out
}

/// Where handler syntax was found in a script that must not have any.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct HandlerSyntax {
    pub(crate) keyword: Keyword,
    pub(crate) offset: usize,
}

/// Finds `on (...) {` or `onClipEvent (...) {` in a token run. A call such as
/// `on(1);` or a function named `on` does not count.
pub(crate) fn find_handler_syntax(src: &str, tokens: &[Token]) -> Option<HandlerSyntax> {
    for (i, token) in tokens.iter().enumerate() {
        if token.kind != TokenKind::Ident {
            continue;
        }
        let Some(keyword) = Keyword::from_ident(token.text(src)) else {
            continue;
        };
        let after_dot_or_function = i.checked_sub(1).map(|p| tokens[p]).is_some_and(|p| {
            p.is(b'.') || (p.kind == TokenKind::Ident && p.text(src) == "function")
        });
        if after_dot_or_function || !tokens.get(i + 1).is_some_and(|t| t.is(b'(')) {
            continue;
        }
        if let Some(close) = matching_close(tokens, i + 1)
            && tokens.get(close + 1).is_some_and(|t| t.is(b'{'))
        {
            return Some(HandlerSyntax {
                keyword,
                offset: token.start,
            });
        }
    }
    None
}

/// Button conditions of a handler, with the key code of a `keyPress`.
pub(crate) fn button_conditions(handler: &Handler) -> Result<(Cond, Option<u8>), Vec<Problem>> {
    if handler.keyword == Keyword::OnClipEvent {
        return Err(vec![Problem::new(
            handler.offset,
            "Clip events are permitted only for movie clip instances",
        )]);
    }
    let mut conditions = Cond::empty();
    let mut key_code = None;
    let mut problems = Vec::new();
    for event in &handler.events {
        let flags = match event.name.to_ascii_lowercase().as_str() {
            "press" => Cond::OVER_UP_TO_OVER_DOWN,
            "release" => Cond::OVER_DOWN_TO_OVER_UP,
            "releaseoutside" => Cond::OUT_DOWN_TO_IDLE,
            "rollover" => Cond::IDLE_TO_OVER_UP,
            "rollout" => Cond::OVER_UP_TO_IDLE,
            "dragover" => Cond::IDLE_TO_OVER_DOWN | Cond::OUT_DOWN_TO_OVER_DOWN,
            "dragout" => Cond::OVER_DOWN_TO_OUT_DOWN | Cond::OVER_DOWN_TO_IDLE,
            "keypress" => match event_key_code(event) {
                Ok(code) => {
                    key_code = Some(code);
                    Cond::from_key_code(code)
                }
                Err(problem) => {
                    problems.push(problem);
                    continue;
                }
            },
            _ => {
                problems.push(Problem::new(event.offset, "Invalid mouse event specified."));
                continue;
            }
        };
        conditions |= flags;
    }
    if problems.is_empty() {
        Ok((conditions, key_code))
    } else {
        Err(problems)
    }
}

/// Clip events of a handler, with the key code of a `keyPress`.
pub(crate) fn clip_events(handler: &Handler) -> Result<(Clip, Option<u8>), Vec<Problem>> {
    let mut events = Clip::empty();
    let mut key_code = None;
    let mut problems = Vec::new();
    for event in &handler.events {
        let name = event.name.to_ascii_lowercase();
        let flags = match handler.keyword {
            Keyword::OnClipEvent => match name.as_str() {
                "load" => Clip::LOAD,
                "enterframe" => Clip::ENTER_FRAME,
                "unload" => Clip::UNLOAD,
                "mousemove" => Clip::MOUSE_MOVE,
                "mousedown" => Clip::MOUSE_DOWN,
                "mouseup" => Clip::MOUSE_UP,
                "keydown" => Clip::KEY_DOWN,
                "keyup" => Clip::KEY_UP,
                "data" => Clip::DATA,
                "initialize" => Clip::INITIALIZE,
                "construct" => Clip::CONSTRUCT,
                _ => {
                    problems.push(Problem::new(
                        event.offset,
                        "Invalid movie clip event specified.",
                    ));
                    continue;
                }
            },
            Keyword::On => match name.as_str() {
                "press" => Clip::PRESS,
                "release" => Clip::RELEASE,
                "releaseoutside" => Clip::RELEASE_OUTSIDE,
                "rollover" => Clip::ROLL_OVER,
                "rollout" => Clip::ROLL_OUT,
                "dragover" => Clip::DRAG_OVER,
                "dragout" => Clip::DRAG_OUT,
                "keypress" => match event_key_code(event) {
                    Ok(code) => {
                        key_code = Some(code);
                        Clip::KEY_PRESS
                    }
                    Err(problem) => {
                        problems.push(problem);
                        continue;
                    }
                },
                _ => {
                    // Flash 8 turns this into a v2 component listener
                    // (`this.addEventListener("<name>", __f_<name>)`).
                    problems.push(Problem::new(
                        event.offset,
                        format!(
                            "on({}) on a movie clip registers a component event listener, \
                             which is not supported",
                            event.name
                        ),
                    ));
                    continue;
                }
            },
        };
        events |= flags;
    }
    if problems.is_empty() {
        Ok((events, key_code))
    } else {
        Err(problems)
    }
}

fn event_key_code(event: &Event) -> Result<u8, Problem> {
    let Some((key, offset)) = &event.key else {
        return Err(Problem::new(event.offset, "Invalid key code"));
    };
    key_code(key).map_err(|message| Problem::new(*offset, message))
}

/// Key names of `keyPress "<Name>"`, as the Flash 8 IDE writes them.
const KEY_NAMES: &[(&str, u8)] = &[
    ("left", 1),
    ("right", 2),
    ("home", 3),
    ("end", 4),
    ("insert", 5),
    ("delete", 6),
    ("backspace", 8),
    ("enter", 13),
    ("up", 14),
    ("down", 15),
    ("pageup", 16),
    ("pagedown", 17),
    ("tab", 18),
    ("escape", 19),
    ("space", 32),
];

/// The SWF key code of a `keyPress` string: a `<Name>` from [`KEY_NAMES`]
/// or one ASCII character.
pub(crate) fn key_code(key: &str) -> Result<u8, String> {
    if let Some(name) = key.strip_prefix('<').and_then(|k| k.strip_suffix('>')) {
        return KEY_NAMES
            .iter()
            .find(|(known, _)| known.eq_ignore_ascii_case(name))
            .map(|&(_, code)| code)
            .ok_or_else(|| "Invalid key code".to_owned());
    }
    let mut chars = key.chars();
    match (chars.next(), chars.next()) {
        (Some(c), None) if c.is_ascii() && c != '\0' => Ok(c as u8),
        (Some(c), None) => Err(format!(
            "keyPress \"{c}\": a key code must be an ASCII character or a <Key> name \
             (Flash 8 would silently store U+{:04X} as key code {})",
            u32::from(c),
            u32::from(c) & 0x7f
        )),
        _ => Err("Invalid key code".to_owned()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::scan::tokenize;

    fn split_src(src: &str, kind: InstanceKind) -> (Vec<Handler>, Vec<Problem>) {
        split(src, &tokenize(src), kind)
    }

    fn body<'a>(src: &'a str, handler: &Handler) -> &'a str {
        &src[handler.body.clone()]
    }

    #[test]
    fn splits_two_clip_handlers() {
        let src = "onClipEvent (load) {\n\tthis.n = 0;\n}\nonClipEvent (enterFrame) {\n\tif (this.n < 3) { this.n++; }\n}\n";
        let (handlers, problems) = split_src(src, InstanceKind::MovieClip);
        assert!(problems.is_empty(), "{problems:?}");
        assert_eq!(handlers.len(), 2);
        assert_eq!(handlers[0].keyword, Keyword::OnClipEvent);
        assert_eq!(handlers[0].events[0].name, "load");
        assert_eq!(body(src, &handlers[0]), "\n\tthis.n = 0;\n");
        assert_eq!(
            body(src, &handlers[1]),
            "\n\tif (this.n < 3) { this.n++; }\n"
        );
    }

    #[test]
    fn braces_in_strings_and_comments_do_not_count() {
        let src = "// a comment with } brace\nonClipEvent (load) {\n\tvar s = \"}{\"; /* } */\n\tif (true) { trace(s); }\n\ttrace('it\\'s }');\n}\n";
        let (handlers, problems) = split_src(src, InstanceKind::MovieClip);
        assert!(problems.is_empty(), "{problems:?}");
        assert_eq!(handlers.len(), 1);
        assert!(body(src, &handlers[0]).ends_with("trace('it\\'s }');\n"));
    }

    #[test]
    fn event_lists_and_key_strings() {
        let src = "on ( press ,\n release, keyPress '<Enter>' ) { }";
        let (handlers, problems) = split_src(src, InstanceKind::Button);
        assert!(problems.is_empty(), "{problems:?}");
        let names: Vec<&str> = handlers[0].events.iter().map(|e| e.name.as_str()).collect();
        assert_eq!(names, ["press", "release", "keyPress"]);
        assert_eq!(
            handlers[0].events[2].key.as_ref().map(|k| k.0.as_str()),
            Some("<Enter>")
        );
    }

    #[test]
    fn code_outside_handlers_is_an_error() {
        let (_, problems) = split_src("trace(\"x\");\non (release) { }", InstanceKind::Button);
        assert_eq!(
            problems,
            [Problem::new(0, "Statement must appear within on handler")]
        );
        let (handlers, problems) = split_src("onClipEvent (load) { };", InstanceKind::MovieClip);
        assert_eq!(handlers.len(), 1);
        assert_eq!(
            problems,
            [Problem::new(
                22,
                "Statement must appear within on/onClipEvent handler"
            )]
        );
    }

    #[test]
    fn syntax_problems_match_flash_messages() {
        let cases: &[(&str, &str, usize)] = &[
            ("on () { }", "Mouse event expected", 4),
            ("onClipEvent () { }", "Clip event expected", 13),
            ("onClipEvent (load, enterFrame) { }", "')' expected", 17),
            ("on (press, press) { }", "Duplicate in event list", 11),
            (
                "on (keyPress \"a\", keyPress \"b\") { }",
                "Duplicate in event list",
                18,
            ),
            ("on (release) trace(1);", "'{' expected", 13),
            (
                "on (press) { trace(1);",
                "Statement block must be terminated by '}'",
                0,
            ),
            ("on press { }", "'(' expected", 3),
            ("on (keyPress) { }", "Invalid key code", 12),
        ];
        for &(src, message, offset) in cases {
            let (_, problems) = split_src(src, InstanceKind::MovieClip);
            assert_eq!(problems, [Problem::new(offset, message)], "{src}");
        }
    }

    #[test]
    fn nested_handlers_are_rejected() {
        let src = "on (press) {\n\ton (release) { trace(1); }\n}\n";
        let (_, problems) = split_src(src, InstanceKind::Button);
        assert_eq!(
            problems,
            [Problem::new(
                14,
                "on handlers may not nest within other on handlers"
            )]
        );
    }

    #[test]
    fn handler_syntax_is_not_a_call_or_a_function() {
        let find = |src: &str| find_handler_syntax(src, &tokenize(src));
        assert_eq!(find("on(1); x.on(2) {}"), None);
        assert_eq!(find("function on(a) { }"), None);
        assert_eq!(
            find("x = 1;\nonClipEvent (load) {}"),
            Some(HandlerSyntax {
                keyword: Keyword::OnClipEvent,
                offset: 7
            })
        );
    }

    fn one_handler(src: &str, kind: InstanceKind) -> Handler {
        let (mut handlers, problems) = split_src(src, kind);
        assert!(problems.is_empty(), "{problems:?}");
        handlers.remove(0)
    }

    #[test]
    fn every_button_event() {
        let cases: &[(&str, Cond)] = &[
            ("press", Cond::OVER_UP_TO_OVER_DOWN),
            ("release", Cond::OVER_DOWN_TO_OVER_UP),
            ("releaseOutside", Cond::OUT_DOWN_TO_IDLE),
            ("rollOver", Cond::IDLE_TO_OVER_UP),
            ("rollOut", Cond::OVER_UP_TO_IDLE),
            (
                "dragOver",
                Cond::IDLE_TO_OVER_DOWN.union(Cond::OUT_DOWN_TO_OVER_DOWN),
            ),
            (
                "dragOut",
                Cond::OVER_DOWN_TO_OUT_DOWN.union(Cond::OVER_DOWN_TO_IDLE),
            ),
            ("PRESS", Cond::OVER_UP_TO_OVER_DOWN),
        ];
        for &(name, expected) in cases {
            let handler = one_handler(&format!("on ({name}) {{ }}"), InstanceKind::Button);
            assert_eq!(button_conditions(&handler), Ok((expected, None)), "{name}");
        }
        // The seven together, as in the Flash 8 probe: nine bits.
        let handler = one_handler(
            "on (press, release, releaseOutside, rollOver, rollOut, dragOver, dragOut) { }",
            InstanceKind::Button,
        );
        assert_eq!(
            button_conditions(&handler),
            Ok((Cond::from_bits_retain(0x1ff), None))
        );
    }

    #[test]
    fn button_key_press_combines_with_mouse_events() {
        let handler = one_handler("on (release, keyPress \"<Tab>\") { }", InstanceKind::Button);
        assert_eq!(
            button_conditions(&handler),
            Ok((
                Cond::OVER_DOWN_TO_OVER_UP | Cond::from_key_code(18),
                Some(18)
            ))
        );
    }

    #[test]
    fn button_rejects_clip_events() {
        let handler = one_handler("onClipEvent (load) { }", InstanceKind::Button);
        assert_eq!(
            button_conditions(&handler),
            Err(vec![Problem::new(
                0,
                "Clip events are permitted only for movie clip instances"
            )])
        );
        let handler = one_handler("on (load) { }", InstanceKind::Button);
        assert_eq!(
            button_conditions(&handler),
            Err(vec![Problem::new(4, "Invalid mouse event specified.")])
        );
    }

    #[test]
    fn every_clip_event() {
        let cases: &[(&str, Clip)] = &[
            ("onClipEvent (load)", Clip::LOAD),
            ("onClipEvent (enterFrame)", Clip::ENTER_FRAME),
            ("onClipEvent (unload)", Clip::UNLOAD),
            ("onClipEvent (mouseMove)", Clip::MOUSE_MOVE),
            ("onClipEvent (mouseDown)", Clip::MOUSE_DOWN),
            ("onClipEvent (mouseUp)", Clip::MOUSE_UP),
            ("onClipEvent (keyDown)", Clip::KEY_DOWN),
            ("onClipEvent (keyUp)", Clip::KEY_UP),
            ("onClipEvent (data)", Clip::DATA),
            ("onClipEvent (initialize)", Clip::INITIALIZE),
            ("onClipEvent (construct)", Clip::CONSTRUCT),
            ("onClipEvent (Load)", Clip::LOAD),
            ("on (press)", Clip::PRESS),
            ("on (release)", Clip::RELEASE),
            ("on (releaseOutside)", Clip::RELEASE_OUTSIDE),
            ("on (rollOver)", Clip::ROLL_OVER),
            ("on (rollOut)", Clip::ROLL_OUT),
            ("on (dragOver)", Clip::DRAG_OVER),
            ("on (dragOut)", Clip::DRAG_OUT),
            (
                "on (release, releaseOutside)",
                Clip::RELEASE.union(Clip::RELEASE_OUTSIDE),
            ),
        ];
        for &(header, expected) in cases {
            let handler = one_handler(&format!("{header} {{ }}"), InstanceKind::MovieClip);
            assert_eq!(clip_events(&handler), Ok((expected, None)), "{header}");
        }
        let handler = one_handler(
            "on (keyPress \"<Enter>\", press) { }",
            InstanceKind::MovieClip,
        );
        assert_eq!(
            clip_events(&handler),
            Ok((Clip::PRESS | Clip::KEY_PRESS, Some(13)))
        );
    }

    #[test]
    fn clip_rejects_unknown_events() {
        let handler = one_handler("onClipEvent (press) { }", InstanceKind::MovieClip);
        assert_eq!(
            clip_events(&handler),
            Err(vec![Problem::new(
                13,
                "Invalid movie clip event specified."
            )])
        );
        let handler = one_handler("on (load) { }", InstanceKind::MovieClip);
        let problems = clip_events(&handler).expect_err("component event");
        assert!(problems[0].message.contains("component event listener"));
    }

    #[test]
    fn key_codes() {
        assert_eq!(key_code("<Enter>"), Ok(13));
        assert_eq!(key_code("<enter>"), Ok(13));
        assert_eq!(key_code("<PageUp>"), Ok(16));
        assert_eq!(key_code("<Space>"), Ok(32));
        assert_eq!(key_code(" "), Ok(32));
        assert_eq!(key_code("a"), Ok(97));
        assert_eq!(key_code("A"), Ok(65));
        assert_eq!(key_code("ab"), Err("Invalid key code".to_owned()));
        assert_eq!(key_code("<Foo>"), Err("Invalid key code".to_owned()));
        assert_eq!(key_code(""), Err("Invalid key code".to_owned()));
        let arabic = key_code("م").expect_err("not ASCII");
        assert!(
            arabic.contains("U+0645") && arabic.contains("69"),
            "{arabic}"
        );
        for (name, code) in KEY_NAMES {
            assert_eq!(key_code(&format!("<{name}>")), Ok(*code));
        }
    }
}
