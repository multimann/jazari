# Research: text-arabic (verified)

## Recommendation

Build the static text path entirely in Rust with no native libraries, which suits a portable app with no installer.

- **Shaping:** harfrust 0.12.x. It is what egui/epaint 0.36 already use, and rustybuzz is archived. Shape each bidi run with direction RightToLeft and script ARABIC; take glyph ids, advances and x/y offsets in font units.
- **Bidi and line breaks:** unicode-bidi (ParagraphBidiInfo::visual_runs) and unicode-linebreak (greedy fill on logical advances, then reshape each line with pre/post context). Bake the alignment into the positions.
- **Fonts:** skrifa 0.44 for metrics and outlines (quadratic glyf as is; CFF cubics converted with kurbo approx_spline), sharing one FontRef with harfrust. fontdb 0.24 for discovery: match the XFL face as a PostScript name first, then family name. Also scan `fonts/` folders next to the exe and the project, plus embedded OFL fallbacks: Noto Naskh Arabic for Naskh faces, Noto Sans Arabic for sans faces, Amiri optional.
- **SWF fonts:** always embed outlines for static text, including device mode. One DefineFont3 per face (swf::Font, version 3, 20480 units per em, y-down), holding only glyphs actually used in the document, deduplicated across all texts. Flags: HAS_WIDE_CODES and HAS_LAYOUT, never IS_ANSI. Unique codes sorted ascending: cmap code, else Presentation Form, else PUA. Kerning table empty. Split into chunks so each tag stays under 60 KB, to avoid the writer's wide-offsets bug. Skip DefineFontAlignZones in M1.
- **SWF text:** one DefineText (DefineText2 if alpha) per DOMStaticText. Matrix = translate(left); PlaceObject gets the XFL matrix. Records hold absolute x/y in twips, and the height is the size in twips. Absorb mark x offsets into signed GlyphEntry advances computed from rounded absolute positions. Give marks with a y offset their own TextRecord. Respect the limits: 255 glyphs per record, i16 offsets, font and height together, first record sets font, height and color.
- **Stage:** draw text on the stage from the same PositionedGlyph output, so the editor matches Ruffle and the SWF; egui text has no bidi.
- **Next steps:** calibrate pixel snapping and vertical metrics against a Flash-published Arabic sample, and add round-trip tests (parse_swf) for every DefineFont3/DefineText the publisher emits.

## Dependencies

- harfrust = "0.12"   # 0.12.0, MIT. Same as epaint 0.36.x, so one read-fonts 0.41 in the tree; latest is 0.15.0 (needs read-fonts 0.46)
- skrifa = "0.44"     # 0.44.0, MIT OR Apache-2.0. Same as epaint 0.36.x (which already turns on features std + autohint_shaping); latest is 0.49.0
- fontdb = "0.24"     # 0.24.0, MIT. Default features std, fs, memmap, fontconfig; 'fontconfig' is the pure-Rust fontconfig-parser, not libfontconfig
- unicode-bidi = "0.3.18"     # MIT OR Apache-2.0, Unicode 16
- unicode-linebreak = "0.1.5" # Apache-2.0, Unicode 15.0
- unicode-script = "0.5.8"    # MIT OR Apache-2.0, Unicode 17; per-character script for itemizing runs
- kurbo = { version = "0.13", default-features = false, features = ["std"] }   # 0.13.1, already in the tree via peniko/vello_cpu; cubic-to-quadratic
- swf = { git = "https://github.com/ruffle-rs/ruffle", rev = "07d3e430631333f80f754df01a968bdcd80d98c5" }   # swf 0.3.0 at the pinned rev
- Assets, SIL OFL-1.1, shipped with OFL.txt: NotoNaskhArabic-Regular.ttf (158,580 B) and -Bold (158,172 B); NotoSansArabic-Regular.ttf (142,140 B) and -Bold (141,852 B). Optional: Amiri-Regular.ttf (431,116 B) for vowelled/Quranic text; NotoSans-Regular.ttf (431,364 B) as a Latin fallback
- Not a dependency: rustybuzz (archived 2026-07-26); ttf-parser (stays transitive via ruffle_core only); ruffle_desktop's 'fontconfig' feature (links libfontconfig)

## Findings (verifier status in brackets; corrections override the claim)

