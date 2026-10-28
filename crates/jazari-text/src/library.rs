//! The font library: discovery, name resolution, metrics, outlines and codes.

use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex, OnceLock};

use jazari_xfl::model::PathCmd;
use skrifa::instance::{LocationRef, Size};
use skrifa::outline::DrawSettings;
use skrifa::{GlyphId, MetadataProvider};

use crate::embedded;
use crate::error::Error;
pub(crate) use crate::face::{FaceData, LoadedFace};
pub use crate::face::{FaceInfo, FontMetrics};
use crate::names::{self, FallbackClass, StyleHint};
use crate::outline::EmPen;

/// Identifies a font face loaded in a [`FontLibrary`].
///
/// Ids are indices into one library and are stable for its lifetime (adding
/// fonts only appends). They mean nothing to another library.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct FontId(pub u32);

/// Where a face was found. Earlier variants win when two faces carry the
/// same name, so a document can override an installed font.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum FontOrigin {
    /// The `fonts/` folder next to the document.
    Document,
    /// The `fonts/` folder next to the executable, or another folder added
    /// through [`FontLibraryOptions::dirs`].
    Folder,
    /// The operating system's fonts (fontdb scan).
    System,
    /// The fonts compiled into the program ([`crate::embedded`]).
    Embedded,
}

/// How [`FontLibrary::resolve_detailed`] found a face.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum FaceMatch {
    /// The face name is a PostScript name of an available face.
    PostScriptName,
    /// The face name matched a family name (possibly after removing style
    /// words such as `Bold` or `-BoldMT`).
    FamilyName,
    /// Nothing matched: an embedded font stands in.
    Fallback,
}

/// The result of resolving a face name.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct Resolution {
    /// The face to use.
    pub font: FontId,
    /// How it was found. Anything but [`FaceMatch::PostScriptName`] and
    /// [`FaceMatch::FamilyName`] is a substitution worth reporting.
    pub matched: FaceMatch,
}

/// The ids of the embedded fallback fonts in a library.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct EmbeddedFonts {
    /// Noto Sans Arabic: Arabic text in sans-serif faces.
    pub sans_arabic: FontId,
    /// Noto Naskh Arabic: Arabic text in Naskh-like faces.
    pub naskh_arabic: FontId,
    /// Noto Sans: Latin and common punctuation.
    pub sans: FontId,
}

/// What [`FontLibrary::with_options`] scans.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FontLibraryOptions {
    /// Scan the operating system's font folders.
    pub system_fonts: bool,
    /// Load the `fonts/` folder next to the executable, if there is one.
    pub exe_fonts_dir: bool,
    /// More folders to load (recursively), with [`FontOrigin::Folder`].
    pub dirs: Vec<PathBuf>,
}

impl Default for FontLibraryOptions {
    /// System fonts and the executable's `fonts/` folder.
    fn default() -> Self {
        Self {
            system_fonts: true,
            exe_fonts_dir: true,
            dirs: Vec::new(),
        }
    }
}

/// The per-character fallback chain for one XFL face name.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct FaceChain {
    /// The face the name resolved to, if any.
    pub primary: Option<FontId>,
    /// Arabic fallbacks, the one matching the face's style first.
    pub arabic: [FontId; 2],
    /// The Latin and common-punctuation fallback.
    pub latin: FontId,
}

impl FaceChain {
    /// The face used when nothing covers a character.
    pub fn first(&self) -> FontId {
        self.primary.unwrap_or(self.latin)
    }
}

/// One face known to the library (its data is loaded on first use).
struct Slot {
    db_id: fontdb::ID,
    origin: FontOrigin,
    families: Vec<String>,
    normalized_families: Vec<String>,
    post_script_name: String,
    weight: u16,
    italic: bool,
    static_data: Option<&'static [u8]>,
    face: OnceLock<Option<LoadedFace>>,
}

