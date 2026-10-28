//! Helpers shared by the integration tests: fixture paths, publishing, and
//! a small SWF inspector (tag dump and root display-list simulation) used to
//! compare our output with Flash 8's `flash8.swf`. No Ruffle involved.

#![allow(dead_code)]

use std::collections::BTreeMap;
use std::path::PathBuf;

use jazari_publish::{BuildMode, PublishOptions, PublishOutput, Severity, publish};
use swf::{CharacterId, ColorTransform, Matrix, PlaceObjectAction, Rectangle, Tag, Twips};

/// `tests/fixtures` at the repository root.
pub fn fixtures_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../tests/fixtures")
}

/// Every fixture folder that has an `xfl/` (new fixtures are picked up).
pub fn all_fixtures() -> Vec<String> {
    let mut out: Vec<String> = std::fs::read_dir(fixtures_dir())
        .map(|rd| {
            rd.filter_map(Result::ok)
                .filter(|e| e.path().join("xfl").is_dir())
                .filter_map(|e| e.file_name().into_string().ok())
                .collect()
        })
        .unwrap_or_default();
    out.sort();
    out
}

/// Opens a fixture's XFL.
pub fn open(name: &str) -> jazari_xfl::Document {
    let path = fixtures_dir().join(name).join("xfl");
    let (doc, _report) = jazari_xfl::open(&path).unwrap_or_else(|e| panic!("open {name}: {e}"));
    doc
}

/// Publishes a fixture with the embedded fonts only.
pub fn publish_fixture(name: &str, mode: BuildMode) -> PublishOutput {
    let doc = open(name);
    publish_doc(&doc, mode)
}

/// Publishes a document with the embedded fonts only.
pub fn publish_doc(doc: &jazari_xfl::Document, mode: BuildMode) -> PublishOutput {
    let fonts = jazari_text::FontLibrary::embedded_only();
    let as2 = jazari_as2::Compiler::new(Default::default());
    publish(
        doc,
        &PublishOptions {
            mode,
            compress: true,
        },
        &fonts,
        &as2,
    )
}

/// The errors among diagnostics, formatted.
pub fn errors(out: &PublishOutput) -> Vec<String> {
    out.diagnostics
        .iter()
        .filter(|d| d.severity == Severity::Error)
        .map(ToString::to_string)
        .collect()
}

/// Flash 8's published SWF for a fixture.
pub fn flash8(name: &str) -> Vec<u8> {
    std::fs::read(fixtures_dir().join(name).join("flash8.swf")).expect("flash8.swf")
}

/// Decompresses and parses, then calls `f` with the parsed movie.
pub fn with_parsed<R>(bytes: &[u8], f: impl FnOnce(&swf::Swf<'_>) -> R) -> R {
    let buf = swf::decompress_swf(bytes).expect("decompress");
    let parsed = swf::parse_swf(&buf).expect("parse");
    f(&parsed)
}

fn name_of(n: Option<&swf::SwfStr>) -> String {
    n.map(|n| format!(" name={}", n.to_str_lossy(swf::UTF_8)))
        .unwrap_or_default()
}

/// A one-line description of a tag.
pub fn describe(tag: &Tag<'_>) -> String {
    match tag {
        Tag::DefineShape(s) => format!("DefineShape{} id={}", s.version, s.id),
        Tag::DefineSprite(s) => format!("DefineSprite id={} frames={}", s.id, s.num_frames),
        Tag::DefineButton2(b) => format!("DefineButton2 id={} records={}", b.id, b.records.len()),
        Tag::PlaceObject(p) => format!(
            "PlaceObject{} depth={} {:?}{}{}",
            p.version,
            p.depth,
            p.action,
            name_of(p.name),
            if p.clip_actions.is_some() {
                " clip-actions"
            } else {
                ""
            }
        ),
        Tag::RemoveObject(r) => format!("RemoveObject2 depth={}", r.depth),
        Tag::DoAction(a) => format!("DoAction ({} bytes)", a.len()),
        Tag::DoInitAction { id, .. } => format!("DoInitAction id={id}"),
        Tag::FrameLabel(l) => format!("FrameLabel {}", l.label.to_str_lossy(swf::UTF_8)),
        Tag::ShowFrame => "ShowFrame".to_owned(),
        Tag::FileAttributes(a) => format!("FileAttributes {a:?}"),
        Tag::SetBackgroundColor(c) => {
            format!("SetBackgroundColor {:02X}{:02X}{:02X}", c.r, c.g, c.b)
        }
        Tag::ExportAssets(e) => format!("ExportAssets {}", e.len()),
        other => format!("{other:?}").chars().take(40).collect(),
    }
}

/// The root tag list, one line per tag (sprite contents indented).
pub fn dump(bytes: &[u8]) -> Vec<String> {
    with_parsed(bytes, |swf| {
        let mut out = Vec::new();
        for t in &swf.tags {
            out.push(describe(t));
            if let Tag::DefineSprite(s) = t {
                for c in &s.tags {
                    out.push(format!("    {}", describe(c)));
                }
            }
        }
        out
    })
}

/// What is at one depth after a frame.
#[derive(Debug, Clone, PartialEq)]
pub struct Placed {
    /// Character id.
    pub char: CharacterId,
    /// Matrix.
    pub matrix: Matrix,
    /// Colour transform.
    pub cxform: ColorTransform,
    /// Instance name.
    pub name: Option<String>,
}

/// What a character is.
#[derive(Debug, Clone, PartialEq)]
pub enum CharInfo {
    /// A shape and its bounds.
    Shape(Rectangle<Twips>),
    /// A sprite.
    Sprite,
    /// Anything else.
    Other,
}

/// The root display list after each frame, and what every character is.
pub fn root_states(bytes: &[u8]) -> (Vec<BTreeMap<u16, Placed>>, BTreeMap<CharacterId, CharInfo>) {
    with_parsed(bytes, |swf| {
        let mut chars = BTreeMap::new();
        let mut dl: BTreeMap<u16, Placed> = BTreeMap::new();
        let mut frames = Vec::new();
        for t in &swf.tags {
            match t {
                Tag::DefineShape(s) => {
                    chars.insert(s.id, CharInfo::Shape(s.shape_bounds));
                }
                Tag::DefineSprite(s) => {
                    chars.insert(s.id, CharInfo::Sprite);
                }
                Tag::DefineButton2(b) => {
                    chars.insert(b.id, CharInfo::Other);
                }
                Tag::RemoveObject(r) => {
                    dl.remove(&r.depth);
                }
                Tag::PlaceObject(p) => {
                    if let PlaceObjectAction::Place(id) = p.action {
                        dl.insert(
                            p.depth,
                            Placed {
                                char: id,
                                matrix: Matrix::IDENTITY,
                                cxform: ColorTransform::IDENTITY,
                                name: None,
                            },
                        );
                    }
                    if let Some(e) = dl.get_mut(&p.depth) {
                        if let PlaceObjectAction::Replace(id) = p.action {
                            e.char = id;
                        }
                        if let Some(m) = p.matrix {
                            e.matrix = m;
                        }
                        if let Some(c) = p.color_transform {
                            e.cxform = c;
                        }
                        if let Some(n) = p.name {
                            e.name = Some(n.to_str_lossy(swf::UTF_8).into_owned());
                        }
                    }
                }
                Tag::ShowFrame => frames.push(dl.clone()),
                _ => {}
            }
        }
        (frames, chars)
    })
}
