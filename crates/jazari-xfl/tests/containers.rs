//! Containers and robustness: the ways to open a folder, `.fla` ZIP archives
//! built from a fixture, refusal of binary FLAs, and documents full of
//! content the reader does not know.

use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};

use jazari_xfl::model::{ContainerKind, Document, Element, LayerKind, Tween};
use jazari_xfl::{Error, TimelineRef, WarningKind};

/// A path under the shared fixtures in `tests/fixtures/` at the repository
/// root: `name` alone is that fixture's XFL folder, `name/rest` a file inside
/// it, and `_src/file` one of the generated source images.
fn data(path: &str) -> PathBuf {
    let fixtures = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../tests/fixtures");
    match path.split_once('/') {
        Some(("_src", file)) => fixtures.join("_src").join(file),
        Some((name, rest)) => fixtures.join(name).join("xfl").join(rest),
        None => fixtures.join(path).join("xfl"),
    }
}

/// A fresh, empty scratch folder for one test.
fn scratch(test: &str) -> PathBuf {
    let dir = Path::new(env!("CARGO_TARGET_TMPDIR"))
        .join("jazari-xfl")
        .join(test);
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(&dir).expect("scratch folder");
    dir
}

/// Every file under `dir`, as (relative path with `/`, bytes).
fn files(dir: &Path) -> Vec<(String, Vec<u8>)> {
    fn walk(root: &Path, dir: &Path, out: &mut Vec<(String, Vec<u8>)>) {
        let mut entries: Vec<_> = fs::read_dir(dir).expect("read_dir").flatten().collect();
        entries.sort_by_key(|e| e.file_name());
        for e in entries {
            let path = e.path();
            if path.is_dir() {
                walk(root, &path, out);
            } else {
                let rel = path
                    .strip_prefix(root)
                    .expect("inside root")
                    .to_string_lossy()
                    .replace('\\', "/");
                out.push((rel, fs::read(&path).expect("read")));
            }
        }
    }
    let mut out = Vec::new();
    walk(dir, dir, &mut out);
    out
}

/// Writes a ZIP archive of `(name, bytes)` entries.
fn write_zip(path: &Path, entries: &[(String, Vec<u8>)]) {
    let file = fs::File::create(path).expect("create archive");
    let mut zip = zip::ZipWriter::new(file);
    let options = zip::write::SimpleFileOptions::default()
        .compression_method(zip::CompressionMethod::Deflated);
    for (name, bytes) in entries {
        zip.start_file(name.as_str(), options).expect("start entry");
        zip.write_all(bytes).expect("write entry");
    }
    zip.finish().expect("finish archive");
}

/// The document without its source information, for comparisons.
fn without_source(mut doc: Document) -> Document {
    doc.source = Default::default();
    doc
}

#[test]
fn a_folder_opens_from_the_folder_the_stub_or_the_document() {
    let dir = data("symbols");
    let (by_dir, r1) = jazari_xfl::open(&dir).expect("folder");
    let (by_stub, r2) = jazari_xfl::open(&dir.join("document.xfl")).expect("stub");
    let (by_doc, r3) =
        jazari_xfl::open_folder(&dir.join("DOMDocument.xml")).expect("DOMDocument.xml");
    assert!(r1.is_empty() && r2.is_empty() && r3.is_empty());
    assert_eq!(by_stub.source.path, dir.join("document.xfl"));
    assert_eq!(by_stub.source.container, ContainerKind::Folder);
    assert!(by_stub.source.base_dir.is_absolute());
    assert!(by_stub.source.base_dir.ends_with("symbols/xfl"));
    let a = without_source(by_dir);
    assert_eq!(a, without_source(by_stub));
    assert_eq!(a, without_source(by_doc));

    assert_eq!(jazari_xfl::sniff(&dir).ok(), Some(ContainerKind::Folder));
    assert_eq!(
        jazari_xfl::sniff(&dir.join("document.xfl")).ok(),
        Some(ContainerKind::Folder)
    );
}