/// All fonts available for layout: the system fonts (fontdb scan), `fonts/`
/// folders next to the executable and the document, and the embedded Noto
/// fallbacks.
///
/// Face data is read lazily, on first use, and copied into memory (no file
/// mapping stays open). The library is `Send + Sync`; its caches use interior
/// mutability, so `&FontLibrary` is enough for layout.
pub struct FontLibrary {
    db: fontdb::Database,
    slots: Vec<Slot>,
    known: HashSet<fontdb::ID>,
    embedded: EmbeddedFonts,
    resolved: Mutex<HashMap<String, Resolution>>,
}

impl std::fmt::Debug for FontLibrary {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("FontLibrary")
            .field("faces", &self.slots.len())
            .field("embedded", &self.embedded)
            .finish_non_exhaustive()
    }
}

impl Default for FontLibrary {
    fn default() -> Self {
        Self::new()
    }
}

impl FontLibrary {
    /// Scans the system fonts and the executable's `fonts/` folder, and
    /// loads the embedded fallbacks. The scan reads every font file's name
    /// table, which takes a moment: create one library and share it.
    pub fn new() -> Self {
        Self::with_options(&FontLibraryOptions::default())
    }

    /// Only the embedded fonts: deterministic on every machine (tests,
    /// reproducible output).
    pub fn embedded_only() -> Self {
        Self::with_options(&FontLibraryOptions {
            system_fonts: false,
            exe_fonts_dir: false,
            dirs: Vec::new(),
        })
    }

    /// Loads what `opts` asks for, plus the embedded fonts.
    pub fn with_options(opts: &FontLibraryOptions) -> Self {
        let mut db = fontdb::Database::new();
        let mut ids = [None; 3];
        for (slot, (_, data)) in ids.iter_mut().zip(embedded::ALL) {
            let source = fontdb::Source::Binary(Arc::new(data));
            *slot = db.load_font_source(source).first().copied();
        }
        let mut lib = Self {
            db,
            slots: Vec::new(),
            known: HashSet::new(),
            embedded: EmbeddedFonts {
                sans_arabic: FontId(0),
                naskh_arabic: FontId(0),
                sans: FontId(0),
            },
            resolved: Mutex::new(HashMap::new()),
        };
        // The embedded fonts are known-good, but stay defensive: a face
        // fontdb failed to parse leaves a slot pointing at the first one.
        let mut embedded_ids = [FontId(0); 3];
        for ((id, (_, data)), out) in ids.iter().zip(embedded::ALL).zip(embedded_ids.iter_mut()) {
            if let Some(id) = id {
                *out = lib.push_slot(*id, FontOrigin::Embedded, Some(data));
            }
        }
        lib.embedded = EmbeddedFonts {
            sans_arabic: embedded_ids[0],
            naskh_arabic: embedded_ids[1],
            sans: embedded_ids[2],
        };
        if opts.system_fonts {
            lib.db.load_system_fonts();
            lib.adopt_new_faces(FontOrigin::System);
        }
        if opts.exe_fonts_dir
            && let Some(dir) = exe_fonts_dir()
        {
            lib.add_fonts_dir(&dir, FontOrigin::Folder);
        }
        for dir in &opts.dirs {
            lib.add_fonts_dir(dir, FontOrigin::Folder);
        }
        lib
    }

    /// Loads the `fonts/` folder next to a document (`doc_dir/fonts`), whose
    /// faces then win over installed ones. Returns how many faces were added.
    pub fn add_document_fonts(&mut self, doc_dir: &Path) -> usize {
        self.add_fonts_dir(&doc_dir.join("fonts"), FontOrigin::Document)
    }

    /// Loads every font file under `dir` (recursively). A missing folder adds
    /// nothing. Returns how many faces were added.
    pub fn add_fonts_dir(&mut self, dir: &Path, origin: FontOrigin) -> usize {
        if !dir.is_dir() {
            return 0;
        }
        self.db.load_fonts_dir(dir);
        self.adopt_new_faces(origin)
    }

