//! Tag encoding: every tag is serialized as soon as it is built.
//!
//! `swf::Tag<'a>` borrows its byte and string data, which makes a whole-movie
//! `Vec<Tag>` awkward to build incrementally. Instead each tag is encoded on
//! the spot into a [`RawTag`] (tag code + body bytes) with the `swf` crate's
//! own writer, and the movie is framed from raw tags at the end with
//! `swf::write::write_swf_raw_tags`.
//!
//! Every [`encode`] immediately reads the bytes back and compares the parsed
//! tag with the original. That catches each of the writer's silent
//! truncations (`docs/research/swf-writer.md`, F26) at the tag that caused
//! it, instead of producing a corrupt movie.
//!
//! Framing is ours, so the bitmap definition tags get the long record header
//! Flash writes for them (research, missed point on header forms).

use std::fmt::Write as _;

use swf::{Compression, Header, Rectangle, Tag, TagCode, Twips};

use crate::error::PublishError;

/// SWF version of everything we write.
pub const SWF_VERSION: u8 = 8;

/// One encoded tag: its code and its body, without the record header.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct RawTag {
    /// The SWF tag code.
    pub code: u16,
    /// The tag body.
    pub body: Vec<u8>,
}

impl RawTag {
    /// A `ShowFrame` tag.
    pub fn show_frame() -> RawTag {
        RawTag {
            code: TagCode::ShowFrame as u16,
            body: Vec::new(),
        }
    }

    /// Overwrites the character id at the start of a definition body
    /// (`DefineShape*`, `DefineSprite`, `DefineButton2`, `DefineBits*`,
    /// `DefineFont*`, `DefineText*`, `DefineEditText` all start with it).
    pub fn set_leading_id(&mut self, id: u16) {
        if self.body.len() >= 2 {
            self.body[..2].copy_from_slice(&id.to_le_bytes());
        }
    }
}