#[test]
fn a_fla_archive_reads_like_its_folder() {
    let tmp = scratch("fla");
    for name in ["symbols", "bitmaps", "as2", "tweens"] {
        let fla = tmp.join(format!("{name}.fla"));
        write_zip(&fla, &files(&data(name)));
        assert_eq!(jazari_xfl::sniff(&fla).ok(), Some(ContainerKind::Zip));
        let (zipped, report) = jazari_xfl::open(&fla).expect("open .fla");
        assert!(report.is_empty(), "{name}: {report:?}");
        assert_eq!(zipped.source.container, ContainerKind::Zip);
        assert_eq!(zipped.source.path, fla);
        assert!(zipped.source.base_dir.ends_with("fla"));
        let (folder, _) = jazari_xfl::open(&data(name)).expect("open folder");
        assert_eq!(without_source(zipped), without_source(folder), "{name}");
    }
}

#[test]
fn archive_lookup_tolerates_backslashes_case_and_renamed_symbol_files() {
    let tmp = scratch("fla-mangled");
    // Backslash separators, odd case, and symbol files whose names no longer
    // match the <Include> hrefs (as with mangled non-ASCII names).
    let entries: Vec<(String, Vec<u8>)> = files(&data("symbols"))
        .into_iter()
        .map(|(name, bytes)| {
            let name = match name.as_str() {
                "DOMDocument.xml" => "domdocument.XML".to_owned(),
                "LIBRARY/symbol-1.xml" => "LIBRARY\\d\u{e9}j\u{e0}-vu.xml".to_owned(),
                other => other.replace('/', "\\"),
            };
            (name, bytes)
        })
        .collect();
    let fla = tmp.join("mangled.fla");
    write_zip(&fla, &entries);
    let (doc, report) = jazari_xfl::open_fla(&fla).expect("open");
    let names: Vec<&str> = doc.library.symbols.keys().map(String::as_str).collect();
    assert_eq!(
        names,
        ["Outer", "Inner", "Box"],
        "Box is found by scanning LIBRARY"
    );
    let kinds: Vec<WarningKind> = report.iter().map(|w| w.kind).collect();
    assert_eq!(
        kinds,
        [WarningKind::MissingItem, WarningKind::Other],
        "{report:?}"
    );
    assert!(report.warnings[0].message.contains("symbol-1.xml"));
    assert_eq!(
        report.warnings[1].location.file.as_deref(),
        Some("LIBRARY/d\u{e9}j\u{e0}-vu.xml")
    );
    // Every instance still resolves.
    let (folder, _) = jazari_xfl::open(&data("symbols")).expect("folder");
    assert_eq!(doc.scenes, folder.scenes);
    assert_eq!(doc.library.symbols["Box"], folder.library.symbols["Box"]);
}

#[test]
fn binary_flash_8_files_are_refused() {
    let tmp = scratch("legacy");
    let fla = tmp.join("old.fla");
    let mut bytes = vec![0xD0, 0xCF, 0x11, 0xE0, 0xA1, 0xB1, 0x1A, 0xE1];
    bytes.resize(1024, 0);
    fs::write(&fla, &bytes).expect("write");
    for result in [
        jazari_xfl::open(&fla).map(|_| ()),
        jazari_xfl::open_fla(&fla).map(|_| ()),
        jazari_xfl::open_folder(&fla).map(|_| ()),
        jazari_xfl::sniff(&fla).map(|_| ()),
    ] {
        let err = result.expect_err("binary FLA must be refused");
        assert!(matches!(err, Error::LegacyFla { .. }), "{err}");
        assert!(
            err.to_string().contains("JPEXS"),
            "the message suggests a converter"
        );
    }
}

