//! One font face: its data in memory, metrics, shaping caches and codes.

use std::sync::OnceLock;

use skrifa::instance::{LocationRef, Size};
use skrifa::raw::TableProvider;
use skrifa::raw::tables::os2::SelectionFlags;
use skrifa::raw::types::Tag;
use skrifa::{FontRef, MetadataProvider};

use crate::library::FontOrigin;

/// Vertical metrics of a face, in ems (1.0 = one em).
///
/// Ascent and descent are both positive distances from the baseline. They come
/// from, in order:
///
/// 1. the OS/2 typographic ascender and descender when the font sets
///    `USE_TYPO_METRICS` (modern fonts such as the embedded Noto fonts, whose
///    Windows metrics are padded for clipping, not for line spacing);
/// 2. the OS/2 `usWinAscent`/`usWinDescent`, which is what GDI, and so
///    Flash 8 on Windows, used: the `text` oracle fixture puts the first
///    baseline of Arial, Times New Roman and Tahoma text at
///    `round(winAscent × size)` and spaces lines by
///    `winAscent + winDescent + lineSpacing`, without the line gap;
/// 3. `hhea`, for fonts without an OS/2 table.
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct FontMetrics {
    /// The face's design units per em.
    pub units_per_em: u16,
    /// Distance from the baseline up to the ascender line (positive).
    pub ascent: f64,
    /// Distance from the baseline down to the descender line (positive).
    pub descent: f64,
    /// The font's recommended line gap (`hhea` or OS/2 typo). Flash does not
    /// add it between lines; it is reported for completeness.
    pub leading: f64,
}

/// Facts about a face that the publisher needs for DefineFont3 and reports.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FaceInfo {
    /// The first (English) family name.
    pub family: String,
    /// The PostScript name (name ID 6), possibly empty.
    pub post_script_name: String,
    /// Weight class 600 or heavier.
    pub bold: bool,
    /// Italic or oblique.
    pub italic: bool,
    /// Where the face was found.
    pub origin: FontOrigin,
    /// OS/2 `fsType` embedding permissions (0 when the table is missing).
    pub fs_type: u16,
}

impl FaceInfo {
    /// Whether the licence bits allow embedding the outlines in a SWF:
    /// neither "restricted licence" nor "bitmap embedding only".
    pub fn embedding_allowed(&self) -> bool {
        let permissions = self.fs_type & 0x000F;
        permissions != 0x0002 && self.fs_type & 0x0200 == 0
    }

    /// Whether the licence allows embedding a subset of the glyphs (fsType
    /// bit 8, "no subsetting", is clear). DefineFont3 only holds the glyphs
    /// a document uses, so a font that forbids subsetting deserves a warning
    /// even when [`embedding_allowed`](Self::embedding_allowed) is true.
    pub fn subsetting_allowed(&self) -> bool {
        self.fs_type & 0x0100 == 0
    }
}

/// Font bytes: borrowed from the executable, or copied from a file.
pub(crate) enum FaceData {
    Static(&'static [u8]),
    Owned(Vec<u8>),
}

impl FaceData {
    pub(crate) fn bytes(&self) -> &[u8] {
        match self {
            Self::Static(b) => b,
            Self::Owned(v) => v,
        }
    }
}

/// A face whose data is in memory, with its shaping caches.
pub(crate) struct LoadedFace {
    data: FaceData,
    index: u32,
    shaper_data: harfrust::ShaperData,
    pub(crate) metrics: FontMetrics,
    pub(crate) glyph_count: u16,
    pub(crate) fs_type: u16,
    /// GSUB has an `arab` script, so harfrust can join Arabic with it.
    arabic_shaping: bool,
    codes: OnceLock<Vec<u16>>,
}

impl LoadedFace {
    pub(crate) fn load(data: FaceData, index: u32) -> Option<Self> {
        let font = FontRef::from_index(data.bytes(), index).ok()?;
        let metrics = read_metrics(&font);
        let glyph_count = font.maxp().map(|m| m.num_glyphs()).unwrap_or(0);
        let fs_type = font.os2().map(|os2| os2.fs_type()).unwrap_or(0);
        let arabic_shaping = has_gsub_script(&font, Tag::new(b"arab"));
        let shaper_data = harfrust::ShaperData::new(&font);
        Some(Self {
            data,
            index,
            shaper_data,
            metrics,
            glyph_count,
            fs_type,
            arabic_shaping,
            codes: OnceLock::new(),
        })
    }

    /// The parsed font. Loading validated the data, so this only fails if
    /// the bytes changed, which they cannot.
    pub fn font(&self) -> Option<FontRef<'_>> {
        FontRef::from_index(self.data.bytes(), self.index).ok()
    }

    /// harfrust's per-font caches.
    pub fn shaper_data(&self) -> &harfrust::ShaperData {
        &self.shaper_data
    }

    /// Vertical metrics in ems.
    pub fn metrics(&self) -> FontMetrics {
        self.metrics
    }

    /// Whether harfrust can join Arabic with this face.
    pub fn arabic_shaping(&self) -> bool {
        self.arabic_shaping
    }

    pub(crate) fn code(&self, glyph: u16) -> u16 {
        let table = self.codes.get_or_init(|| match self.font() {
            Some(font) => crate::codes::build_code_table(&font, self.glyph_count),
            None => Vec::new(),
        });
        table.get(usize::from(glyph)).copied().unwrap_or(0xFFFD)
    }
}

/// Reads ascent and descent (see [`FontMetrics`] for the sources).
fn read_metrics(font: &FontRef<'_>) -> FontMetrics {
    let fallback = font.metrics(Size::unscaled(), LocationRef::default());
    let upem = f64::from(fallback.units_per_em.max(1));
    let os2 = font.os2().ok();
    let typo = os2.as_ref().filter(|os2| {
        os2.fs_selection()
            .contains(SelectionFlags::USE_TYPO_METRICS)
    });
    let (mut ascent, mut descent) = match (typo, &os2) {
        (Some(os2), _) => (
            f64::from(os2.s_typo_ascender()),
            -f64::from(os2.s_typo_descender()),
        ),
        (None, Some(os2))
            if u32::from(os2.us_win_ascent()) + u32::from(os2.us_win_descent()) > 0 =>
        {
            (
                f64::from(os2.us_win_ascent()),
                f64::from(os2.us_win_descent()),
            )
        }
        _ => match font.hhea() {
            Ok(hhea) => (
                f64::from(hhea.ascender().to_i16()),
                -f64::from(hhea.descender().to_i16()),
            ),
            Err(_) => (f64::from(fallback.ascent), -f64::from(fallback.descent)),
        },
    };
    if ascent + descent <= 0.0 {
        ascent = 0.8 * upem;
        descent = 0.2 * upem;
    }
    FontMetrics {
        units_per_em: fallback.units_per_em.max(1),
        ascent: ascent / upem,
        descent: descent / upem,
        leading: f64::from(fallback.leading) / upem,
    }
}

/// Whether the font's GSUB lists the given script.
fn has_gsub_script(font: &FontRef<'_>, script: Tag) -> bool {
    font.gsub()
        .and_then(|gsub| gsub.script_list())
        .map(|list| {
            list.script_records()
                .iter()
                .any(|r| r.script_tag() == script)
        })
        .unwrap_or(false)
}