/// A short name for a tag, for messages.
pub fn tag_name(tag: &Tag<'_>) -> &'static str {
    match tag {
        Tag::ShowFrame => "ShowFrame",
        Tag::DefineShape(_) => "DefineShape",
        Tag::DefineSprite(_) => "DefineSprite",
        Tag::DefineButton2(_) => "DefineButton2",
        Tag::PlaceObject(_) => "PlaceObject",
        Tag::RemoveObject(_) => "RemoveObject2",
        Tag::DoAction(_) => "DoAction",
        Tag::DoInitAction { .. } => "DoInitAction",
        Tag::FrameLabel(_) => "FrameLabel",
        Tag::ExportAssets(_) => "ExportAssets",
        Tag::FileAttributes(_) => "FileAttributes",
        Tag::SetBackgroundColor(_) => "SetBackgroundColor",
        Tag::Protect(_) => "Protect",
        Tag::DefineBitsLossless(_) => "DefineBitsLossless",
        Tag::DefineBitsJpeg2 { .. } => "DefineBitsJPEG2",
        Tag::DefineBitsJpeg3(_) => "DefineBitsJPEG3",
        Tag::DefineFont2(_) => "DefineFont2/3",
        Tag::DefineFontAlignZones { .. } => "DefineFontAlignZones",
        Tag::DefineFontName { .. } => "DefineFontName",
        Tag::DefineText(_) => "DefineText",
        Tag::DefineText2(_) => "DefineText2",
        Tag::DefineEditText(_) => "DefineEditText",
        Tag::CsmTextSettings(_) => "CSMTextSettings",
        Tag::DefineScalingGrid { .. } => "DefineScalingGrid",
        _ => "tag",
    }
}

/// Encodes one tag and checks that it reads back equal (see the module
/// docs). `Tag::End` and `Tag::DefineSprite` are refused: the end tag is
/// implicit, and sprites are framed with [`sprite`].
pub fn encode(tag: &Tag<'_>) -> Result<RawTag, PublishError> {
    let name = tag_name(tag);
    if matches!(tag, Tag::End | Tag::DefineSprite(_)) {
        return Err(PublishError::OutOfRange(format!(
            "{name} must not be encoded directly"
        )));
    }
    let framed = write_single(tag).map_err(|source| PublishError::Encode { tag: name, source })?;
    let parsed = swf::read::Reader::new(&framed, SWF_VERSION)
        .read_tag()
        .map_err(|e| PublishError::RoundTrip {
            tag: name,
            detail: format!("cannot read it back: {e}"),
        })?;
    if !tags_equal(tag, &parsed) {
        return Err(PublishError::RoundTrip {
            tag: name,
            detail: difference(tag, &parsed),
        });
    }
    let (code, body) = split_record(&framed).ok_or_else(|| PublishError::RoundTrip {
        tag: name,
        detail: "malformed record header".to_owned(),
    })?;
    Ok(RawTag { code, body })
}

/// Frames a `DefineSprite` from already encoded child tags. The end tag is
/// appended here; `children` must not contain one.
pub fn sprite(id: u16, num_frames: u16, children: &[RawTag]) -> RawTag {
    let mut body = Vec::with_capacity(4 + children.iter().map(|t| t.body.len() + 6).sum::<usize>());
    body.extend_from_slice(&id.to_le_bytes());
    body.extend_from_slice(&num_frames.to_le_bytes());
    for t in children {
        frame_into(&mut body, t);
    }
    body.extend_from_slice(&[0, 0]); // End
    RawTag {
        code: TagCode::DefineSprite as u16,
        body,
    }
}

/// Writes a whole movie from raw root tags (without the end tag, which is
/// appended here).
pub fn write_movie(header: &Header, tags: &[RawTag]) -> Result<Vec<u8>, PublishError> {
    let mut stream = Vec::with_capacity(tags.iter().map(|t| t.body.len() + 6).sum::<usize>() + 2);
    for t in tags {
        frame_into(&mut stream, t);
    }
    stream.extend_from_slice(&[0, 0]); // End
    let mut out = Vec::new();
    swf::write::write_swf_raw_tags(header, &stream, &mut out).map_err(|source| {
        PublishError::Encode {
            tag: "movie",
            source,
        }
    })?;
    Ok(out)
}

/// Appends one tag with its record header.
fn frame_into(out: &mut Vec<u8>, t: &RawTag) {
    let len = t.body.len();
    let long = len >= 0x3f || wants_long_header(t.code);
    if long {
        out.extend_from_slice(&((t.code << 6) | 0x3f).to_le_bytes());
        // Bodies are far below 4 GiB (the movie itself is limited to that).
        out.extend_from_slice(&u32::try_from(len).unwrap_or(u32::MAX).to_le_bytes());
    } else {
        // `len < 0x3f` here, so it fits the 6-bit field.
        out.extend_from_slice(&((t.code << 6) | len as u16).to_le_bytes());
    }
    out.extend_from_slice(&t.body);
}

/// Flash writes long record headers for bitmap definitions even when they
/// are short, and some players expect it.
fn wants_long_header(code: u16) -> bool {
    [
        TagCode::DefineBits as u16,
        TagCode::DefineBitsJpeg2 as u16,
        TagCode::DefineBitsJpeg3 as u16,
        TagCode::DefineBitsLossless as u16,
        TagCode::DefineBitsLossless2 as u16,
    ]
    .contains(&code)
}

/// Serializes one tag (with its record header) through `write_swf`.
fn write_single(tag: &Tag<'_>) -> swf::error::Result<Vec<u8>> {
    let header = Header {
        compression: Compression::None,
        version: SWF_VERSION,
        stage_size: Rectangle {
            x_min: Twips::ZERO,
            x_max: Twips::ZERO,
            y_min: Twips::ZERO,
            y_max: Twips::ZERO,
        },
        frame_rate: swf::Fixed8::ONE,
        num_frames: 1,
    };
    let mut empty = Vec::new();
    swf::write_swf(&header, &[], &mut empty)?;
    let mut out = Vec::new();
    swf::write_swf(&header, std::slice::from_ref(tag), &mut out)?;
    // `empty` = prefix + End (2 bytes); `out` = prefix + tag + End.
    let start = empty.len().saturating_sub(2);
    let end = out.len().saturating_sub(2);
    Ok(out.get(start..end).map(<[u8]>::to_vec).unwrap_or_default())
}

/// Splits a framed record into its code and body.
pub fn split_record(framed: &[u8]) -> Option<(u16, Vec<u8>)> {
    let head = u16::from_le_bytes([*framed.first()?, *framed.get(1)?]);
    let code = head >> 6;
    let short = usize::from(head & 0x3f);
    let (len, start): (usize, usize) = if short == 0x3f {
        let l = u32::from_le_bytes(framed.get(2..6)?.try_into().ok()?);
        (usize::try_from(l).ok()?, 6)
    } else {
        (short, 2)
    };
    let body = framed.get(start..start.checked_add(len)?)?;
    Some((code, body.to_vec()))
}

/// Tag equality, ignoring what the reader synthesizes for shapes below
/// version 4 (it sets `edge_bounds = shape_bounds` and
/// `HAS_NON_SCALING_STROKES`; neither field is written for them).
fn tags_equal(a: &Tag<'_>, b: &Tag<'_>) -> bool {
    match (a, b) {
        (Tag::DefineShape(x), Tag::DefineShape(y)) if x.version < 4 => {
            x.version == y.version
                && x.id == y.id
                && x.shape_bounds == y.shape_bounds
                && x.styles == y.styles
                && x.shape == y.shape
        }
        _ => a == b,
    }
}

/// A bounded description of how two tags differ.
fn difference(a: &Tag<'_>, b: &Tag<'_>) -> String {
    let (x, y) = (format!("{a:?}"), format!("{b:?}"));
    let at = x
        .bytes()
        .zip(y.bytes())
        .position(|(p, q)| p != q)
        .unwrap_or(x.len().min(y.len()));
    let from = at.saturating_sub(40);
    let mut s = String::new();
    let snip = |t: &str| -> String { t.chars().skip(from).take(160).collect() };
    let _ = write!(s, "written ...{}... read back ...{}...", snip(&x), snip(&y));
    s
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn show_frame_round_trips() {
        let raw = encode(&Tag::ShowFrame).expect("encode");
        assert_eq!(raw, RawTag::show_frame());
    }

    #[test]
    fn long_bodies_and_bitmaps_get_long_headers() {
        let mut out = Vec::new();
        frame_into(
            &mut out,
            &RawTag {
                code: 12,
                body: vec![0; 3],
            },
        );
        assert_eq!(out.len(), 5);
        out.clear();
        frame_into(
            &mut out,
            &RawTag {
                code: TagCode::DefineBitsJpeg2 as u16,
                body: vec![0; 3],
            },
        );
        assert_eq!(out.len(), 9);
        assert_eq!(split_record(&out), Some((21, vec![0; 3])));
    }

    #[test]
    fn end_and_sprite_are_refused() {
        assert!(encode(&Tag::End).is_err());
    }

    #[test]
    fn sprite_framing_reads_back() {
        let child = encode(&Tag::DoAction(&[0x07, 0x00])).expect("encode");
        let sprite = sprite(5, 1, &[child, RawTag::show_frame()]);
        let mut framed = Vec::new();
        frame_into(&mut framed, &sprite);
        let tag = swf::read::Reader::new(&framed, SWF_VERSION)
            .read_tag()
            .expect("read");
        match tag {
            Tag::DefineSprite(s) => {
                assert_eq!(s.id, 5);
                assert_eq!(s.num_frames, 1);
                assert_eq!(s.tags, vec![Tag::DoAction(&[0x07, 0x00]), Tag::ShowFrame]);
            }
            other => panic!("unexpected {other:?}"),
        }
    }

    #[test]
    fn every_emitted_tag_type_round_trips() {
        use swf::{
            BlendMode, Button, ButtonAction, ButtonActionCondition, ButtonRecord, ButtonState,
            Color, ColorTransform, ExportedAsset, FileAttributes, FrameLabel, Matrix, RemoveObject,
        };
        let actions = [0x07u8, 0x00];
        let button = Button {
            id: 3,
            is_track_as_menu: true,
            records: vec![ButtonRecord {
                states: ButtonState::UP | ButtonState::HIT_TEST,
                id: 1,
                depth: 1,
                matrix: Matrix::IDENTITY,
                color_transform: ColorTransform::IDENTITY,
                filters: vec![swf::Filter::BlurFilter(Box::new(swf::BlurFilter {
                    blur_x: swf::Fixed16::ONE,
                    blur_y: swf::Fixed16::ONE,
                    flags: swf::BlurFilterFlags::from_passes(1),
                }))],
                blend_mode: BlendMode::Multiply,
            }],
            actions: vec![
                ButtonAction {
                    conditions: ButtonActionCondition::OVER_DOWN_TO_OVER_UP,
                    action_data: &actions,
                },
                ButtonAction {
                    conditions: ButtonActionCondition::from_key_code(13),
                    action_data: &actions,
                },
            ],
        };
        let all = [
            Tag::FileAttributes(FileAttributes::USE_NETWORK_SANDBOX),
            Tag::SetBackgroundColor(Color::from_rgb(0xE0E0E0, 255)),
            Tag::Protect(None),
            Tag::DoAction(&actions),
            Tag::DoInitAction {
                id: 4,
                action_data: &actions,
            },
            Tag::FrameLabel(FrameLabel {
                label: "حلقة".into(),
                is_anchor: false,
            }),
            Tag::FrameLabel(FrameLabel {
                label: "anchor".into(),
                is_anchor: true,
            }),
            Tag::ExportAssets(vec![ExportedAsset {
                id: 4,
                name: "__Packages.a.B".into(),
            }]),
            Tag::RemoveObject(RemoveObject {
                depth: 16383,
                character_id: None,
            }),
            Tag::DefineButton2(Box::new(button)),
        ];
        for t in &all {
            encode(t).unwrap_or_else(|e| panic!("{}: {e}", tag_name(t)));
        }
    }

    #[test]
    fn silent_truncations_are_caught() {
        use swf::{LineStyle, ShapeFlag, ShapeStyles};
        // The writer stores line widths as u16 twips.
        let shape = swf::Shape {
            version: 4,
            id: 1,
            shape_bounds: Rectangle::ZERO,
            edge_bounds: Rectangle::ZERO,
            flags: ShapeFlag::HAS_SCALING_STROKES,
            styles: ShapeStyles {
                fill_styles: vec![],
                line_styles: vec![LineStyle::new().with_width(Twips::new(70_000))],
            },
            shape: vec![],
        };
        let err = encode(&Tag::DefineShape(Box::new(shape))).expect_err("truncated");
        assert!(matches!(err, PublishError::RoundTrip { .. }), "{err}");
    }

    #[test]
    fn leading_id_is_patched() {
        let mut t = RawTag {
            code: 2,
            body: vec![0, 0, 9],
        };
        t.set_leading_id(0x1234);
        assert_eq!(t.body, vec![0x34, 0x12, 9]);
    }
}