#[test]
fn other_files_are_not_xfl() {
    let tmp = scratch("not-xfl");
    let txt = tmp.join("notes.txt");
    fs::write(&txt, "hello").expect("write");
    assert!(matches!(jazari_xfl::open(&txt), Err(Error::NotXfl { .. })));
    assert!(matches!(
        jazari_xfl::open_fla(&txt),
        Err(Error::NotXfl { .. })
    ));
    let empty = tmp.join("empty");
    fs::create_dir_all(&empty).expect("mkdir");
    assert!(matches!(
        jazari_xfl::open(&empty),
        Err(Error::NotXfl { .. })
    ));
    assert!(matches!(
        jazari_xfl::open_folder(&empty),
        Err(Error::MissingDocument { .. })
    ));
    // A stub whose folder lost its DOMDocument.xml.
    fs::write(empty.join("lost.xfl"), "PROXY-CS5").expect("write");
    assert!(matches!(
        jazari_xfl::open(&empty.join("lost.xfl")),
        Err(Error::MissingDocument { .. })
    ));
    assert!(matches!(
        jazari_xfl::open(&tmp.join("missing")),
        Err(Error::Io { .. })
    ));
    // A ZIP without DOMDocument.xml.
    let zip = tmp.join("empty.fla");
    write_zip(&zip, &[("readme.txt".to_owned(), b"x".to_vec())]);
    assert!(matches!(
        jazari_xfl::open(&zip),
        Err(Error::MissingDocument { .. })
    ));
    // A damaged archive.
    let broken = tmp.join("broken.fla");
    fs::write(&broken, b"PK\x03\x04 this is not really a zip").expect("write");
    assert!(matches!(jazari_xfl::open(&broken), Err(Error::Zip { .. })));
}

#[test]
fn malformed_documents_are_errors_but_malformed_symbols_are_warnings() {
    let tmp = scratch("malformed");
    let bad_doc = tmp.join("bad-doc");
    fs::create_dir_all(&bad_doc).expect("mkdir");
    fs::write(
        bad_doc.join("DOMDocument.xml"),
        "<DOMDocument><timelines></DOMDocument>",
    )
    .expect("write");
    let err = jazari_xfl::open(&bad_doc).expect_err("malformed DOMDocument.xml");
    assert!(
        matches!(&err, Error::Xml { file, .. } if file == "DOMDocument.xml"),
        "{err}"
    );

    let wrong_root = tmp.join("wrong-root");
    fs::create_dir_all(&wrong_root).expect("mkdir");
    fs::write(wrong_root.join("DOMDocument.xml"), "<html/>").expect("write");
    assert!(matches!(
        jazari_xfl::open(&wrong_root),
        Err(Error::Xml { .. })
    ));

    let bad_symbol = tmp.join("bad-symbol");
    fs::create_dir_all(bad_symbol.join("LIBRARY")).expect("mkdir");
    fs::write(
        bad_symbol.join("DOMDocument.xml"),
        r#"<DOMDocument xflVersion="2.2"><symbols><Include href="A.xml"/><Include href="Gone.xml"/></symbols>
           <timelines><DOMTimeline name="Scene 1"/></timelines></DOMDocument>"#,
    )
    .expect("write");
    fs::write(
        bad_symbol.join("LIBRARY/A.xml"),
        "<DOMSymbolItem name='A'><timeline>",
    )
    .expect("write");
    let (doc, report) = jazari_xfl::open(&bad_symbol).expect("a bad symbol is not fatal");
    assert!(doc.library.symbols.is_empty());
    let kinds: Vec<WarningKind> = report.iter().map(|w| w.kind).collect();
    assert_eq!(kinds, [WarningKind::Other, WarningKind::MissingItem]);
    assert_eq!(
        report.warnings[0].location.file.as_deref(),
        Some("LIBRARY/A.xml")
    );
}