    /// Loads one font file (all faces of a collection) and returns the new
    /// faces' ids.
    ///
    /// # Errors
    ///
    /// [`Error::Io`] if the file cannot be read, [`Error::NoFaces`] if no
    /// face in it parses.
    pub fn add_font_file(&mut self, path: &Path, origin: FontOrigin) -> Result<Vec<FontId>, Error> {
        let data = std::fs::read(path).map_err(|source| Error::Io {
            path: path.to_owned(),
            source,
        })?;
        self.add_font_data(data, origin)
            .map_err(|_| Error::NoFaces {
                what: path.display().to_string(),
            })
    }

    /// Loads a font from memory (all faces of a collection) and returns the
    /// new faces' ids.
    ///
    /// # Errors
    ///
    /// [`Error::NoFaces`] if no face in the data parses.
    pub fn add_font_data(
        &mut self,
        data: Vec<u8>,
        origin: FontOrigin,
    ) -> Result<Vec<FontId>, Error> {
        let ids = self
            .db
            .load_font_source(fontdb::Source::Binary(Arc::new(data)));
        if ids.is_empty() {
            return Err(Error::NoFaces {
                what: "font data".to_owned(),
            });
        }
        let first = self.slots.len();
        self.adopt_new_faces(origin);
        Ok((first..self.slots.len())
            .filter_map(|i| u32::try_from(i).ok())
            .map(FontId)
            .collect())
    }

    /// Number of faces known to the library.
    pub fn len(&self) -> usize {
        self.slots.len()
    }

    /// Whether the library has no face (never true: the embedded fonts are
    /// always there).
    pub fn is_empty(&self) -> bool {
        self.slots.is_empty()
    }

    /// The embedded fallback fonts.
    pub fn embedded(&self) -> EmbeddedFonts {
        self.embedded
    }

    /// Resolves an XFL face name: PostScript name first, then family name
    /// (Flash's device names `_sans`, `_serif` and `_typewriter` stand for
    /// Arial, Times New Roman and Courier New, or a similar installed
    /// family), then the embedded fallbacks (Noto Naskh Arabic for Naskh-like names
    /// such as `Traditional Arabic`, Noto Sans Arabic for other Arabic names,
    /// Noto Sans for everything else). Never fails.
    ///
    /// Layout does more than this: characters the resolved face lacks fall
    /// back character by character (see [`layout`](crate::layout())).
    pub fn resolve(&self, face: &str) -> FontId {
        self.resolve_detailed(face).font
    }

    /// Like [`resolve`](Self::resolve), and says whether a fallback was used.
    pub fn resolve_detailed(&self, face: &str) -> Resolution {
        let mut cache = self.resolved.lock().unwrap_or_else(|e| e.into_inner());
        if let Some(r) = cache.get(face) {
            return *r;
        }
        let r = match self.find(face) {
            Some((font, matched)) => Resolution { font, matched },
            None => Resolution {
                font: self.fallback_for(face),
                matched: FaceMatch::Fallback,
            },
        };
        cache.insert(face.to_owned(), r);
        r
    }

    /// The face a name matches, without any fallback.
    pub fn lookup(&self, face: &str) -> Option<FontId> {
        match self.resolve_detailed(face) {
            Resolution {
                matched: FaceMatch::Fallback,
                ..
            } => None,
            r => Some(r.font),
        }
    }

    /// The outline of a glyph in ems (1.0 = one em), y down, quadratic
    /// segments only (CFF cubics are approximated with kurbo). Every contour
    /// starts with `MoveTo` and ends on its start point. Empty for spaces,
    /// unknown fonts and unknown glyphs.
    ///
    /// For DefineFont3, multiply by 20480 (its units per em).
    pub fn glyph_outline(&self, font: FontId, glyph: u16) -> Vec<PathCmd> {
        let Some(face) = self.face(font) else {
            return Vec::new();
        };
        let Some(font_ref) = face.font() else {
            return Vec::new();
        };
        let Some(outline) = font_ref
            .outline_glyphs()
            .get(GlyphId::new(u32::from(glyph)))
        else {
            return Vec::new();
        };
        let mut pen = EmPen::new(face.metrics.units_per_em);
        let settings = DrawSettings::unhinted(Size::unscaled(), LocationRef::default());
        if let Err(e) = outline.draw(settings, &mut pen) {
            log::warn!("glyph {glyph} of font {}: {e}", font.0);
            return Vec::new();
        }
        pen.finish()
    }

