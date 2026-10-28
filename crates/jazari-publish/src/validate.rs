//! Validation by parse-back: the finished movie is decompressed and parsed
//! with the `swf` crate's reader, then checked for what the player relies
//! on (`docs/research/swf-writer.md`, F16, F24, F26).
//!
//! Per-tag truncations are already caught when each tag is encoded
//! ([`crate::tags::encode`] compares every tag with its own read-back). This
//! pass checks the movie as a whole:
//!
//! - the file parses, with version 8 and the expected root tag count;
//! - `num_frames` equals the ShowFrame count, for the root and every sprite;
//! - character ids are unique and every id is defined before it is used
//!   (placements, button records, bitmap fills, text fonts, exports, init
//!   actions);
//! - every placement depth is in 1..=16383;
//! - clip actions carry a key code exactly when they handle KEY_PRESS.

use std::collections::HashSet;

use swf::{CharacterId, ClipEventFlag, FillStyle, PlaceObjectAction, ShapeRecord, Tag};

use crate::display::MAX_TIMELINE_DEPTH;

/// What the parse-back found.
#[derive(Debug, Default, PartialEq, Eq)]
pub struct Report {
    /// Number of root frames in the header.
    pub num_frames: u16,
    /// Number of root tags read (without the end tag).
    pub root_tags: usize,
    /// Problems found; empty when the movie is consistent.
    pub problems: Vec<String>,
}

/// Parses `bytes` back and checks them. `Err` when the file cannot be read
/// at all.
pub fn validate(bytes: &[u8], expected_root_tags: Option<usize>) -> Result<Report, String> {
    let buf = swf::decompress_swf(bytes).map_err(|e| format!("cannot decompress: {e}"))?;
    let parsed = swf::parse_swf(&buf).map_err(|e| format!("cannot parse: {e}"))?;
    let mut report = Report {
        num_frames: parsed.header.num_frames(),
        root_tags: parsed.tags.len(),
        problems: Vec::new(),
    };
    let p = &mut report.problems;
    if parsed.header.version() != crate::tags::SWF_VERSION {
        p.push(format!("SWF version is {}", parsed.header.version()));
    }
    if let Some(n) = expected_root_tags
        && n != parsed.tags.len()
    {
        p.push(format!(
            "{n} root tags were written but {} read back",
            parsed.tags.len()
        ));
    }
    let frames = parsed
        .tags
        .iter()
        .filter(|t| matches!(t, Tag::ShowFrame))
        .count();
    if frames != usize::from(report.num_frames) {
        p.push(format!(
            "the header says {} frames but the root timeline has {frames}",
            report.num_frames
        ));
    }
    if !matches!(parsed.tags.first(), Some(Tag::FileAttributes(_))) {
        p.push("FileAttributes is not the first tag".to_owned());
    }
    let mut defined: HashSet<CharacterId> = HashSet::new();
    for tag in &parsed.tags {
        check_tag(tag, &mut defined, p, true);
    }
    Ok(report)
}

fn need(defined: &HashSet<CharacterId>, id: CharacterId, what: &str, p: &mut Vec<String>) {
    if !defined.contains(&id) {
        p.push(format!("{what} uses character {id} before it is defined"));
    }
}

fn define(defined: &mut HashSet<CharacterId>, id: CharacterId, p: &mut Vec<String>) {
    if !defined.insert(id) {
        p.push(format!("character {id} is defined twice"));
    }
}

fn check_fill(f: &FillStyle, defined: &HashSet<CharacterId>, p: &mut Vec<String>) {
    if let FillStyle::Bitmap { id, .. } = f {
        need(defined, *id, "a bitmap fill", p);
    }
}