/// A CS6-style document with a namespace prefix, a symbol in a library
/// folder, and plenty of content milestone 1 does not support.
const ODD_DOCUMENT: &str = r##"<?xml version="1.0" encoding="UTF-8"?>
<xfl:DOMDocument xmlns:xfl="http://ns.adobe.com/xfl/2008/" width="320" height="240" frameRate="30" backgroundColor="#102030" xflVersion="2.2">
  <xfl:folders><xfl:DOMFolderItem name="art" isExpanded="true"/></xfl:folders>
  <xfl:fonts><xfl:DOMFontItem name="Font 1" font="Tahoma" size="0" embedRanges="1|9"/></xfl:fonts>
  <xfl:media>
    <xfl:DOMSoundItem name="beep.wav" soundDataHRef="M 1.dat"/>
    <xfl:DOMBitmapItem name="missing.png" bitmapDataHRef="M 9.dat" frameRight="200" frameBottom="100"/>
  </xfl:media>
  <xfl:symbols><xfl:Include href="art/Ball.xml"/></xfl:symbols>
  <xfl:timelines>
    <xfl:DOMTimeline name="Intro">
      <xfl:layers>
        <xfl:DOMLayer name="cam" layerType="camera"><xfl:frames><xfl:DOMFrame index="0"/></xfl:frames></xfl:DOMLayer>
        <xfl:DOMLayer name="art" color="#FF0000">
          <xfl:frames>
            <xfl:DOMFrame index="0" duration="10" tweenType="motion object" soundName="beep.wav">
              <xfl:elements>
                <xfl:DOMSymbolInstance libraryItemName="art/Ball" name="ball"/>
                <xfl:DOMSymbolInstance libraryItemName="art/Nowhere"/>
                <xfl:DOMHologram depth="3"/>
                <xfl:DOMBitmapInstance libraryItemName="ghost.png"/>
                <xfl:DOMShape><xfl:edges><xfl:Edge fillStyle1="2" edges="!0 0|100 #G"/></xfl:edges></xfl:DOMShape>
              </xfl:elements>
            </xfl:DOMFrame>
          </xfl:frames>
        </xfl:DOMLayer>
      </xfl:layers>
    </xfl:DOMTimeline>
    <xfl:DOMTimeline name="Main"><xfl:layers><xfl:DOMLayer name="l"><xfl:frames><xfl:DOMFrame index="0" tweenType="wobble"/></xfl:frames></xfl:DOMLayer></xfl:layers></xfl:DOMTimeline>
  </xfl:timelines>
  <xfl:swatchLists/>
  <xfl:mysteryBlock/>
</xfl:DOMDocument>
"##;

const ODD_SYMBOL: &str = r##"<DOMSymbolItem xmlns="http://ns.adobe.com/xfl/2008/" name="art/Ball" symbolType="graphic" linkageExportForAS="true" linkageIdentifier="ball">
  <timeline><DOMTimeline name="Ball"><layers><DOMLayer name="Layer 1"><frames>
    <DOMFrame index="0"><elements><DOMOvalObject objectWidth="20" objectHeight="20" x="-10" y="-10"><fill><SolidColor color="#FFCC00"/></fill></DOMOvalObject></elements></DOMFrame>
  </frames></DOMLayer></layers></DOMTimeline></timeline>
</DOMSymbolItem>"##;