    /// The nominal (unshaped) advance of a glyph in ems, from `hmtx`.
    /// Layout uses the shaper's advances; this is for DefineFont3's advance
    /// table.
    pub fn glyph_advance(&self, font: FontId, glyph: u16) -> f64 {
        let Some(face) = self.face(font) else {
            return 0.0;
        };
        let Some(font_ref) = face.font() else {
            return 0.0;
        };
        let upem = f64::from(face.metrics.units_per_em);
        font_ref
            .glyph_metrics(Size::unscaled(), LocationRef::default())
            .advance_width(GlyphId::new(u32::from(glyph)))
            .map_or(0.0, |a| f64::from(a) / upem)
    }

    /// The DefineFont3 code of a glyph: its cmap code (a base code point
    /// before an Arabic Presentation Form), else a Private Use code. Unique
    /// within the font and independent of the text, so codes from different
    /// layouts can be merged into one code table.
    pub fn glyph_code(&self, font: FontId, glyph: u16) -> u16 {
        self.face(font).map_or(0xFFFD, |f| f.code(glyph))
    }

    /// Vertical metrics of a face (see [`FontMetrics`] for the source).
    pub fn metrics(&self, font: FontId) -> FontMetrics {
        self.face(font).map(|f| f.metrics).unwrap_or(FontMetrics {
            units_per_em: 1000,
            ascent: 0.8,
            descent: 0.2,
            leading: 0.0,
        })
    }

    /// Names, style, origin and embedding bits of a face.
    pub fn face_info(&self, font: FontId) -> Option<FaceInfo> {
        let slot = self.slot(font)?;
        let fs_type = self.face(font).map_or(0, |f| f.fs_type);
        Some(FaceInfo {
            family: slot.families.first().cloned().unwrap_or_default(),
            post_script_name: slot.post_script_name.clone(),
            bold: slot.weight >= 600,
            italic: slot.italic,
            origin: slot.origin,
            fs_type,
        })
    }

    /// The per-character fallback chain of a face name.
    pub(crate) fn chain(&self, face: &str) -> FaceChain {
        let primary = self.lookup(face);
        let e = self.embedded;
        let arabic = match names::classify(face) {
            FallbackClass::Naskh => [e.naskh_arabic, e.sans_arabic],
            FallbackClass::SansArabic | FallbackClass::Latin => [e.sans_arabic, e.naskh_arabic],
        };
        FaceChain {
            primary,
            arabic,
            latin: e.sans,
        }
    }

    /// The loaded face, reading its data on first use. `None` for unknown ids
    /// and unreadable files.
    pub(crate) fn face(&self, font: FontId) -> Option<&LoadedFace> {
        let slot = self.slot(font)?;
        slot.face
            .get_or_init(|| {
                let data = match slot.static_data {
                    Some(bytes) => FaceData::Static(bytes),
                    None => FaceData::Owned(self.db.with_face_data(slot.db_id, |d, _| d.to_vec())?),
                };
                let index = self.db.face(slot.db_id).map_or(0, |f| f.index);
                let loaded = LoadedFace::load(data, index);
                if loaded.is_none() {
                    log::warn!(
                        "cannot load font face {:?} ({})",
                        slot.post_script_name,
                        font.0
                    );
                }
                loaded
            })
            .as_ref()
    }

    fn slot(&self, font: FontId) -> Option<&Slot> {
        self.slots.get(usize::try_from(font.0).ok()?)
    }