fn check_tag(tag: &Tag<'_>, defined: &mut HashSet<CharacterId>, p: &mut Vec<String>, root: bool) {
    match tag {
        Tag::DefineShape(s) => {
            for f in &s.styles.fill_styles {
                check_fill(f, defined, p);
            }
            for r in &s.shape {
                if let ShapeRecord::StyleChange(sc) = r
                    && let Some(ns) = &sc.new_styles
                {
                    for f in &ns.fill_styles {
                        check_fill(f, defined, p);
                    }
                }
            }
            define(defined, s.id, p);
        }
        Tag::DefineSprite(s) => {
            let frames = s
                .tags
                .iter()
                .filter(|t| matches!(t, Tag::ShowFrame))
                .count();
            if frames != usize::from(s.num_frames) {
                p.push(format!(
                    "sprite {} says {} frames but has {frames}",
                    s.id, s.num_frames
                ));
            }
            for t in &s.tags {
                match t {
                    Tag::PlaceObject(_)
                    | Tag::RemoveObject(_)
                    | Tag::ShowFrame
                    | Tag::DoAction(_)
                    | Tag::FrameLabel(_) => check_tag(t, defined, p, false),
                    other => p.push(format!(
                        "sprite {} contains a {} tag",
                        s.id,
                        crate::tags::tag_name(other)
                    )),
                }
            }
            define(defined, s.id, p);
        }
        Tag::DefineButton2(b) => {
            for r in &b.records {
                need(defined, r.id, "a button record", p);
            }
            define(defined, b.id, p);
        }
        Tag::DefineText(t) | Tag::DefineText2(t) => {
            for r in &t.records {
                if let Some(f) = r.font_id {
                    need(defined, f, "a text record", p);
                }
            }
            define(defined, t.id, p);
        }
        Tag::DefineEditText(e) => {
            if let Some(f) = e.font_id() {
                need(defined, f, "an edit text", p);
            }
            define(defined, e.id(), p);
        }
        Tag::DefineFont2(f) => define(defined, f.id, p),
        Tag::DefineBitsLossless(b) => define(defined, b.id, p),
        Tag::DefineBitsJpeg2 { id, .. } => define(defined, *id, p),
        Tag::DefineBitsJpeg3(b) => define(defined, b.id, p),
        Tag::DefineFontAlignZones { id, .. } | Tag::DefineFontName { id, .. } => {
            need(defined, *id, "font information", p);
        }
        Tag::ExportAssets(list) => {
            for a in list {
                need(defined, a.id, "ExportAssets", p);
            }
        }
        Tag::DoInitAction { id, .. } => need(defined, *id, "DoInitAction", p),
        Tag::PlaceObject(po) => {
            if po.depth == 0 || u32::from(po.depth) > MAX_TIMELINE_DEPTH {
                p.push(format!("a placement uses depth {}", po.depth));
            }
            if let PlaceObjectAction::Place(id) | PlaceObjectAction::Replace(id) = po.action {
                need(
                    defined,
                    id,
                    if root {
                        "a placement"
                    } else {
                        "a sprite placement"
                    },
                    p,
                );
            }
            if let Some(ca) = &po.clip_actions {
                let mut all = ClipEventFlag::empty();
                for r in &ca.records {
                    all |= r.events;
                    if r.events.contains(ClipEventFlag::KEY_PRESS) != r.key_code.is_some() {
                        p.push(format!(
                            "clip actions at depth {} misuse the key code",
                            po.depth
                        ));
                    }
                }
                if all != ca.all_event_flags {
                    p.push(format!(
                        "clip actions at depth {} have wrong event flags",
                        po.depth
                    ));
                }
            }
        }
        _ => {}
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tags::{self, RawTag};
    use swf::{Header, Rectangle, Twips};

    fn header(frames: u16) -> Header {
        Header {
            compression: swf::Compression::Zlib,
            version: 8,
            stage_size: Rectangle {
                x_min: Twips::ZERO,
                x_max: Twips::new(11000),
                y_min: Twips::ZERO,
                y_max: Twips::new(8000),
            },
            frame_rate: swf::Fixed8::from_f32(12.0),
            num_frames: frames,
        }
    }

    #[test]
    fn frame_count_and_undefined_ids_are_found() {
        let place = tags::encode(&Tag::PlaceObject(Box::new(swf::PlaceObject {
            version: 2,
            action: PlaceObjectAction::Place(7),
            depth: 1,
            matrix: Some(swf::Matrix::IDENTITY),
            color_transform: None,
            ratio: None,
            name: None,
            clip_depth: None,
            class_name: None,
            filters: None,
            background_color: None,
            blend_mode: None,
            clip_actions: None,
            has_image: false,
            is_bitmap_cached: None,
            is_visible: None,
            amf_data: None,
        })))
        .expect("encode");
        let fa = tags::encode(&Tag::FileAttributes(swf::FileAttributes::empty())).expect("fa");
        let tags = vec![fa, place, RawTag::show_frame()];
        let bytes = tags::write_movie(&header(2), &tags).expect("write");
        let r = validate(&bytes, Some(3)).expect("parse");
        assert_eq!(r.root_tags, 3);
        assert_eq!(r.problems.len(), 2, "{:?}", r.problems);
    }

    #[test]
    fn garbage_is_an_error() {
        assert!(validate(b"CWS\x08junk", None).is_err());
    }
}
