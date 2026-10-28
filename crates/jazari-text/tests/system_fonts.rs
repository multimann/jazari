//! Tests with installed system fonts (they pass without checking anything
//! where the fonts are missing, and say so on stderr).
//!
//! # Calibration against Flash 8's own layout of the `text` oracle fixture
//!
//! `C:\dev\samples\fixtures\text\text.swf` was published by the Flash 8 IDE
//! from static texts in Arial, Times New Roman and Tahoma (see the fixture's
//! MANIFEST entry). Its DefineText records give, for each line, the origin x
//! and the baseline y in whole pixels, and each glyph's advance in twips. The
//! numbers below were read from those records.
//!
//! The test needs those fonts installed (Windows has them).

use jazari_text::{
    FaceMatch, FontLibrary, FontOrigin, TWIPS_PER_PX, TextBlock, TextLayout, layout,
};
use jazari_xfl::model::{Align, TextAttrs, TextRun};

/// Lays out an auto-expanding static text as the fixture has it.
fn fixture_text(
    lib: &FontLibrary,
    face: &str,
    size: f64,
    align: Align,
    width: f64,
    text: &str,
) -> TextLayout {
    let attrs = TextAttrs {
        face: face.to_owned(),
        size,
        align,
        ..TextAttrs::default()
    };
    let block = TextBlock {
        runs: vec![TextRun {
            text: text.to_owned(),
            attrs,
        }],
        width,
        auto_expand: true,
        ..TextBlock::default()
    };
    layout(lib, &block)
}

/// (x, baseline) of each line in pixels.
fn origins(l: &TextLayout) -> Vec<(f64, f64)> {
    l.lines
        .iter()
        .map(|line| (line.x / TWIPS_PER_PX, line.baseline / TWIPS_PER_PX))
        .collect()
}

/// The system library, or `None` (after a note) when a face is missing.
fn system_library(faces: &[&str]) -> Option<FontLibrary> {
    let lib = FontLibrary::new();
    let installed = |face: &str| {
        let r = lib.resolve_detailed(face);
        r.matched != FaceMatch::Fallback
            && lib
                .face_info(r.font)
                .is_some_and(|i| i.origin == FontOrigin::System)
    };
    if let Some(missing) = faces.iter().find(|f| !installed(f)) {
        eprintln!("skipped: {missing} is not installed");
        return None;
    }
    Some(lib)
}

#[test]
fn matches_flash8_line_origins_baselines_and_advances() {
    let faces = [
        "ArialMT",
        "TimesNewRomanPSMT",
        "TimesNewRomanPS-BoldItalicMT",
        "Tahoma",
    ];
    let Some(lib) = system_library(&faces) else {
        return;
    };

    let l = fixture_text(
        &lib,
        "ArialMT",
        18.0,
        Align::Left,
        192.2,
        "Left aligned, Arial 18\rsecond line of static text",
    );
    assert_eq!(origins(&l), [(0.0, 16.0), (0.0, 38.0)]);
    let flash: [f64; 22] = [
        200., 200., 100., 100., 100., 200., 80., 80., 200., 200., 200., 200., 100., 80., 240.,
        120., 80., 200., 80., 100., 200., 200.,
    ];
    let ours: Vec<f64> = l.lines[0].glyphs.iter().map(|g| g.advance).collect();
    assert_eq!(ours, flash);

    let l = fixture_text(
        &lib,
        "TimesNewRomanPSMT",
        24.0,
        Align::Center,
        187.4,
        "Centered, Times 24\rline two",
    );
    assert_eq!(origins(&l), [(0.0, 21.0), (55.0, 50.0)]);

    let l = fixture_text(
        &lib,
        "ArialMT",
        12.0,
        Align::Right,
        116.7,
        "Right aligned, Arial 12\ra third line follows\rand a fourth",
    );
    assert_eq!(origins(&l), [(0.0, 11.0), (22.0, 26.0), (53.0, 41.0)]);
    let flash: [f64; 20] = [
        133., 67., 67., 133., 53., 80., 133., 67., 53., 53., 133., 133., 67., 67., 133., 53., 53.,
        133., 173., 120.,
    ];
    let ours: Vec<f64> = l.lines[1].glyphs.iter().map(|g| g.advance).collect();
    assert_eq!(ours, flash);

    let l = fixture_text(
        &lib,
        "TimesNewRomanPS-BoldItalicMT",
        30.0,
        Align::Left,
        168.55,
        "Bold Italic 30",
    );
    assert_eq!(origins(&l), [(0.0, 27.0)]);
    let flash: [f64; 14] = [
        400., 300., 167., 300., 150., 233., 167., 300., 167., 167., 266., 150., 300., 300.,
    ];
    assert_eq!(
        l.lines[0]
            .glyphs
            .iter()
            .map(|g| g.advance)
            .collect::<Vec<_>>(),
        flash
    );

    // Flash 8 drew the Arabic unjoined and left to right; only the vertical
    // metrics are comparable.
    let l = fixture_text(&lib, "Tahoma", 28.0, Align::Left, 187.85, "مرحبا بالعالم");
    assert_eq!(l.lines[0].baseline, 28.0 * TWIPS_PER_PX);
    assert!(l.lines[0].rtl);
}