#[test]
fn unknown_content_never_aborts_and_is_reported_with_its_location() {
    let dir = scratch("odd");
    fs::create_dir_all(dir.join("LIBRARY/art")).expect("mkdir");
    fs::write(dir.join("DOMDocument.xml"), ODD_DOCUMENT).expect("write");
    fs::write(dir.join("LIBRARY/art/Ball.xml"), ODD_SYMBOL).expect("write");
    fs::write(dir.join("odd.xfl"), "PROXY-CS5").expect("write");

    let (doc, report) = jazari_xfl::open(&dir.join("odd.xfl")).expect("never aborts");
    assert_eq!(
        (doc.width, doc.height, doc.frame_rate),
        (320.0, 240.0, 30.0)
    );
    assert_eq!(doc.library.folders, ["art"]);
    assert_eq!(doc.library.fonts["Font 1"].embed_ranges, [1, 9]);
    assert_eq!(doc.scenes.len(), 2);
    assert_eq!(doc.scenes[1].name, "Main");

    let ball = &doc.library.symbols["art/Ball"];
    assert_eq!(
        ball.linkage.as_ref().map(|l| l.identifier.as_str()),
        Some("ball")
    );
    assert!(
        matches!(&ball.timeline.layers[0].frames[0].elements[0], Element::Shape(s) if s.is_drawing_object)
    );

    let intro = &doc.scenes[0];
    assert_eq!(intro.layers[0].kind, LayerKind::Other("camera".to_owned()));
    let frame = &intro.layers[1].frames[0];
    assert_eq!(frame.tween, Tween::Other("motion object".to_owned()));
    assert_eq!(frame.elements.len(), 5);
    assert_eq!(
        frame.elements[2],
        Element::Unsupported {
            tag: "DOMHologram".to_owned()
        }
    );
    assert_eq!(
        doc.scenes[1].layers[0].frames[0].tween,
        Tween::Other("wobble".to_owned())
    );

    let messages: Vec<String> = report.iter().map(ToString::to_string).collect();
    let find = |needle: &str| {
        report
            .iter()
            .find(|w| w.message.contains(needle))
            .unwrap_or_else(|| panic!("no warning about {needle:?} in:\n{}", messages.join("\n")))
    };
    assert_eq!(find("mysteryBlock").kind, WarningKind::UnknownElement);
    assert_eq!(find("sound 'beep.wav'").kind, WarningKind::Unsupported);
    assert_eq!(
        find("missing.png' was not found").kind,
        WarningKind::MissingItem
    );
    assert_eq!(find("camera layer").kind, WarningKind::AnimateFeature);
    assert_eq!(find("motion object").kind, WarningKind::AnimateFeature);
    assert_eq!(find("wobble").kind, WarningKind::UnknownElement);

    let hologram = find("DOMHologram");
    assert_eq!(hologram.kind, WarningKind::UnknownElement);
    assert_eq!(hologram.location.file.as_deref(), Some("DOMDocument.xml"));
    assert_eq!(
        hologram.location.timeline,
        Some(TimelineRef::Scene {
            index: 0,
            name: "Intro".to_owned()
        })
    );
    assert_eq!(
        hologram.location.layer.as_ref().map(|l| l.name.as_str()),
        Some("art")
    );
    assert_eq!(hologram.location.frame, Some(0));
    assert_eq!(hologram.location.element, vec![2]);
    assert_eq!(
        hologram.to_string(),
        "DOMDocument.xml › scene 'Intro' › layer 'art' › frame 1 › element 2: unknown element <DOMHologram> is left out"
    );

    let nowhere = find("'art/Nowhere' is not in the library");
    assert_eq!(nowhere.kind, WarningKind::MissingItem);
    assert_eq!(nowhere.location.element, vec![1]);
    assert_eq!(
        find("the bitmap 'ghost.png' is not in the library")
            .location
            .element,
        vec![3]
    );

    let edge = find("not a valid number");
    assert_eq!(edge.kind, WarningKind::BadValue);
    assert_eq!(edge.location.element, vec![4]);
    assert_eq!(find("fillStyle1=\"2\"").kind, WarningKind::BadValue);
    let Element::Shape(s) = &frame.elements[4] else {
        panic!()
    };
    assert_eq!(
        s.edges[0].path.len(),
        1,
        "the moveTo before the bad number is kept"
    );

    // Nothing from the editor-only <swatchLists> is reported.
    assert!(report.iter().all(|w| !w.message.contains("swatchLists")));
}

#[test]
fn fla_inside_a_relative_path() {
    // Opening by a relative path works and records an absolute base dir.
    let rel = Path::new("../../tests/fixtures/smoke/xfl/document.xfl");
    let cwd = std::env::current_dir().expect("cwd");
    if !cwd.join(rel).exists() {
        // Tests normally run from the crate folder; skip otherwise.
        return;
    }
    let (doc, report) = jazari_xfl::open(rel).expect("relative stub path");
    assert!(report.is_empty());
    assert!(doc.source.base_dir.is_absolute());
    assert_eq!(doc.library.symbols.len(), 1);
}