    /// Adds a slot for every fontdb face not seen yet.
    fn adopt_new_faces(&mut self, origin: FontOrigin) -> usize {
        let new: Vec<fontdb::ID> = self
            .db
            .faces()
            .map(|f| f.id)
            .filter(|id| !self.known.contains(id))
            .collect();
        for id in &new {
            self.push_slot(*id, origin, None);
        }
        if !new.is_empty() {
            self.resolved
                .get_mut()
                .unwrap_or_else(|e| e.into_inner())
                .clear();
        }
        new.len()
    }

    fn push_slot(
        &mut self,
        id: fontdb::ID,
        origin: FontOrigin,
        static_data: Option<&'static [u8]>,
    ) -> FontId {
        self.known.insert(id);
        let (families, post_script_name, weight, italic) = match self.db.face(id) {
            Some(info) => (
                info.families
                    .iter()
                    .map(|(name, _)| name.clone())
                    .collect::<Vec<_>>(),
                info.post_script_name.clone(),
                info.weight.0,
                info.style != fontdb::Style::Normal,
            ),
            None => (Vec::new(), String::new(), 400, false),
        };
        let normalized_families = families.iter().map(|f| names::normalize(f)).collect();
        let font = FontId(u32::try_from(self.slots.len()).unwrap_or(u32::MAX));
        self.slots.push(Slot {
            db_id: id,
            origin,
            families,
            normalized_families,
            post_script_name,
            weight,
            italic,
            static_data,
            face: OnceLock::new(),
        });
        font
    }

    /// Finds a face by PostScript name, then family name. Among equal
    /// matches, the earliest origin wins, then the closest style.
    fn find(&self, face: &str) -> Option<(FontId, FaceMatch)> {
        let face = face.trim();
        if face.is_empty() {
            return None;
        }
        // 0. Flash's device font names stand for installed families.
        if let Some(families) = names::device_font_families(face) {
            return families
                .iter()
                .find_map(|family| self.find(family))
                .map(|(id, _)| (id, FaceMatch::FamilyName));
        }
        let best = |pred: &dyn Fn(&Slot) -> bool, style: StyleHint| -> Option<FontId> {
            self.slots
                .iter()
                .enumerate()
                .filter(|(_, s)| pred(s))
                .min_by_key(|(i, s)| {
                    let italic_miss = u32::from(s.italic != style.italic) * 10_000;
                    let weight_miss = u32::from(s.weight.abs_diff(style.weight()));
                    (s.origin, italic_miss + weight_miss, *i)
                })
                .and_then(|(i, _)| u32::try_from(i).ok().map(FontId))
        };
        // 1. PostScript name, exactly, then ignoring case.
        let regular = StyleHint::default();
        if let Some(id) = best(&|s| s.post_script_name == face, regular)
            .or_else(|| best(&|s| s.post_script_name.eq_ignore_ascii_case(face), regular))
        {
            return Some((id, FaceMatch::PostScriptName));
        }
        // 2. Family name, ignoring case, spaces and hyphens.
        let whole = names::normalize(face);
        if let Some(id) = best(&|s| s.normalized_families.contains(&whole), regular) {
            return Some((id, FaceMatch::FamilyName));
        }
        // 3. Family name after removing style words and PS/MT decorations.
        let (stem, style) = names::split_style(face);
        let stem_matches = |s: &Slot| {
            s.normalized_families
                .iter()
                .any(|f| f == &stem || names::strip_vendor_suffix(f) == stem)
        };
        best(&stem_matches, style).map(|id| (id, FaceMatch::FamilyName))
    }

    fn fallback_for(&self, face: &str) -> FontId {
        match names::classify(face) {
            FallbackClass::Naskh => self.embedded.naskh_arabic,
            FallbackClass::SansArabic => self.embedded.sans_arabic,
            FallbackClass::Latin => self.embedded.sans,
        }
    }
}

