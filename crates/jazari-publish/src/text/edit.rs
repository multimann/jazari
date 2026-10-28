//! Dynamic and input text: DefineEditText with a device font.
//!
//! Calibrated on the `text` and `as2` fixtures:
//!
//! - bounds are the XFL box grown by Flash's 2 px gutter on every side;
//! - the font is a device font named after the first run's family, with the
//!   first run's size and colour; the layout (alignment, margins, indent,
//!   leading) comes from the first paragraph;
//! - the flags are `HAS_TEXT`, `HAS_TEXT_COLOR`, `HAS_FONT`, `HAS_LAYOUT`
//!   and `HTML`, plus `READ_ONLY` for dynamic text, `NO_SELECT`, `BORDER`,
//!   `MULTILINE`, `WORD_WRAP`, `PASSWORD` and `HAS_MAX_LENGTH` from the
//!   field's properties. `autoExpand` sets no flag (Flash 8 sets none);
//!   outlines are never used, so the text is device text.
//! - CSMTextSettings follows, except for Flash's `_sans`, `_serif` and
//!   `_typewriter` (pixel grid fit for left-aligned text, sub-pixel
//!   otherwise), as a probe of Flash 8 showed;
//! - input text drops its links, as Flash 8 does.

use jazari_as2::Origin;
use jazari_text::FontLibrary;
use jazari_xfl::model::{Align, LineType, TextAttrs, TextElement, TextKind};
use swf::{
    CharacterId, CsmTextSettings, EditText, Rectangle, SwfStr, Tag, TextAlign, TextGridFit,
    TextLayout, Twips,
};

use super::html::{self, DeviceFace};
use super::{define_shared, excerpt, font, is_arabic};
use crate::context::{CharKind, Ctx};
use crate::tags;
use crate::units::{self, round_clamped};

/// Flash's gutter around a text field's box, in pixels.
const GUTTER: f64 = 2.0;

/// Defines the DefineEditText of a dynamic or input text element.
pub fn define(ctx: &mut Ctx<'_>, el: &TextElement, origin: &Origin) -> Option<CharacterId> {
    let attrs = el.runs.first().map(|r| r.attrs.clone()).unwrap_or_default();
    let lib = ctx.fonts;
    let device = |face: &str| device_face(lib, face);
    let (family, bold, italic) = device(&attrs.face);
    let font = font::device_font(ctx, &family, bold, italic, origin)?;

    let text: String = el.runs.iter().map(|r| r.text.as_str()).collect();
    let arabic: String = text.chars().filter(|&c| is_arabic(c)).take(24).collect();
    if !arabic.is_empty() {
        let kind = if el.kind == TextKind::Input {
            "input"
        } else {
            "dynamic"
        };
        ctx.warn(
            origin.clone(),
            format!(
                "Arabic in {kind} text ({}) is not shaped by Flash Player 8 or Ruffle: it shows as separate letters, left to right; use static text for Arabic",
                excerpt(&arabic)
            ),
        );
    }
    let links = el
        .runs
        .iter()
        .any(|r| r.attrs.url.as_deref().is_some_and(|u| !u.is_empty()));
    if links && el.kind == TextKind::Input {
        ctx.info(
            origin.clone(),
            "links in input text are dropped, as Flash 8 drops them",
        );
    }
    let html = html::field_html(&el.runs, &device, el.kind != TextKind::Input);
    let initial = (!el.runs.is_empty()).then_some(html.as_str());

    let height = clamp_twips(
        ctx,
        origin,
        "font size",
        attrs.size,
        0.0,
        f64::from(u16::MAX),
    );
    let layout = field_layout(ctx, origin, &attrs);
    let max_length = match el.max_chars {
        Some(n) if el.kind == TextKind::Input => {
            if n > u32::from(u16::MAX) {
                ctx.warn(
                    origin.clone(),
                    format!("maximum characters {n} was reduced to 65535"),
                );
            }
            Some(u16::try_from(n).unwrap_or(u16::MAX))
        }
        _ => None,
    };
    let variable = el.variable.as_deref().unwrap_or("").replace('\0', "");
    let edit = EditText::new()
        .with_bounds(field_bounds(el))
        .with_font_id(font, height)
        .with_color(Some(units::color(attrs.fill)))
        .with_max_length(max_length)
        .with_layout(Some(layout))
        .with_variable_name(SwfStr::from_bytes(variable.as_bytes()))
        .with_initial_text(initial.map(|s| SwfStr::from_bytes(s.as_bytes())))
        .with_is_read_only(el.kind == TextKind::Dynamic)
        .with_is_selectable(el.selectable)
        .with_has_border(el.border)
        .with_is_html(true)
        .with_is_multiline(matches!(
            el.line_type,
            LineType::Multiline | LineType::MultilineNoWrap
        ))
        .with_is_word_wrap(el.line_type == LineType::Multiline)
        .with_is_password(el.line_type == LineType::Password);
    // Flash 8 follows a field with CSMTextSettings unless its font is one
    // of the `_sans`, `_serif` and `_typewriter` device names: advanced
    // anti-aliasing, fitted to pixels when left-aligned and to sub-pixels
    // otherwise.
    let csm = (!family.starts_with('_')).then(|| {
        Tag::CsmTextSettings(CsmTextSettings {
            id: 0,
            use_advanced_rendering: true,
            grid_fit: if attrs.align == Align::Left {
                TextGridFit::Pixel
            } else {
                TextGridFit::SubPixel
            },
            thickness: 0.0,
            sharpness: 0.0,
        })
    });
    let encoded = tags::encode(&Tag::DefineEditText(Box::new(edit))).and_then(|t| {
        let mut out = vec![t];
        if let Some(c) = &csm {
            out.push(tags::encode(c)?);
        }
        Ok(out)
    });
    match encoded {
        Ok(raw) => define_shared(ctx, CharKind::EditText, raw, vec![font], origin),
        Err(e) => {
            ctx.error(origin.clone(), format!("text field left out: {e}"));
            None
        }
    }
}