/// Fonts with a real lam-alef ligature glyph give one glyph, coded with its
/// Presentation Form (U+FEFB isolated, U+FEFC final).
#[test]
fn lam_alef_ligature_glyphs_get_presentation_form_codes() {
    let faces = ["ArialMT", "Tahoma", "Traditional Arabic"];
    let Some(lib) = system_library(&faces) else {
        return;
    };
    for face in faces {
        let l = fixture_text(&lib, face, 24.0, Align::Left, 0.0, "لا");
        let codes: Vec<u16> = l.glyphs().map(|g| g.code).collect();
        assert_eq!(codes, [0xFEFB], "{face}");
        // سلام: meem (isolated), lam-alef (final), seen (initial), left to right.
        let l = fixture_text(&lib, face, 24.0, Align::Left, 0.0, "سلام");
        let codes: Vec<u16> = l.glyphs().map(|g| g.code).collect();
        assert_eq!(codes, [0x0645, 0xFEFC, 0xFEB3], "{face}");
        let font = l.lines[0].glyphs[0].font;
        assert_eq!(
            lib.face_info(font).map(|i| i.origin),
            Some(FontOrigin::System),
            "{face}"
        );
    }
}

/// CFF outlines (cubic) come out as closed quadratic contours. Uses the
/// first CFF-flavoured `.otf` in the system font folder, if there is one.
#[test]
fn cff_outlines_become_closed_quadratics() {
    use jazari_xfl::model::PathCmd;
    use skrifa::MetadataProvider;
    use skrifa::outline::OutlineGlyphFormat;

    let dir = std::env::var_os("SYSTEMROOT")
        .map(|root| std::path::Path::new(&root).join("Fonts"))
        .unwrap_or_else(|| std::path::PathBuf::from("/usr/share/fonts/opentype"));
    let Ok(entries) = std::fs::read_dir(&dir) else {
        eprintln!("skipped: cannot list {}", dir.display());
        return;
    };
    let cff = entries.filter_map(Result::ok).map(|e| e.path()).find(|p| {
        p.extension().is_some_and(|x| x.eq_ignore_ascii_case("otf"))
            && std::fs::read(p).ok().is_some_and(|data| {
                skrifa::FontRef::new(&data)
                    .is_ok_and(|f| f.outline_glyphs().format() == Some(OutlineGlyphFormat::Cff))
            })
    });
    let Some(path) = cff else {
        eprintln!("skipped: no CFF font in {}", dir.display());
        return;
    };
    let data = std::fs::read(&path).expect("font file reads");
    let font = skrifa::FontRef::new(&data).expect("font parses");
    let glyphs: Vec<u16> = font
        .charmap()
        .mappings()
        .filter_map(|(_, g)| u16::try_from(g.to_u32()).ok())
        .take(40)
        .collect();

    let mut lib = FontLibrary::embedded_only();
    let ids = lib
        .add_font_file(&path, FontOrigin::Folder)
        .expect("CFF font loads");
    let mut curved = 0;
    for glyph in glyphs {
        let cmds = lib.glyph_outline(ids[0], glyph);
        let (mut start, mut last) = (None, None);
        for cmd in &cmds {
            match *cmd {
                PathCmd::MoveTo(p) => {
                    assert_eq!(
                        start,
                        last,
                        "{}: glyph {glyph} has an open contour",
                        path.display()
                    );
                    start = Some(p);
                    last = Some(p);
                }
                PathCmd::LineTo(p) => last = Some(p),
                PathCmd::QuadTo(_, p) => {
                    curved += 1;
                    last = Some(p);
                }
            }
        }
        assert_eq!(
            start,
            last,
            "{}: glyph {glyph} has an open contour",
            path.display()
        );
    }
    assert!(curved > 0, "{}: cubics became quadratics", path.display());
}

/// Flash's device font names resolve to the fonts Flash Player uses for
/// them on Windows.
#[test]
fn device_font_names_resolve_to_installed_families() {
    let Some(lib) = system_library(&["Arial", "Times New Roman", "Courier New"]) else {
        return;
    };
    for (device, family) in [
        ("_sans", "Arial"),
        ("_serif", "Times New Roman"),
        ("_typewriter", "Courier New"),
    ] {
        let r = lib.resolve_detailed(device);
        assert_eq!(r.matched, FaceMatch::FamilyName, "{device}");
        assert_eq!(r.font, lib.resolve(family), "{device}");
    }
}