- **F1** [confirmed] Shaping engine: use harfrust, pinned to 0.12.x. egui/epaint 0.36.x already depend on harfrust 0.12.0, skrifa 0.44.0 and read-fonts 0.41.0, so matching those versions gives one read-fonts in the tree. harfrust and skrifa then share the same FontRef type. rustybuzz is archived. harfrust is MIT; skrifa is MIT OR Apache-2.0.
  - evidence: C:\dev\ref\ruffle\Cargo.lock:1367 (egui 0.36.2), :1569-1592 (epaint 0.36.2 depends on harfrust, skrifa, vello_cpu), :2400-2408 (harfrust 0.12.0 -> read-fonts), :5604-5611 (skrifa 0.44.0 -> read-fonts), :4527 (read-fonts 0.41.0). crates.io API: epaint 0.36.1 and 0.36.2 both require harfrust ^0.12.0 and skrifa ^0.44.0 (https://crates.io/api/v1/crates/epaint/0.36.1/dependencies). epaint shapes with harfrust: epaint-0.36.2/src/text/text_layout.rs:1431-1433. harfrust re-exports read_fonts::FontRef (harfrust-0.12.0/src/lib.rs:22-25); so does skrifa (skrifa-0.44.0/src/lib.rs:64). Latest harfrust is 0.15.0 (2026-10-22, needs read-fonts ^0.46); minor releases ran 0.8.3 -> 0.15.0 between June and October 2026 (https://crates.io/api/v1/crates/harfrust/versions). harfrust README: 'Matches HarfBuzz v13.0.0'. rustybuzz: last release 0.20.1 (2024-11-12), uses ttf-parser 0.25 and tracks HarfBuzz 10.0.1; repo archived 2026-07-26, README tells users to switch to HarfRust (https://github.com/harfbuzz/rustybuzz). Local copies of the downloaded crates are under SCRATCH = C:\Users\A04E~1\AppData\Local\Temp\claude\C--Users-----------CluadeCowork---------------\b1f4a133-7216-4e29-a076-c5e0c76642d5\scratchpad\crates\x-<crate>-<ver>\<crate>-<ver>\ (also browsable at https://docs.rs/crate/harfrust/0.12.0/source/).
- **F2** [confirmed] harfrust 0.12 API for shaping an RTL Arabic run. FontRef::from_index(data, idx) -> ShaperData::new(&font) -> data.shaper(&font).build() gives a Shaper. Fill a UnicodeBuffer (add(char, cluster) or push_str, set_direction(Direction::RightToLeft), set_script(script::ARABIC), set_language, set_flags, set_pre_context / set_post_context). Then shaper.shape(buf, ShapeOptions::new().features(&f)) returns a GlyphBuffer. Read glyph_infos() (glyph_id: u32 <= u16::MAX, cluster: u32) and glyph_positions() (x_advance, y_advance, x_offset, y_offset: i32). Positions are in font units, y-up, unless ShapeOptions::scale is set. Output is in visual left-to-right order even for RTL. Arabic marks get zero advance (zeroed late, by GDEF); their placement is in x_offset/y_offset. The buffer does no bidi. There is no Arabic fallback shaper for fonts without GSUB.
  - evidence: read-fonts-0.41.0/src/lib.rs:365 (FontRef::from_index). harfrust-0.12.0/src/hb/face.rs:26-37 (ShaperData, no lifetime), :70 (shaper), :224 (build), :269/:304/:324 (ShapeOptions new/scale/features; lines 297-303 say None = font units), :522 (units_per_em), :543 (shape). src/lib.rs:48 (hb_font_t re-exported as Shaper), :59-77 (BufferFlags incl. BEGINNING_OF_TEXT, END_OF_TEXT, PRODUCE_SAFE_TO_INSERT_TATWEEL). src/hb/buffer.rs:23-35 (GlyphPosition), :78-91 (GlyphInfo), :345 (unsafe_to_break), :1666-1678 (push_str sets cluster = byte offset), :1873/:1889/:1914/:1921/:1933/:1944/:1969 (buffer setters), :2054/:2060 (glyph_infos/glyph_positions). src/hb/common.rs:16-27 (Direction enum), :402 (script::ARABIC), :618 (Feature::new(Tag, u32, range)), :644 (FromStr, e.g. "-kern"). src/hb/ot_shape.rs:437-438 (buffer reversed for backward directions = visual order), :766-772 (RTL mirroring). src/hb/ot_shaper_arabic.rs:731 (ZERO_WIDTH_MARKS_BY_GDEF_LATE). README 'No Arabic fallback shaper'. URL: https://docs.rs/crate/harfrust/0.12.0/source/src/hb/face.rs
- **F3** [confirmed] Bidi: unicode-bidi 0.3.18 (Unicode 16). Split paragraphs ourselves and use ParagraphBidiInfo::new(text, None | Some(Level::rtl())). Its levels are indexed per byte. visual_runs(line_byte_range) returns the level runs (byte ranges) in visual order. Shape each run separately with the direction from its level's parity; inside an RTL run, place the items right-to-left. egui 0.36 text does no bidi, so the stage canvas must draw Arabic from our own positioned glyphs, not from egui text.
  - evidence: SCRATCH\x-unicode-bidi-0.3.18\unicode-bidi-0.3.18\src\lib.rs:723 (ParagraphBidiInfo::new), :701 (levels), :848 (visual_runs(&self, line) -> (Vec<Level>, Vec<LevelRun>)), :484 and :669 (BidiInfo::new / visual_runs with &ParagraphInfo); src/prepare.rs:26 (LevelRun = Range<usize>, byte indices); src/level.rs:58/64/116 (Level::ltr, Level::rtl, is_rtl); src/char_data/tables.rs:8 (Unicode 16.0.0). epaint-0.36.2/src/text/font.rs:829 ('TODO(emilk): heed bidi characters'). Ruffle desktop also uses it: C:\dev\ref\ruffle\desktop\Cargo.toml:54.
- **F4** [confirmed] Line breaking: unicode-linebreak 0.1.5 (Apache-2.0, Unicode 15.0). linebreaks(&str) yields (byte index after the break, BreakOpportunity::{Mandatory, Allowed}). Fixed-width wrapping is a greedy fill over logical cluster advances from a first shaping pass. Each line is then reshaped with pre/post context and laid out in bidi visual order. DefineText has no alignment field, so left/right/center/justify (the XFL alignment attribute) must be baked into the glyph positions. swf::TextAlign exists only for DefineEditText.
  - evidence: SCRATCH\x-unicode-linebreak-0.1.5\unicode-linebreak-0.1.5\src\lib.rs:27 (UNICODE_VERSION 15.0.0), :67-72 (BreakOpportunity), :89 (linebreaks); Cargo.toml:31 (Apache-2.0). C:\dev\ref\xfl-schema\xfl.xsd:937-946 (alignment left/right/center/justify). C:\dev\ref\ruffle\swf\src\types.rs:1634-1650 (TextLayout/TextAlign, only used by EditText). For justified Arabic, harfrust can flag kashida points: BufferFlags::PRODUCE_SAFE_TO_INSERT_TATWEEL (harfrust lib.rs:75) and GlyphInfo::safe_to_insert_tatweel (buffer.rs:390).
- **F5** [confirmed] Flash calibration from a real Flash-authored sample. For a DOMStaticText with left=8.9, width=126.2 and matrix (22.1, 80.25), Flash wrote: PlaceObject2 matrix = XFL matrix (442, 1605 twips); DefineText.matrix = translate(left*20, 0) = (178, 0). Record coordinates are relative to the box's left/top. Line origins and baselines land on whole pixels: x 660/0/780 for centered lines; baselines 220/840/1220. The baseline formula fits prev_baseline + descent + lineSpacing + next ascent, rounded to whole px. Per-glyph advances are twip-precise. A 12 pt run gives record height 240 twips. Records without x_offset (a mid-line font-size change) keep the pen running. DefineText bounds are in post-matrix space (x_min 187 = 178 + ink). The DefineFont3 for this static-only text had flags 0x04 (HAS_WIDE_CODES, no layout) and name 'Times New Roman\0'. Flash also wrote DefineFontAlignZones and CSMTextSettings.
  - evidence: XFL: C:\dev\ref\ruffle\tests\tests\swfs\avm2\static_text\test.fla, DOMDocument.xml (DOMStaticText left=8.9 width=126.2 autoExpand=true; Matrix tx=22.1 ty=80.25; DOMTextAttrs alignment=center bitmapSize=240/600 face=TimesNewRomanPSMT). SWF: C:\dev\ref\ruffle\tests\tests\swfs\avm2\static_text\test.swf, dumped with my read-only scripts SCRATCH\..\scripts\dump_swf_text.py and dump_place.py (DefineText id=2 bounds=[187,2873,8,1252] matrix t=(178,0); rec1 {font 1, color 000000, x 660, y 220, h 240}; rec {x 0, y 840}; rec {font 1, h 600} with no x/y; PlaceObject2 t=(442,1605)). Same dump of C:\dev\ref\ruffle\swf\tests\swfs\DefineFont3-CS6.swf: DefineFont3 flags 0x84 (HAS_LAYOUT | HAS_WIDE_CODES), lang 1.
- **F6** [partly-wrong] Font loading: use skrifa 0.44 rather than ttf-parser. With Size::unscaled() and LocationRef::default(), skrifa gives units_per_em, ascent/descent/leading, per-glyph advance and bounds, cmap map and reverse mappings, and outlines through OutlinePen (move_to, line_to, quad_to, curve_to, close; f32 font units, y-up). It covers glyf (quadratic), CFF/CFF2 (cubic) and VARC, plus variable locations. ttf-parser 0.25.1 is stale (2024-11-29); it remains in the tree only via ruffle_core. fontdb 0.24 dropped it.
  - evidence: SCRATCH\x-skrifa-0.44.0\skrifa-0.44.0\src\provider.rs:17-51 (MetadataProvider: metrics, glyph_metrics, charmap, outline_glyphs); src/metrics.rs:72-102 (Metrics {units_per_em: u16, ascent, descent, leading: f32, ...}), :291 (advance_width(GlyphId) -> Option<f32>), :352 (bounds); src/charmap.rs:102 (map), :108 and :203-204 (mappings(), Item = (u32, GlyphId)); src/outline/mod.rs:122-131 (OutlineGlyphFormat {Glyf, Cff, Cff2, Varc}), :189 (DrawSettings::unhinted), :353 (OutlineGlyph::draw), :639 (OutlineGlyphCollection::get); src/instance.rs:29 (Size::unscaled); read-fonts-0.41.0/src/model/pen.rs:8-24 (OutlinePen). OS/2 embedding bits: read-fonts generated os2 fs_type() (generated_os2.rs:461). C:\dev\ref\ruffle\Cargo.lock:6425 (ttf-parser 0.25.1). fontdb CHANGELOG 0.24.0: 'no longer depends on ttf-parser'.
  - CORRECTION: The skrifa API is correct. MetadataProvider::metrics is at provider.rs:37, not :17-51. Metrics is at metrics.rs:72-102. advance_width(GlyphId) -> Option<f32> is at :291. Charmap::map is at charmap.rs:102 and mappings() at :108, with Item=(u32, GlyphId) at :204. OutlineGlyphFormat is at outline/mod.rs:122-131. DrawSettings::unhinted is at :189, draw(impl Into<DrawSettings>, &mut impl OutlinePen) at :353 and get at :639. OutlinePen (read-fonts pen.rs:8-24) is re-exported at outline/mod.rs:105. WRONG: 'ttf-parser remains in the tree only via ruffle_core'. Cargo.lock:1899 shows fontdb 0.23.0 depending on ttf-parser (used by ruffle_desktop, desktop/Cargo.toml:22). Cargo.lock:12 shows ab_glyph -> owned_ttf_parser, and ab_glyph is pulled in by sctk-adwaita (Cargo.lock:5409), i.e. winit's Wayland decorations. Also note: skrifa's ascent/descent use OS/2 typo when USE_TYPO_METRICS is set, else hhea, else win (metrics.rs:159-187). All three bundled fonts set USE_TYPO_METRICS.
- **F7** [confirmed] Cubic to quadratic conversion: SWF shape records have only straight and quadratic edges, so CFF/OTF cubics need approximation. kurbo 0.13.1 is already in the dependency tree (via peniko, vello_cpu and epaint). CubicBez::approx_spline(accuracy) -> Option<QuadSpline> produces G1-continuous quads (a cu2qu-style spline); to_quads(accuracy) is the fallback. lyon_geom 1.0.6 (via ruffle_render) offers for_each_quadratic_bezier as an alternative.
  - evidence: C:\dev\ref\ruffle\swf\src\types.rs:724-733 (ShapeRecord::{StyleChange, StraightEdge, CurvedEdge{control_delta, anchor_delta}}). C:\dev\ref\ruffle\Cargo.lock:2928 (kurbo 0.13.1, depended on by peniko 0.6.1). SCRATCH\x-kurbo-0.13.1\kurbo-0.13.1\src\cubicbez.rs:78 (to_quads), :103 (approx_spline); src/quadspline.rs:31 (to_quads, 'guaranteed to be G1 continuous'). SCRATCH\x-lyon_geom-1.0.6\lyon_geom-1.0.6\src\cubic_bezier.rs:510.
- **F8** [confirmed] Font discovery: fontdb 0.24.0 (MIT) scans system fonts with no native library. On Windows it reads %SYSTEMROOT%\Fonts and the per-user font folders; on Linux it uses fontconfig-parser (pure-Rust XML parser) and falls back to /usr/share/fonts and similar. Database::query matches family names by exact string equality. XFL 'face' is usually a PostScript name (e.g. TimesNewRomanPSMT, ArialMT, Arial-BoldMT), and DOMTextAttrs has no bold/italic attributes, so the style lives inside the face name. Resolve by FaceInfo.post_script_name first, then case-insensitive family, then fallback. For a portable app, also load_fonts_dir(<exe>/fonts) and (<project>/fonts), and load embedded fonts with load_font_data. Do not enable ruffle_desktop's 'fontconfig' feature, which links libfontconfig.
  - evidence: SCRATCH\x-fontdb-0.24.0\fontdb-0.24.0\Cargo.toml [features] default = [std, fs, memmap, fontconfig], fontconfig = [fontconfig-parser, fs]; src/lib.rs:197 (load_font_data), :306 (load_fonts_dir), :402-424 (load_system_fonts on Windows), :487-497 (Linux fallback dirs), :505 (load_fontconfig), :663-680 (query, exact match at :669), :687 (faces), :723 (with_face_data), :838 (families), :845 (post_script_name). Without memmap, files are read with std::fs::read (:288-293). C:\dev\ref\xfl-schema\xfl.xsd:907 (face, required), :897 (size in pt), :902 (bitmapSize in twips). C:\dev\ref\ruffle\desktop\Cargo.toml:60 and :84 (fontconfig = yeslogic-fontconfig-sys, default feature).
- **F9** [confirmed] Embedded fallback fonts, all SIL OFL 1.1 (ship OFL.txt with them; the repo's Unlicense does not cover them). Unhinted static TTFs: Noto Sans Arabic Regular 142,140 B (Bold 141,852); Noto Naskh Arabic Regular 158,580 B (Bold 158,172); Amiri Regular 431,116 B (Bold 413,480). The Noto variable font NotoSansArabic[wdth,wght].ttf is 844,676 B. All have upem 1000 and GSUB+GPOS 'arab'. The Noto Arabic fonts have zero Latin A-Z; Amiri has full Latin and 6,710 glyphs. A Latin fallback (Noto Sans Regular, 431,364 B) would also be needed if content must not depend on system Latin fonts. Recommendation: Noto Naskh Arabic as the Naskh fallback (Traditional/Simplified Arabic, Times-like faces) plus Noto Sans Arabic as the sans fallback (Tahoma/Arial/Segoe). Amiri is the best choice for fully vowelled or Quranic text but is about 3x larger.
  - evidence: GitHub API: https://api.github.com/repos/notofonts/notofonts.github.io/contents/fonts/NotoSansArabic/unhinted/ttf , .../NotoNaskhArabic/unhinted/ttf , .../NotoSans/unhinted/ttf ; https://api.github.com/repos/google/fonts/contents/ofl/amiri , .../ofl/notosansarabic. Coverage and metrics via fontTools (CPU-only) script SCRATCH\..\scripts\inspect_fonts.py run on the downloads in SCRATCH\..\fonts\: NotoSansArabic upem 1000, 1399 glyphs, hhea 1374/-738, latinAZaz 0, 772 presentation-form cmap entries; NotoNaskhArabic 1415 glyphs, hhea 1069/-634, win 1405/634, latinAZaz 0; Amiri 6710 glyphs, hhea 1124/-634, win 1850/910, latinAZaz 52. Name ID 13 for all three = 'This Font Software is licensed under the SIL Open Font License'.
- **F10** [confirmed] DefineFont3 in the swf crate is swf::Font with version: 3, written as Tag::DefineFont2(Box<Font>). Its glyph space is 20480 units per em (1024 x 20), y-down, so SWF = round(fu * 20480 / upem) with y negated. Per glyph: code u16, advance u16 and bounds Option<Rectangle<Twips>>, all in the same 20480 space. FontLayout has ascent u16, descent u16, leading i16 and Vec<KerningRecord{left_code, right_code, adjustment: Twips}>. Glyph shapes are ShapeRecords drawn with fill style 1 (Flash uses fill_style_0 = Some(1) on the first move). Ruffle turns each glyph into a white-filled shape with NON_ZERO winding and computes bounds itself when they are empty. Since SWF6 the code table must use wide codes and be sorted ascending.
  - evidence: C:\dev\ref\ruffle\swf\src\types.rs:538-541 and :560-561 (Tag::DefineFont2 / DefineFontAlignZones / DefineText / DefineText2), :1191-1199 (Font), :1201-1213 (FontFlag: IS_BOLD=1, IS_ITALIC=2, HAS_WIDE_CODES=4, HAS_WIDE_OFFSETS=8, IS_ANSI=16, IS_SMALL_TEXT=32, IS_SHIFT_JIS=64, HAS_LAYOUT=128), :1225-1230 (Glyph), :1233-1245 (FontLayout, KerningRecord), :244-251 (Language). write.rs:2109-2113 (version 3 -> TagCode::DefineFont3); read.rs:456. C:\dev\ref\ruffle\core\src\font.rs:393-395 ('DefineFont3 stores coordinates at 20x the scale', scale 20480.0). C:\dev\ref\ruffle\render\src\shape_utils.rs:1213-1239 (swf_glyph_to_shape: white fill, NON_ZERO_WINDING_RULE, bounds fallback at 1218-1221). C:\dev\ref\ruffle\swf\src\test_data.rs:544-549 (Flash CS5.5 glyph with fill_style_0: Some(1)). https://www.m2osw.com/swf_tag_definefont2 ('Since V6.x the f_font2_wide must always be set to 1'; glyphs 'must be ordered in ascending order').
- **F11** [confirmed] The swf writer for DefineFont2/3 has consistency traps. (1) It writes font.flags verbatim but decides wide codes from !IS_ANSI, while the reader decides from HAS_WIDE_CODES: always set HAS_WIDE_CODES and never IS_ANSI. (2) It picks wide offsets internally (only when some glyph start offset in the shape buffer exceeds 0xFFFF) but never sets HAS_WIDE_OFFSETS in the flags. Worse, in the narrow case it truncates (offset + 2n+2) and code_table_offset to u16. So a shape table just under 64 KiB silently produces a corrupt tag. (3) If HAS_LAYOUT is set, every glyph's bounds must be Some. (4) The kerning adjustment is written as i16 and advances as u16. By my estimate an Arabic glyph is about 340-420 bytes in SWF, so roughly 140-180 glyphs fill 60 KB, and real Arabic static text can hit this limit.
  - evidence: C:\dev\ref\ruffle\swf\src\write.rs:2010 (has_wide_codes = !IS_ANSI), :2024-2028 (has_wide_offsets only if a glyph offset > 0xFFFF), :2042 (flags written as-is), :2051-2073 (u16 casts of offsets and CodeTableOffset), :2092 (advance u16), :2095-2100 (error if bounds is None), :2190 (kerning i16). read.rs:1022-1046 (offset width from HAS_WIDE_OFFSETS), :1070 (code width from HAS_WIDE_CODES). Size estimate from fontTools script SCRATCH\..\scripts\estimate_swf_glyph_bytes.py: NotoSansArabic avg 405 B (p90 691), NotoNaskhArabic avg 424 B, Amiri avg 338 B per Arabic glyph. swf::write_swf and Header::default_with_swf_version are public, so tags can be measured (write.rs:35; types.rs:78).
- **F12** [confirmed] Code table for contextual Arabic glyphs that have no unique code point: Ruffle's DefineText rendering looks glyphs up by index only, so codes do not affect static rendering. Codes do matter for DefineEditText lookup (a code->index map where the last duplicate wins), for TextSnapshot / StaticText.text, which return each glyph's character, and for kerning pairs. Assign each embedded glyph a unique u16 code: (a) the font's own cmap code via reverse mappings(), preferring base code points (U+0600-06FF) over Presentation Forms; (b) otherwise the Presentation Forms-A/B code the font maps to that glyph (Noto has 772 such mappings: initial/medial/final forms and the lam-alef ligature); (c) otherwise a Private Use Area code (U+E000..). Then sort glyphs by code ascending and remap indices.
  - evidence: C:\dev\ref\ruffle\core\src\display_object\text.rs:180 (font.get_glyph(c.index)), :97-100 (text() uses g.character()); C:\dev\ref\ruffle\core\src\font.rs:122-128 (code_point_to_glyph used by DefineEditText, kerning_pairs keyed by code), :141-147 (get_by_index), :370 (code_point_to_glyph.insert, so duplicate codes overwrite), :184-199 (kerning lookup by char). SCRATCH\x-skrifa-0.44.0\skrifa-0.44.0\src\charmap.rs:108 (mappings). Presentation-form counts from inspect_fonts.py (F9).
- **F13** [confirmed] DefineText/DefineText2 and exact Ruffle semantics. Text has id, bounds, matrix and records. TextRecord has font_id, color, x_offset, y_offset, height (twips) and glyphs: Vec<GlyphEntry{index: u32, advance: i32}>. Ruffle's renderer: x_offset sets the pen x (tx) and y_offset sets the baseline (ty), both absolute in DefineText space. Missing fields carry over from the previous record. Each glyph is drawn at (tx, ty) scaled by height / 20480, then tx += advance (signed). If the index is invalid, nothing is drawn and the advance is skipped. The font-advance table is ignored for static text. Initial state is font 0, height 0 and color alpha 0, so the first record must set font, height and color. Writer and reader constraints: font_id and height must be Some together (one flag bit); at most 255 glyphs per record (u8 count); x/y offsets are i16 (±32767 twips); height is u16; glyph and advance bit widths are computed per tag; DefineText colors are RGB and DefineText2 colors are RGBA.
  - evidence: C:\dev\ref\ruffle\swf\src\types.rs:1276-1297 (Text, TextRecord, GlyphEntry). C:\dev\ref\ruffle\core\src\display_object\text.rs:134-193 (render_self; 150-157 initial color/font/height, 160-165 x/y offsets, 175 scale = height / font.scale(), 180-188 glyph render and signed advance), :195-197 (self_bounds = bounds, used for culling at 135-142 and hit-testing at 199-265). C:\dev\ref\ruffle\swf\src\write.rs:2202-2266 (2209-2222 bit counts, 2225-2229 flags 0x80|font 0x08|color 0x04|y 0x02|x 0x01, 2235-2239 RGB vs RGBA, 2242-2245 i16 offsets, 2247-2249 u16 height, 2250 u8 glyph count, 2253-2254 ubits/sbits). read.rs:2319-2378 (font and height tied at 2332 and 2356). Twips is i32 (types/twips.rs:19).
- **F14** [confirmed] Positioning harakat (marks with non-zero GPOS offsets). Let pen_i be the shaper pen and P_i = pen_i + x_offset_i the drawing x. Fold x offsets into signed GlyphEntry advances: advance_i = round(P_{i+1}) - round(P_i). This is exact and needs no extra records; round absolute positions so error does not accumulate. For y offsets, start a new TextRecord with y_offset = baseline - y_offset_i and an explicit x_offset = round(P_i). The next glyph starts another record that restores y_offset = baseline (about 7-8 bytes per raised or lowered mark). Units: twips = font_units * height_twips / upem; SWF y is down, so subtract the shaper's y_offset.
  - evidence: Signed advance honored: C:\dev\ref\ruffle\core\src\display_object\text.rs:187 (transform.matrix.tx += Twips::new(c.advance)); x/y are absolute per record at :160-165. Writer encodes signed advances: C:\dev\ref\ruffle\swf\src\write.rs:2215-2220 and :2254. Shaper semantics: harfrust-0.12.0/src/hb/buffer.rs:18-35 ('All positions are relative to the current point'; offsets do not affect the advance). Flash itself emits records without x_offset that continue the pen (test.swf dump, F5).
- **F15** [confirmed] DefineFontAlignZones is optional and can be skipped in M1. Ruffle core never reads it; it only reads CSMTextSettings, which feeds TextRenderSettings. For static text that changes only hit-testing (an 'advanced' text hit-tests by its bounding box), not rendering. The writer supports align zones (FontAlignZone {left, width, bottom, height: i16}, with a TODO to store them as f16) and FontThickness. Kerning: leave FontLayout.kerning empty, because harfrust's GPOS kerning is already baked into the GlyphEntry advances. Turn off kerning with Feature 'kern'=0 when XFL autoKern="false".
  - evidence: Repo-wide search found no DefineFontAlignZones handling under C:\dev\ref\ruffle\core\src; CsmTextSettings is handled at C:\dev\ref\ruffle\core\src\display_object\movie_clip.rs:527 and core/src/font/text_render_settings.rs:159-160; text.rs:208-211 (advanced settings mean bounding-box hit-test). C:\dev\ref\ruffle\swf\src\types.rs:1659-1672; write.rs:643-659. C:\dev\ref\xfl-schema\xfl.xsd:932 (autoKern, default true).
- **F16** [confirmed] Device fonts versus embedded outlines: always embed outlines for static text in M1. XFL fontRenderingMode can be device, bitmap, standard, advanced (the default) or customThicknessSharpness. In Ruffle, DefineText can only draw from a font with in-memory glyphs: get_by_index returns None for Empty, FontFace and ExternalRenderer sources. So device-font static text would be invisible (and its advances skipped). For fontRenderingMode="device", embed anyway and emit a publish warning.
  - evidence: C:\dev\ref\xfl-schema\xfl.xsd:1025-1035 (fontRenderingMode enum). C:\dev\ref\ruffle\core\src\font.rs:141-147 (get_by_index), :497-499 (has_glyphs). Flash's device-text sample C:\dev\ref\ruffle\swf\tests\swfs\DefineFont3-DeviceText.swf has DefineFont3 '_sans' with 0 glyphs and flags 0x04 (my dump).
- **F17** [confirmed] Which glyphs to embed: take glyph ids from the shaping output (post-GSUB contextual forms and ligatures), not from cmap[char]. Union them per resolved face (font file, face index, variation location, bold/italic) across all static texts in the document: every timeline, symbol and size, since size lives in the record height. Emit one DefineFont3 per face, split into several when the encoded tag would exceed about 60 KB (F11). Keep whitespace glyphs as empty shapes: Ruffle only advances the pen when the index is valid, and TextSnapshot text keeps the spaces.
  - evidence: C:\dev\ref\ruffle\core\src\display_object\text.rs:175-188 (scale from height, so one font serves all sizes; advance only inside if let Some(glyph)), :97-100 (snapshot text). F11 size estimate. Shaper output glyph ids: harfrust-0.12.0/src/hb/buffer.rs:78-91.
- **F18** [confirmed] XFL inputs for layout: size (pt, default 12) or bitmapSize (twips = size*20, which gives the record height directly); lineSpacing (pt, default 2.0, added between lines); letterSpacing (pt); autoKern; alignment; indent, leftMargin and rightMargin (pt); and on DOMStaticText: left, top, width, height and autoExpand (label vs. fixed width, with wrapping). There are no bold/italic attributes; the style is part of the face name.
  - evidence: C:\dev\ref\xfl-schema\xfl.xsd:897 (size), :902 (bitmapSize), :907 (face), :922 (lineSpacing), :927 (letterSpacing), :932 (autoKern), :937 (alignment), :962 (indent), :967 (leftMargin), :1043-1072 (left/top/width/height/scrollable/autoExpand), :1609-1620 (DOMStaticText).

## Risks

- swf writer bug (C:\dev\ref\ruffle\swf\src\write.rs:2010, 2026-2028, 2042, 2062, 2067): HAS_WIDE_OFFSETS is never set automatically, and in the narrow case offsets and CodeTableOffset are truncated to u16. A DefineFont3 whose shape table is close to 64 KiB is silently corrupt. Arabic glyphs average about 340-420 bytes each, so roughly 150 glyphs reach the limit. Mitigation: split fonts into chunks measured with write_swf (budget 60 KB), add a round-trip test (parse_swf, compare glyph count and shapes), and consider an upstream fix.
- Inconsistent flags make the reader misparse the font: HAS_WIDE_CODES must be set and IS_ANSI cleared, because the writer derives code width from !IS_ANSI and the reader from HAS_WIDE_CODES.
- TextRecord limits: at most 255 glyphs per record; x/y offsets are i16 (±1638 px from the DefineText origin); font_id and height must be set together; the first record must set font, height and color (Ruffle starts with alpha 0 and height 0, so text would be invisible).
- Ruffle draws glyph shapes with NON_ZERO winding (shape_utils.rs:1227). Adobe Flash Player's fill rule for overlapping glyph contours may differ (probably even-odd), so fonts with overlapping or composite contours could show holes in the real Flash Player while the Ruffle preview looks fine. Confidence low to medium; consider overlap removal later.
- harfrust has no Arabic fallback shaper: legacy fonts without GSUB 'arab' features will render unjoined letters. Detect a missing 'arab' script in GSUB and fall back to the embedded Noto font.
- Version churn: harfrust minor releases are breaking and come every few weeks. Bump harfrust and skrifa only together with epaint when the Ruffle rev moves, otherwise two read-fonts versions end up in the binary.
- Font substitution is machine-dependent: Linux Mint/Ubuntu lack Tahoma and Traditional Arabic, so the same XFL publishes different glyphs on different machines. Mitigation: a project-local and exe-local fonts/ folder, plus a publish report listing substitutions.
- Licensing: embedding outlines of system fonts (Arial, Tahoma, Traditional Arabic) into a SWF depends on the OS/2 fsType embedding bits. Read fsType and warn. Bundled fonts are OFL-1.1, not Unlicense; keep them in a separate assets folder with OFL.txt.
- Vertical metrics fidelity: ascent/descent can come from hhea, OS/2 typo or OS/2 win, and Arabic fonts differ a lot (Amiri hhea 1124/634 vs win 1850/910), so line heights may not match Flash IDE output until calibrated.
- egui 0.36 text has no bidi (epaint font.rs:829 TODO). If the stage canvas uses egui text instead of our PositionedGlyph outlines, Arabic and mixed text will look wrong on stage and differ from the published SWF.
- Each raised or lowered mark (non-zero y offset) costs two extra TextRecords (about 8 bytes). Fully vowelled Quranic text grows the SWF a little but stays exact.
- fontdb's default memmap uses unsafe file mapping, and the system scan reads hundreds of files at startup. Copy the bytes of faces actually used, and run the scan lazily or on a background thread.
- letterSpacing applied to Arabic breaks the visual joining; how Flash behaves here is unknown.

## Open questions

- Exact Flash IDE rules for static text, confirmed so far on one Latin sample only: are line origins and baselines snapped to whole pixels for every anti-alias mode, or only for aliasText=false / 'anti-alias for readability'? Is the snap round or floor? Which ascent/descent source does Flash use (hhea, OS/2 typo or OS/2 win)? This needs an Arabic FLA published by Flash/Animate as an oracle.
- How did Flash CS5/CS6 store and lay out Arabic classic static text in XFL: logical order in <characters>? Any direction attribute? Does alignment='left' mean visual left for RTL paragraphs?
- Should static-text fonts be merged with fonts embedded for dynamic/input text (same DefineFont3, codes must stay unique, layout required), or kept separate in M1? Recommendation: keep them separate.
- Which fallback fonts to bundle by default: Noto Naskh Arabic + Noto Sans Arabic (about 600 KB with bold weights)? Add Amiri for Quranic text? Add a Latin fallback, or rely on system Latin fonts (DejaVu on Ubuntu/Mint, Arial on Windows)?
- Kashida (tatweel) justification for Arabic using harfrust's PRODUCE_SAFE_TO_INSERT_TATWEEL, versus space-only justification in M1.
- Faux bold/italic when a face lacks the requested style: Flash synthesizes it; would we need transformed outline copies in a separate DefineFont3?
- Fix the swf DefineFont2/3 writer flags upstream in Ruffle, or carry a vendored patch, given that jazari pins the Ruffle rev?
- Emit DefineFontAlignZones and CSMTextSettings for fontRenderingMode='advanced'? Ruffle ignores the align zones, but Adobe Flash Player uses them.

## Verifier: problems in the code sketch

- Bidi bug that drops glyphs: items are split on the pre-L1 levels (bidi.levels), but visual_runs() returns runs split on L1-adjusted levels (unicode-bidi lib.rs:645-648, :783-795). The filter it.range.start >= vr.start && it.range.end <= vr.end silently drops any item that straddles a run boundary. A typical case is an LTR item ending in a trailing space on a line of an RTL paragraph: L1 resets the space to the paragraph level, so the item spans two runs and disappears. Fix: split items at run boundaries, and take each run's direction from the returned levels vector (let (lv, vruns) = ...; lv[vr.start].is_rtl()), not from bidi.levels[vr.start].
- Trailing whitespace in RTL lines: ws is subtracted from line_w to compute x0, but the loop still advances x over those whitespace glyphs. After L1, logical-trailing spaces of an RTL paragraph sit visually leftmost, so Right/Center-aligned RTL lines are shifted right by ws and overflow the box. Skip the advance of trailing-ws glyphs, or start x at x0 - ws when they come first visually.
- Justify's last line uses bidi.levels.first() as the paragraph direction. Use bidi.paragraph_level (pub field, lib.rs:704) instead; the first character can be neutral or LTR inside an RTL paragraph.
- Font fallback falls through to a face that does not cover the character: face_for_char returns self.sans even when sans lacks the character. Both Noto Arabic fonts lack ASCII ( ) ? " ; / [ ] (verified with fontTools), so common punctuation in Arabic text becomes .notdef (gid 0). Add a Latin/common fallback (e.g. Noto Sans) and check gid==0 after shaping.
- build_define_text sets the last glyph of each line's advance to next line's x minus this x, i.e. a large negative jump. Because num_advance_bits is the maximum over the whole tag (write.rs:2215-2220), this widens every GlyphEntry in the text. When the next glyph starts a new record, use the glyph's own rounded advance; the explicit x_offset makes the value irrelevant anyway.
- An empty glyph list leaves bounds = Rectangle::INVALID (0x7ffffff sentinel, types/rectangle.rs:9 and :84-89), which then gets written into the tag. Emit no DefineText for empty text, or use Rectangle::ZERO.
- Splitting a face into DefineFont3 chunks by halving the code-ordered glyph list scatters glyphs of one word across fonts. Each switch costs a record with font_id+height+x, which is very common with Arabic contextual forms. Prefer patching the swf writer (set HAS_WIDE_OFFSETS from the actual decision, and decide wide from shape_buf.len() + init_offset > 0xFFFF; write.rs:2024-2072) or chunking by usage locality.
- DefineText.matrix = translate(left, 0) ignores DOMStaticText@top. The Flash sample had top=0, so this is unverified. At least add top to the matrix ty or to the baseline origin, and confirm against an oracle.
- Minor: itemize() calls fonts.resolve() (a linear scan over all fontdb faces with string compares) and face_for_char() (which builds a new Charmap) for every character. Cache them per style and per face. vis.push((g, it.clone())) clones an Item per glyph. This compiles but is wasteful.
- Compile check passes for the rest. Signatures I checked against source are all correct: ShaperData::shaper(&FontRef<'a>).build(), ShapeOptions::features(&[Feature]), Feature::new(Tag, u32, ..), UnicodeBuffer::add then set_pre_context/set_post_context (correct order, since add resets the post-context), Script FromStr (lenient, never panics), skrifa OutlinePen methods, DrawSettings::unhinted, GlyphId::new/to_u32, LocationRef: Default, kurbo CubicBez::new over (f64,f64), approx_spline/to_quads, swf StyleChangeData fields, SwfStr::from_bytes (const fn, string.rs:34), Rectangle::encompass, Matrix::translate, Twips::get, Color (Copy+Eq+Hash) and FontFlag::set (bitflags 2).

## Verifier: missed points

- Stage rendering: epaint can fill only convex paths (epaint-0.36.2/src/shapes/path_shape.rs:14), so glyph outlines with holes and counters cannot be drawn as egui Shape::Path. Rasterize them with vello_cpu (already a dependency of epaint, used at src/text/font.rs:273) into a texture, tessellate them with lyon (already in the tree via ruffle_render) into an egui Mesh, or render the stage via the embedded Ruffle renderer.
- A Latin/common fallback font is mandatory, not optional. Noto Naskh/Sans Arabic lack ASCII parentheses, quotes, question mark, semicolon, slash and brackets. They have only !,.0-9: plus '-' in Sans. RTL bracket mirroring also needs those glyphs present.
- XFL static-text features the report does not cover, which M1 should implement or warn about: orientation='vertical right to left' / 'vertical left to right' (NonInputTextAttributeGroup, xfl.xsd:1130-1146), characterPosition superscript/subscript (:950), rotation (:977), url/target hyperlinks on static text (:982/:987) and aliasText (:895).
- Portability, the user's stated goal: if Jazari reuses ruffle_desktop or ruffle_frontend_utils code, avoid their default 'fontconfig' and 'freetype' features (desktop/Cargo.toml:84 and :98). Expect two fontdb versions (Ruffle desktop pins 0.23 at desktop/Cargo.toml:22 against the proposed 0.24). Embed the fallback fonts with include_bytes so the app stays a single executable. fontdb's pure-Rust fontconfig-parser path keeps the Linux system scan free of native libraries.
- SWF version: the only Flash oracle examined is SWF 15 (AVM2). There is no SWF 8 / AS2 static-text sample. Also, Flash writes lang=1 (Latin) and NUL-terminated font names in DefineFont3. The sketch uses Language::Unknown and an unterminated name; both are harmless in Ruffle, but match Flash for fidelity.
- Selectable static text (isSelectable, SelectableAttributeGroup) and TextSnapshot return glyph codes in record (visual) order. Copied or snapshotted Arabic text will therefore come out reversed and in presentation forms or PUA. Decide whether M1 forces isSelectable=false or accepts this.
- harfrust is still early (0.x, MSRV 1.85 per Cargo.toml:14) and its minor releases are breaking. Pin it exactly (=0.12.0) alongside epaint, and add a CPU-only golden test that shapes a few Arabic strings (lam-alef, harakat, mixed digits) with the embedded Noto fonts, so a Ruffle-rev bump that drags harfrust forward is caught.
- A round-trip test is needed for every emitted tag (swf::parse_swf then compare). In the current writer, font_id and height are tied to one flag (read.rs:2332, :2356) and the count is u8, i16 offsets are truncated silently (write.rs:2242-2250), and DefineFont3 offsets are truncated (write.rs:2062, :2072). All of these fail silently rather than returning an error.

## Code sketch (researcher; check against corrections above)
```rust
// ===== jazari-text: static text -> SWF8 (DefineFont3 + DefineText/DefineText2) =====
// Units: FU = font units (y-up) -> twips (1/20 px, y-down); DefineFont3 EM = 20480 (y-down).
use std::{collections::{BTreeMap, BTreeSet, HashMap}, ops::Range, sync::Arc};
use harfrust::{BufferFlags, Direction, Feature, ShapeOptions, ShaperData, Tag, UnicodeBuffer};
use skrifa::{instance::{LocationRef, Size}, outline::{DrawSettings, OutlinePen}, FontRef, GlyphId, MetadataProvider};
use unicode_bidi::ParagraphBidiInfo;
use unicode_linebreak::{linebreaks, BreakOpportunity};
use unicode_script::{Script as UScript, UnicodeScript};
use swf::{CharacterId, Color, Font, FontFlag, FontLayout, Glyph, GlyphEntry, Language, Matrix, Point, PointDelta,
          Rectangle, ShapeRecord, StyleChangeData, SwfStr, Tag as SwfTag, Text, TextRecord, Twips};

pub const SWF_EM: f64 = 20480.0;        // ruffle core/src/font.rs:395
const FONT_TAG_BUDGET: usize = 60_000;  // swf writer never sets HAS_WIDE_OFFSETS itself (write.rs:2026-2073)

// ------------------------------------------------------------------ fonts
#[derive(Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Debug)]
pub struct FaceKey(pub u32);

pub struct LoadedFace {
    pub data: Arc<Vec<u8>>, pub index: u32, pub family: String, pub ps_name: String,
    pub bold: bool, pub italic: bool,
    pub shaper_data: ShaperData,          // harfrust cache (no lifetime)
}
impl LoadedFace { pub fn font(&self) -> FontRef<'_> { FontRef::from_index(&self.data, self.index).unwrap() } }

pub struct FontStore { db: fontdb::Database, faces: Vec<LoadedFace>, by_id: HashMap<fontdb::ID, FaceKey>,
                       naskh: FaceKey, sans: FaceKey /* embedded OFL fallbacks */ }

impl FontStore {
    pub fn new(exe_dir: Option<&std::path::Path>, project_dir: Option<&std::path::Path>) -> Self {
        let mut db = fontdb::Database::new();
        db.load_system_fonts();                                   // pure Rust; no libfontconfig
        for d in [exe_dir, project_dir].into_iter().flatten() { db.load_fonts_dir(d.join("fonts")); } // portable
        db.load_font_data(include_bytes!("../fonts/NotoNaskhArabic-Regular.ttf").to_vec());
        db.load_font_data(include_bytes!("../fonts/NotoSansArabic-Regular.ttf").to_vec());
        // (+ Bold variants; optional Amiri / Noto Sans Latin)
        todo!("build store; resolve naskh/sans keys by post_script_name")
    }
    /// XFL `face` is usually a PostScript name ("TimesNewRomanPSMT", "Arial-BoldMT").
    pub fn resolve(&mut self, face: &str) -> Option<FaceKey> {
        let id = self.db.faces().find(|f| f.post_script_name == face).map(|f| f.id)
            .or_else(|| self.db.faces().find(|f| f.families.iter().any(|(n, _)| n.eq_ignore_ascii_case(face))).map(|f| f.id))?;
        Some(self.load(id))
    }
    fn load(&mut self, id: fontdb::ID) -> FaceKey {
        if let Some(k) = self.by_id.get(&id) { return *k; }
        let info = self.db.face(id).unwrap().clone();
        let data = Arc::new(self.db.with_face_data(id, |d, _| d.to_vec()).unwrap()); // own bytes, no mmap lifetime
        let shaper_data = ShaperData::new(&FontRef::from_index(&data, info.index).unwrap());
        // TODO: read OS/2 fsType; warn if restricted-license embedding
        let key = FaceKey(self.faces.len() as u32);
        self.faces.push(LoadedFace { data, index: info.index, family: info.families[0].0.clone(),
            ps_name: info.post_script_name.clone(), bold: info.weight.0 >= 600,
            italic: info.style != fontdb::Style::Normal, shaper_data });
        self.by_id.insert(id, key); key
    }
    pub fn face(&self, k: FaceKey) -> &LoadedFace { &self.faces[k.0 as usize] }
    fn covers(&self, k: FaceKey, c: char) -> bool { self.face(k).font().charmap().map(c).is_some() }
    /// primary face -> Arabic fallback (naskh/sans by style of primary) -> sans
    fn face_for_char(&self, primary: FaceKey, c: char) -> FaceKey {
        if self.covers(primary, c) { primary } else if self.covers(self.naskh, c) { self.naskh } else { self.sans }
    }
}

// ------------------------------------------------------------------ layout
pub struct StyledRun { pub text: String, pub face: String, pub size_twips: u16, pub color: Color,
                       pub letter_spacing_twips: f64, pub line_spacing_twips: f64, pub auto_kern: bool }
#[derive(Clone, Copy, PartialEq)] pub enum Align { Left, Right, Center, Justify }

#[derive(Clone, Debug)]
pub struct PositionedGlyph {                // box-relative, twips, y-down; x = pen + x_offset
    pub face: FaceKey, pub glyph_id: u32, pub x: f64, pub y: f64,
    pub size_twips: u16, pub color: Color, pub cluster: usize, pub is_space: bool,
}
#[derive(Clone)] struct Item { range: Range<usize>, face: FaceKey, rtl: bool, script: harfrust::Script, style: usize }
#[derive(Clone)] struct Shaped { gid: u32, cluster: usize, adv: f64, xo: f64, yo: f64 } // twips

/// width: Some(box width - margins) for fixed-width static text, None for autoExpand labels (no wrap).
pub fn layout_static_text(runs: &[StyledRun], width: Option<f64>, align: Align, fonts: &mut FontStore) -> Vec<PositionedGlyph> {
    let (text, spans) = flatten(runs);                       // spans: Vec<(Range<usize>, style_idx)>
    let mut out = Vec::new();
    let mut prev: Option<(f64, f64)> = None;                 // (baseline, max descent) of previous line
    for para in split_paragraphs(&text) {                    // XFL uses '\r' (&#xD;) between paragraphs
        let p = &text[para.clone()];
        let bidi = ParagraphBidiInfo::new(p, None);          // UAX#9, auto base level (P2/P3)
        let items = itemize(p, para.start, &bidi.levels, &spans, runs, fonts);
        // pass 1: logical advances per byte (for fitting)
        let mut adv = vec![0.0f64; p.len() + 1];
        for it in &items { for g in shape(p, it, runs, fonts) { adv[g.cluster] += g.adv; } }
        let lines = break_lines(p, &adv, width);
        let n = lines.len();
        for (li, line) in lines.into_iter().enumerate() {
            // pass 2: reshape the line's pieces (static text is small) in bidi visual order
            let line_items: Vec<Item> = items.iter().filter_map(|it| clip(it, &line)).collect();
            let (_, vruns) = bidi.visual_runs(line.clone());           // byte ranges, visual order
            let mut vis: Vec<(Shaped, Item)> = Vec::new();
            for vr in &vruns {
                let mut its: Vec<&Item> = line_items.iter().filter(|it| it.range.start >= vr.start && it.range.end <= vr.end).collect();
                if bidi.levels[vr.start].is_rtl() { its.reverse(); }   // items of an RTL run go right-to-left
                for it in its { for g in shape(p, it, runs, fonts) { vis.push((g, it.clone())); } } // harfrust: visual L->R
            }
            let trim = trim_ws_end(p, &line);                          // logical trailing whitespace
            let ws: f64 = vis.iter().filter(|(g, _)| g.cluster >= trim).map(|(g, _)| g.adv).sum();
            let line_w: f64 = vis.iter().map(|(g, _)| g.adv).sum::<f64>() - ws;
            let avail = width.unwrap_or(line_w);
            let spaces = vis.iter().filter(|(g, _)| g.cluster < trim && is_space(p, g.cluster)).count();
            let (x0, extra) = match align {
                Align::Left => (0.0, 0.0),
                Align::Right => (avail - line_w, 0.0),
                Align::Center => ((avail - line_w) / 2.0, 0.0),
                Align::Justify if li + 1 < n && spaces > 0 => (0.0, (avail - line_w) / spaces as f64),
                Align::Justify => (if bidi.levels.first().map_or(false, |l| l.is_rtl()) { avail - line_w } else { 0.0 }, 0.0),
            };
            let mut x = (x0 / 20.0).round() * 20.0;                    // Flash snaps line origin to whole px (F5)
            let (asc, desc, lead) = line_metrics(&vis, runs, fonts);  // twips; ascent source = open question
            let baseline = match prev { None => asc, Some((b, d)) => b + d + lead + asc };
            let baseline = (baseline / 20.0).round() * 20.0;           // whole px (F5)
            for (g, it) in &vis {
                let st = &runs[it.style];
                let sp = is_space(p, g.cluster);
                out.push(PositionedGlyph { face: it.face, glyph_id: g.gid, x: x + g.xo, y: baseline - g.yo,
                    size_twips: st.size_twips, color: st.color, cluster: para.start + g.cluster, is_space: sp });
                x += g.adv + if sp && g.cluster < trim { extra } else { 0.0 };
            }
            prev = Some((baseline, desc));
        }
    }
    out
}

fn shape(p: &str, it: &Item, runs: &[StyledRun], fonts: &FontStore) -> Vec<Shaped> {
    let st = &runs[it.style];
    let face = fonts.face(it.face);
    let font = face.font();
    let shaper = face.shaper_data.shaper(&font).build();
    let mut buf = UnicodeBuffer::new();
    for (i, c) in p[it.range.clone()].char_indices() { buf.add(c, (it.range.start + i) as u32); } // cluster = byte idx
    buf.set_pre_context(&p[..it.range.start]);              // keeps Arabic joining across item/line cuts
    buf.set_post_context(&p[it.range.end..]);
    buf.set_direction(if it.rtl { Direction::RightToLeft } else { Direction::LeftToRight });
    buf.set_script(it.script);
    let mut fl = BufferFlags::empty();
    if it.range.start == 0 { fl |= BufferFlags::BEGINNING_OF_TEXT; }
    if it.range.end == p.len() { fl |= BufferFlags::END_OF_TEXT; }
    buf.set_flags(fl);
    let no_kern = [Feature::new(Tag::new(b"kern"), 0, ..)];
    let feats: &[Feature] = if st.auto_kern { &[] } else { &no_kern };
    let gb = shaper.shape(buf, ShapeOptions::new().features(feats));      // font units (scale = None)
    let k = st.size_twips as f64 / shaper.units_per_em() as f64;          // FU -> twips
    let mut v: Vec<Shaped> = gb.glyph_infos().iter().zip(gb.glyph_positions()).map(|(i, q)| Shaped {
        gid: i.glyph_id, cluster: i.cluster as usize,
        adv: q.x_advance as f64 * k, xo: q.x_offset as f64 * k, yo: q.y_offset as f64 * k }).collect();
    if st.letter_spacing_twips != 0.0 { add_after_each_cluster(&mut v, st.letter_spacing_twips); }
    v
}

fn itemize(p: &str, base: usize, levels: &[unicode_bidi::Level], spans: &[(Range<usize>, usize)],
           runs: &[StyledRun], fonts: &mut FontStore) -> Vec<Item> {
    let mut items: Vec<Item> = Vec::new();
    let mut last_script = harfrust::script::COMMON;
    for (i, c) in p.char_indices() {
        let style = style_at(spans, base + i);
        let primary = fonts.resolve(&runs[style].face).unwrap_or(fonts.naskh);
        let inherited = matches!(c.script(), UScript::Inherited);   // harakat stay with their base's face
        let face = match (inherited, items.last()) { (true, Some(it)) => it.face, _ => fonts.face_for_char(primary, c) };
        let script = match c.script() {
            UScript::Common | UScript::Inherited | UScript::Unknown => last_script,
            s => { last_script = s.short_name().parse().unwrap(); last_script } };
        let rtl = levels[i].is_rtl();
        let end = i + c.len_utf8();
        match items.last_mut() {
            Some(it) if it.style == style && it.face == face && it.rtl == rtl && it.script == script => it.range.end = end,
            _ => items.push(Item { range: i..end, face, rtl, script, style }),
        }
    }
    items
}

fn break_lines(p: &str, adv: &[f64], width: Option<f64>) -> Vec<Range<usize>> {
    let Some(max_w) = width else { return vec![0..p.len()] };
    let (mut lines, mut start, mut seg_start, mut w) = (Vec::new(), 0usize, 0usize, 0.0f64);
    for (pos, kind) in linebreaks(p) {                       // pos = byte index after the opportunity
        let full: f64 = adv[seg_start..pos].iter().sum();
        let trimmed: f64 = adv[seg_start..trim_ws(p, seg_start, pos)].iter().sum();
        if w + trimmed > max_w && seg_start > start { lines.push(start..seg_start); start = seg_start; w = 0.0; }
        w += full; seg_start = pos;
        if kind == BreakOpportunity::Mandatory && pos < p.len() { lines.push(start..pos); start = pos; w = 0.0; }
    }
    lines.push(start..p.len());
    lines
}

// ------------------------------------------------------------------ DefineFont3
struct SwfGlyphPen { k: f64, recs: Vec<ShapeRecord>, cur: (i32, i32), start: (i32, i32), fu: (f64, f64),
                     open: bool, first: bool, bb: Option<(i32, i32, i32, i32)> }
impl SwfGlyphPen {
    fn new(k: f64) -> Self { Self { k, recs: vec![], cur: (0, 0), start: (0, 0), fu: (0.0, 0.0), open: false, first: true, bb: None } }
    fn q(&self, x: f64, y: f64) -> (i32, i32) { ((x * self.k).round() as i32, (-y * self.k).round() as i32) } // y-up -> y-down
    fn grow(&mut self, p: (i32, i32)) { let b = self.bb.get_or_insert((p.0, p.0, p.1, p.1));
        b.0 = b.0.min(p.0); b.1 = b.1.max(p.0); b.2 = b.2.min(p.1); b.3 = b.3.max(p.1); }
    fn edge(&mut self, a: (i32, i32)) { if a != self.cur {
        self.recs.push(ShapeRecord::StraightEdge { delta: PointDelta::new(Twips::new(a.0 - self.cur.0), Twips::new(a.1 - self.cur.1)) });
        self.cur = a; self.grow(a); } }
    fn quad(&mut self, c: (i32, i32), a: (i32, i32)) {
        if c == self.cur || c == a { return self.edge(a); }
        self.recs.push(ShapeRecord::CurvedEdge {
            control_delta: PointDelta::new(Twips::new(c.0 - self.cur.0), Twips::new(c.1 - self.cur.1)),
            anchor_delta: PointDelta::new(Twips::new(a.0 - c.0), Twips::new(a.1 - c.1)) });
        self.cur = a; self.grow(c); self.grow(a);
    }
    fn bounds(&self) -> Rectangle<Twips> { let (x0, x1, y0, y1) = self.bb.unwrap_or((0, 0, 0, 0));
        Rectangle { x_min: Twips::new(x0), x_max: Twips::new(x1), y_min: Twips::new(y0), y_max: Twips::new(y1) } }
}
impl OutlinePen for SwfGlyphPen {
    fn move_to(&mut self, x: f32, y: f32) {
        self.close();
        let p = self.q(x as f64, y as f64);
        self.recs.push(ShapeRecord::StyleChange(Box::new(StyleChangeData {
            move_to: Some(Point::new(Twips::new(p.0), Twips::new(p.1))),
            fill_style_0: if self.first { Some(1) } else { None },   // as Flash writes it (test_data.rs:546)
            fill_style_1: None, line_style: None, new_styles: None })));
        self.first = false; self.open = true; self.cur = p; self.start = p; self.fu = (x as f64, y as f64); self.grow(p);
    }
    fn line_to(&mut self, x: f32, y: f32) { let a = self.q(x as f64, y as f64); self.edge(a); self.fu = (x as f64, y as f64); }
    fn quad_to(&mut self, cx: f32, cy: f32, x: f32, y: f32) {
        let (c, a) = (self.q(cx as f64, cy as f64), self.q(x as f64, y as f64)); self.quad(c, a); self.fu = (x as f64, y as f64); }
    fn curve_to(&mut self, c1x: f32, c1y: f32, c2x: f32, c2y: f32, x: f32, y: f32) {   // CFF/CFF2 -> quads
        let cb = kurbo::CubicBez::new(self.fu, (c1x as f64, c1y as f64), (c2x as f64, c2y as f64), (x as f64, y as f64));
        let tol = 0.25;                                          // font units (~1/4000 em at upem 1000)
        let quads: Vec<kurbo::QuadBez> = match cb.approx_spline(tol) {
            Some(s) => s.to_quads().collect(), None => cb.to_quads(tol).map(|(_, _, q)| q).collect() };
        for q in quads { let (c, a) = (self.q(q.p1.x, q.p1.y), self.q(q.p2.x, q.p2.y)); self.quad(c, a); }
        self.fu = (x as f64, y as f64);
    }
    fn close(&mut self) { if self.open { let s = self.start; self.edge(s); self.open = false; } }
}

/// Unique u16 codes, ascending: cmap code (prefer U+0600.. over presentation forms) -> presentation form -> PUA.
fn assign_codes(font: &FontRef, used: &BTreeSet<u32>) -> BTreeMap<u16, u32> {
    let is_pf = |c: u32| (0xFB50..=0xFDFF).contains(&c) || (0xFE70..=0xFEFF).contains(&c);
    let mut best: HashMap<u32, u32> = HashMap::new();
    for (cp, gid) in font.charmap().mappings() {
        let g = gid.to_u32();
        if cp > 0xFFFF || !used.contains(&g) { continue; }
        best.entry(g).and_modify(|c| if is_pf(*c) && !is_pf(cp) { *c = cp }).or_insert(cp);
    }
    let (mut out, mut pua) = (BTreeMap::new(), 0xE000u32);
    for &g in used {
        let mut code = best.get(&g).copied().filter(|c| !out.contains_key(&(*c as u16)));
        if code.is_none() { while out.contains_key(&(pua as u16)) { pua += 1; } code = Some(pua); pua += 1; }
        out.insert(code.unwrap() as u16, g);
    }
    out
}

pub struct FontChunk<'n> { pub font: Font<'n>, pub index_of: HashMap<u32 /*gid*/, u32 /*SWF glyph index*/> }

pub fn build_define_font3<'n>(face: &LoadedFace, used: &BTreeSet<u32>, name: &'n str, next_id: &mut CharacterId) -> Vec<FontChunk<'n>> {
    let font = face.font();
    let m = font.metrics(Size::unscaled(), LocationRef::default());
    let k = SWF_EM / m.units_per_em as f64;                       // FU -> DefineFont3 units
    let gm = font.glyph_metrics(Size::unscaled(), LocationRef::default());
    let outlines = font.outline_glyphs();
    let glyphs: Vec<(u32, Glyph)> = assign_codes(&font, used).into_iter().map(|(code, gid)| {
        let mut pen = SwfGlyphPen::new(k);
        if let Some(g) = outlines.get(GlyphId::new(gid)) {
            let _ = g.draw(DrawSettings::unhinted(Size::unscaled(), LocationRef::default()), &mut pen);
        }
        pen.close();
        let advance = (gm.advance_width(GlyphId::new(gid)).unwrap_or(0.0) as f64 * k).round().clamp(0.0, 65535.0) as u16;
        let bounds = Some(pen.bounds());
        (gid, Glyph { shape_records: pen.recs, code, advance, bounds })    // empty shape for spaces is fine
    }).collect();
    let mut flags = FontFlag::HAS_WIDE_CODES | FontFlag::HAS_LAYOUT;       // never IS_ANSI (write.rs:2010)
    flags.set(FontFlag::IS_BOLD, face.bold);
    flags.set(FontFlag::IS_ITALIC, face.italic);
    let layout = FontLayout { ascent: (m.ascent as f64 * k).round() as u16, descent: (m.descent.abs() as f64 * k).round() as u16,
                              leading: (m.leading as f64 * k).round() as i16, kerning: vec![] }; // kerning baked into DefineText
    let make = |part: &[(u32, Glyph)], id: CharacterId| Font { version: 3, id, name: SwfStr::from_bytes(name.as_bytes()),
        language: Language::Unknown, layout: Some(layout.clone()), glyphs: part.iter().map(|g| g.1.clone()).collect(), flags };
    let (mut out, mut stack) = (Vec::new(), vec![&glyphs[..]]);
    while let Some(part) = stack.pop() {                           // keep each tag < 64 KiB (F11)
        if part.len() > 1 && encoded_len(&make(part, 0)) > FONT_TAG_BUDGET {
            let (a, b) = part.split_at(part.len() / 2); stack.push(b); stack.push(a); continue;
        }
        let id = *next_id; *next_id += 1;
        out.push(FontChunk { font: make(part, id), index_of: part.iter().enumerate().map(|(i, g)| (g.0, i as u32)).collect() });
    }
    out
}
fn encoded_len(f: &Font) -> usize {
    let mut v = Vec::new();
    swf::write_swf(&swf::Header::default_with_swf_version(8), &[SwfTag::DefineFont2(Box::new(f.clone()))], &mut v).unwrap();
    v.len()
}

// ------------------------------------------------------------------ DefineText
/// left_twips: XFL DOMStaticText@left * 20 -> DefineText.matrix (as Flash does, F5). PlaceObject gets the XFL <matrix>.
pub fn build_define_text(id: CharacterId, left_twips: i32, glyphs: &[PositionedGlyph],
                         map: &HashMap<(FaceKey, u32), (CharacterId, u32)>,
                         glyph_bounds: impl Fn(FaceKey, u32) -> Rectangle<Twips> /* DefineFont3 units */) -> SwfTag<'static> {
    let r = |v: f64| v.round() as i32;
    let mut records: Vec<TextRecord> = Vec::new();
    let mut cur: Option<(CharacterId, u16, Color, i32)> = None;     // font, height, color, baseline y
    let mut bounds = Rectangle::<Twips>::INVALID;
    for (i, g) in glyphs.iter().enumerate() {
        let (font_id, index) = map[&(g.face, g.glyph_id)];
        let (x, y) = (r(g.x), r(g.y));
        assert!(x.abs() <= i16::MAX as i32 && y.abs() <= i16::MAX as i32, "TextRecord offsets are i16");
        let style = (font_id, g.size_twips, g.color, y);
        let full = records.last().map_or(true, |rec| rec.glyphs.len() == 255);
        if cur != Some(style) || full {
            let font_changed = cur.map(|c| (c.0, c.1)) != Some((font_id, g.size_twips));
            records.push(TextRecord {
                font_id: font_changed.then_some(font_id),                       // font + height are tied
                height: font_changed.then_some(Twips::new(g.size_twips as i32)),
                color: (cur.map(|c| c.2) != Some(g.color)).then_some(g.color),  // first record sets all three
                x_offset: Some(Twips::new(x)),                                  // always explicit (absolute)
                y_offset: (cur.map(|c| c.3) != Some(y)).then_some(Twips::new(y)), // raised/lowered marks -> own record
                glyphs: Vec::new() });
            cur = Some(style);
        }
        // signed advance to the next drawing position: absorbs mark x_offsets exactly
        let next_x = glyphs.get(i + 1).map_or(x, |n| r(n.x));
        records.last_mut().unwrap().glyphs.push(GlyphEntry { index, advance: next_x - x });
        let s = g.size_twips as f64 / SWF_EM; let b = glyph_bounds(g.face, g.glyph_id);
        for (bx, by) in [(b.x_min, b.y_min), (b.x_max, b.y_max)] {
            bounds = bounds.encompass(Point::new(Twips::new(left_twips + x + r(bx.get() as f64 * s)), Twips::new(y + r(by.get() as f64 * s))));
        }
    }
    let text = Text { id, bounds, matrix: Matrix::translate(Twips::new(left_twips), Twips::ZERO), records };
    if glyphs.iter().any(|g| g.color.a != 255) { SwfTag::DefineText2(Box::new(text)) } else { SwfTag::DefineText(Box::new(text)) }
}

// ------------------------------------------------------------------ publish orchestration
// 1) layout every DOMStaticText -> Vec<PositionedGlyph> (the stage canvas renders from the same output)
// 2) used: BTreeMap<FaceKey, BTreeSet<u32>> = union of glyph ids across the whole document
// 3) per face: build_define_font3 -> chunks; map[(face, gid)] = (chunk.font.id, chunk.index_of[gid])
// 4) emit DefineFont3 tags before first use; one DefineText per DOMStaticText; PlaceObject2 with XFL matrix
// helpers flatten/split_paragraphs/clip/style_at/trim_ws/trim_ws_end/is_space/line_metrics/add_after_each_cluster omitted
```