/// The field's bounds: the XFL box plus the 2 px gutter on every side.
fn field_bounds(el: &TextElement) -> Rectangle<Twips> {
    let w = if el.width.is_finite() {
        el.width.max(0.0)
    } else {
        0.0
    };
    let h = if el.height.is_finite() {
        el.height.max(0.0)
    } else {
        0.0
    };
    Rectangle {
        x_min: units::px_to_twips(el.left - GUTTER),
        x_max: units::px_to_twips(el.left + w + GUTTER),
        y_min: units::px_to_twips(el.top - GUTTER),
        y_max: units::px_to_twips(el.top + h + GUTTER),
    }
}

/// The field layout from the first paragraph's attributes.
fn field_layout(ctx: &mut Ctx<'_>, origin: &Origin, a: &TextAttrs) -> TextLayout {
    let (u16_max, i16_min, i16_max) = (
        f64::from(u16::MAX),
        f64::from(i16::MIN),
        f64::from(i16::MAX),
    );
    TextLayout {
        align: match a.align {
            Align::Left => TextAlign::Left,
            Align::Right => TextAlign::Right,
            Align::Center => TextAlign::Center,
            Align::Justify => TextAlign::Justify,
        },
        left_margin: clamp_twips(ctx, origin, "left margin", a.left_margin, 0.0, u16_max),
        right_margin: clamp_twips(ctx, origin, "right margin", a.right_margin, 0.0, u16_max),
        indent: clamp_twips(ctx, origin, "indent", a.indent, i16_min, i16_max),
        leading: clamp_twips(
            ctx,
            origin,
            "line spacing",
            a.line_spacing,
            i16_min,
            i16_max,
        ),
    }
}

/// Points to twips within `lo..=hi` twips, with a warning when clamped.
fn clamp_twips(ctx: &mut Ctx<'_>, origin: &Origin, what: &str, pt: f64, lo: f64, hi: f64) -> Twips {
    let v = if pt.is_finite() { pt * 20.0 } else { 0.0 };
    if v.round() < lo || v.round() > hi {
        ctx.warn(
            origin.clone(),
            format!(
                "{what} {pt} pt is outside {}..{} pt and was clamped",
                lo / 20.0,
                hi / 20.0
            ),
        );
    }
    Twips::new(round_clamped(v.clamp(lo, hi), hi.abs().max(lo.abs())))
}

/// The device face for an XFL face name: Flash's `_sans`, `_serif` and
/// `_typewriter` as they are; an installed face by its family name and
/// style; otherwise the name with PostScript style suffixes removed
/// (`Arial-BoldMT` becomes `Arial`, bold). An empty name is `_sans`.
pub fn device_face(lib: &FontLibrary, face: &str) -> DeviceFace {
    let face = face.trim();
    if face.is_empty() {
        return ("_sans".to_owned(), false, false);
    }
    if face.starts_with('_') {
        return (face.to_owned(), false, false);
    }
    if let Some(info) = lib
        .lookup(face)
        .and_then(|id| lib.face_info(id))
        .filter(|i| !i.family.is_empty())
    {
        return (info.family, info.bold, info.italic);
    }
    let (base, style) = face.split_once('-').unwrap_or((face, ""));
    let style = style.to_ascii_lowercase();
    let lower = face.to_ascii_lowercase();
    let bold = style.contains("bold") || (style.is_empty() && lower.ends_with(" bold"));
    let italic = style.contains("italic") || style.contains("oblique");
    let base = base
        .strip_suffix("PSMT")
        .or_else(|| base.strip_suffix("MT"))
        .unwrap_or(base)
        .trim_end_matches("PS");
    let base = if base.is_empty() { face } else { base };
    (base.to_owned(), bold, italic)
}