/// `<exe dir>/fonts`, when the executable's path is known.
fn exe_fonts_dir() -> Option<PathBuf> {
    let exe = std::env::current_exe().ok()?;
    Some(exe.parent()?.join("fonts"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn embedded_fonts_resolve_by_postscript_and_family_name() {
        let lib = FontLibrary::embedded_only();
        let e = lib.embedded();
        assert_eq!(lib.len(), 3);
        let r = lib.resolve_detailed("NotoNaskhArabic-Regular");
        assert_eq!(
            r,
            Resolution {
                font: e.naskh_arabic,
                matched: FaceMatch::PostScriptName
            }
        );
        let r = lib.resolve_detailed("Noto Sans Arabic");
        assert_eq!(
            r,
            Resolution {
                font: e.sans_arabic,
                matched: FaceMatch::FamilyName
            }
        );
        assert_eq!(lib.resolve("notosans"), e.sans);
    }

    #[test]
    fn missing_faces_fall_back_by_style() {
        let lib = FontLibrary::embedded_only();
        let e = lib.embedded();
        for naskh in [
            "Traditional Arabic",
            "Simplified Arabic",
            "Arabic Typesetting",
        ] {
            assert_eq!(lib.resolve_detailed(naskh).font, e.naskh_arabic, "{naskh}");
        }
        assert_eq!(lib.resolve("Arabic Transparent"), e.sans_arabic);
        let r = lib.resolve_detailed("SomeMissingFont");
        assert_eq!(
            r,
            Resolution {
                font: e.sans,
                matched: FaceMatch::Fallback
            }
        );
        assert_eq!(lib.lookup("SomeMissingFont"), None);
        let chain = lib.chain("Traditional Arabic");
        assert_eq!(chain.primary, None);
        assert_eq!(chain.arabic, [e.naskh_arabic, e.sans_arabic]);
        assert_eq!(lib.chain("Tahoma").arabic, [e.sans_arabic, e.naskh_arabic]);
    }

    #[test]
    fn faces_load_with_metrics_and_info() {
        let lib = FontLibrary::embedded_only();
        let e = lib.embedded();
        for id in [e.sans_arabic, e.naskh_arabic, e.sans] {
            let m = lib.metrics(id);
            assert_eq!(m.units_per_em, 1000);
            assert!(m.ascent > 0.5 && m.descent > 0.1, "{m:?}");
            let info = lib.face_info(id).expect("embedded face info");
            assert_eq!(info.origin, FontOrigin::Embedded);
            assert!(info.embedding_allowed(), "{info:?}");
            assert!(info.subsetting_allowed(), "{info:?}");
            assert!(!info.bold && !info.italic);
        }
        assert!(
            lib.face(e.naskh_arabic)
                .is_some_and(LoadedFace::arabic_shaping)
        );
        assert!(!lib.face(e.sans).is_some_and(LoadedFace::arabic_shaping));
        assert_eq!(lib.face_info(FontId(99)), None);
        let restricted = FaceInfo {
            fs_type: 0x0002,
            ..lib.face_info(e.sans).expect("Noto Sans info")
        };
        assert!(!restricted.embedding_allowed());
        let whole_font_only = FaceInfo {
            fs_type: 0x0100,
            ..restricted.clone()
        };
        assert!(whole_font_only.embedding_allowed() && !whole_font_only.subsetting_allowed());
        assert!(lib.glyph_outline(FontId(99), 1).is_empty());
    }

    #[test]
    fn glyph_codes_are_unique_and_prefer_base_code_points() {
        let lib = FontLibrary::embedded_only();
        let id = lib.embedded().naskh_arabic;
        let face = lib.face(id).expect("Naskh loads");
        let font = face.font().expect("font parses");
        let charmap = font.charmap();
        let alef = charmap.map('\u{0627}').expect("alef").to_u32() as u16;
        assert_eq!(lib.glyph_code(id, alef), 0x0627);
        let mut seen = HashSet::new();
        for g in 0..face.glyph_count {
            assert!(
                seen.insert(lib.glyph_code(id, g)),
                "duplicate code for glyph {g}"
            );
        }
        // The final form of alef has no base code point of its own.
        let final_alef = charmap.map('\u{FE8E}').expect("alef final form").to_u32() as u16;
        if final_alef != alef {
            assert_eq!(lib.glyph_code(id, final_alef), 0xFE8E);
        }
    }
}
