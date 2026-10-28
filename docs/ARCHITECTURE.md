# Al-Jazari architecture (milestone 1)

This document is the contract the code is built against. The research behind it
lives in `docs/research/*.md`: each file is a researcher's report checked line
by line by a second agent. Where a correction is given, it overrides the claim.
Read the file for your area before writing code.

## 1. Milestone 1 scope

A vertical slice through every layer of the tool:

1. **Open** an XFL document: an uncompressed folder (`DOMDocument.xml`, or the
   `*.xfl` stub inside it) or a CS5+ `.fla` ZIP. Pre-CS5 binary FLA (OLE2 magic
   `D0 CF 11 E0`) is refused with a message suggesting JPEXS FLA Convert.
2. **Show** it: the stage at any frame, a timeline with layers, frames,
   keyframes and tweens, the library, document and frame properties, frame
   scripts (read-only), and an Output panel.
3. **Compile** ActionScript 2 frame scripts, button `on()` handlers and
   `onClipEvent()` handlers with Rascal behind our own front-end.
4. **Publish** a SWF version 8 file (zlib `CWS`).
5. **Test Movie**: run the published SWF in an embedded Ruffle player with
   `trace()` output in the Output panel.
6. **GPU/CPU switch** in settings: the same wgpu renderer on a hardware
   adapter or on a software one (WARP on Windows, lavapipe/llvmpipe on Linux).
   It takes effect after a restart.
7. **Static text, including Arabic**, is shaped and laid out by us and
   published as glyph runs (DefineFont3 + DefineText), so it renders correctly
   in Flash Player 8 and in Ruffle.

Out of scope for M1 (warn, never crash): editing tools, saving XFL, shape
tweens (publish keyframes only), motion guides (tween without the path),
sounds, video, camera/layer depth, IK, Arabic in dynamic/input text, AS2
classes from external `.as` files (best effort). Scene gotos are resolved at
compile time as Flash 8 does (section 6, Scenes); computed frames
(`gotoAndPlay(expr)`) carry the scene bias, which is unverified against Flash.

## 2. Hard constraints

- **Windows 11 and Linux** (Ubuntu 24.04, Mint 22). CI later on
  `windows-latest` and `ubuntu-24.04`.
- **Portable**: a single executable; no installer; no runtime dependency
  beyond the standard desktop system libraries (no Java, no WebView, no VC++
  redistributable: Windows builds use `+crt-static`). Fonts and other assets
  are embedded with `include_bytes!`. Settings and layout live in `JazariData/`
  next to the executable when that is writable. Otherwise they go in the
  platform config dir (`dirs::config_dir()/Al-Jazari`). The `JAZARI_HOME`
  environment variable overrides both.
- **Never put GPU load on the development machine** (its UPS cuts power).
  Every test, tool and agent run uses the software adapter. The app honours
  `JAZARI_FORCE_CPU=1`: CPU mode is forced, the GPU option is disabled in the
  UI, and the flag is never overwritten in saved settings. The machine-local
  `C:\dev\rust\cargo\config.toml` sets it in `[env]`.
- **ASCII paths only** for the checkout, `CARGO_HOME` and target dirs: Ruffle's
  build script passes paths to `java`.
- **Licensing**: our code is public domain (Unlicense). Vendored third-party
  code keeps its licence and notices (`vendor/*/LICENSE*`). Release bundles
  must ship `THIRD-PARTY-LICENSES` (generated later with cargo-about) and the
  OFL text for embedded fonts.

## 3. Build environment

| Item | Value |
|---|---|
| Rust | stable 1.99.1, `CARGO_HOME=C:\dev\rust\cargo`, `RUSTUP_HOME=C:\dev\rust\rustup` |
| Java (build only) | `C:\Program Files\Eclipse Adoptium\jdk-21.0.12.101-hotspot\bin` on `PATH` (Ruffle ignores `JAVA_HOME`) |
| C++ | VS 2022 Build Tools |
| Env scripts | `tools/env.sh` (Git Bash) and `tools/env.ps1` set all of the above |

Agent shells do not inherit an up-to-date `PATH`, so source the env script
first. The dev profile optimises dependencies (`[profile.dev.package."*"]
opt-level = 2`), so Ruffle is usable in debug builds. Our crates stay at
opt-level 0. `panic = "unwind"` everywhere, because the AS2 compiler and
Ruffle are wrapped in `catch_unwind`.

## 4. Workspace layout

```
Cargo.toml                 workspace, [workspace.dependencies], [patch.crates-io]
crates/
  jazari-xfl/              XFL reader + document model (no swf, no GUI)
  jazari-as2/              AS2 front-end over Rascal: handlers, diagnostics, panics
  jazari-text/             fonts, shaping (harfrust), bidi, line breaking, outlines
  jazari-publish/          Document -> SWF8 bytes (release build and stage build)
  jazari-preview/          Ruffle embedding: shared-device player, offscreen renderer
  jazari-app/              eframe GUI, binary name `jazari`
  jazari-cli/              binary `jazari-cli`: dump, publish, render, compare
vendor/rascal/             Rascal 0.3.6 (MIT OR Apache-2.0), patched by us
assets/fonts/              embedded OFL fonts + OFL.txt
tests/fixtures/<name>/     Flash 8 oracle fixtures: <name>.fla, flash8.swf, xfl/
tools/                     env scripts, oracle (JSFL) helpers
docs/                      this file, research notes
```

Dependency direction: `xfl` ← `publish` → `as2`, `text`; `preview` depends on
Ruffle only; `app` and `cli` depend on everything. `publish` must never depend
on Ruffle: it stays fast to build and test.

### Pinned dependencies

- Ruffle git `https://github.com/ruffle-rs/ruffle`, rev
  `07d3e430631333f80f754df01a968bdcd80d98c5`:
  - `ruffle_core` with features `default_font`, `audio`, `mp3`, `nellymoser`;
  - `ruffle_render_wgpu`;
  - `ruffle_render`.
  - Do not use `ruffle_frontend_utils` (reqwest/OpenSSL) or `ruffle_desktop`.
- `swf` from the same git rev, plus `[patch.crates-io] swf = { git, rev }`, so
  Rascal, `jazari-publish` and `ruffle_core` share one copy of the crate.
- `rascal = "=0.3.6"`, patched to `vendor/rascal` through `[patch.crates-io]`.
- `eframe = "0.36.2"` (wgpu 30, matching Ruffle's lock), `egui_dock = "0.21.1"`,
  `rfd = { version = "0.17.2", default-features = false, features = ["xdg-portal", "wayland"] }`,
  `dirs = "6"`, `serde`, `toml`.
- `quick-xml = "0.37"`, `zip = { version = "9", default-features = false, features = ["deflate"] }`,
  `flate2 = "1"`.
- Text: `harfrust = "=0.12.0"`, `skrifa = "0.44"`, `fontdb = "0.24"`,
  `unicode-bidi = "0.3.18"`, `unicode-linebreak = "0.1.5"`,
  `unicode-script = "0.5"`, `kurbo = "0.13"`.
- Errors and logging: `thiserror`, `anyhow` (binaries only), `log`, `tracing`.

Seed `Cargo.lock` from Ruffle's lock where versions overlap. Never let two
`wgpu` versions into the tree: check with `cargo tree -i wgpu`.

## 5. Document model (`jazari-xfl::model`)

The model is the in-memory authoring document. Coordinates are **pixels as
`f64`**, except shape edges, which keep **twips as `f64`**, because XFL edge
numbers are twips and may be fractional. Layer order follows XFL: index 0 is the
top layer.

```rust
pub struct Document {
    pub width: f64, pub height: f64,          // px
    pub frame_rate: f64,
    pub background: Rgba,
    pub xfl_version: String,
    pub scenes: Vec<Timeline>,                // one per DOMTimeline, in order
    pub library: Library,
    pub publish: PublishSettings,             // from PublishSettings.xml, with defaults
    pub source: SourceInfo,                   // path, container kind, base dir for #include
}
pub struct Library {
    pub symbols: IndexMap<String, Symbol>,    // key = DOMSymbolItem@name (full path "folder/Name")
    pub bitmaps: IndexMap<String, BitmapItem>,
    pub fonts: IndexMap<String, FontItem>,
    pub folders: Vec<String>,
}
pub struct Symbol { pub name: String, pub kind: SymbolKind, pub timeline: Timeline,
                    pub linkage: Option<Linkage>, pub scale9: Option<Rect> }
pub enum SymbolKind { MovieClip, Graphic, Button }
pub struct Linkage { pub identifier: String, pub export_for_as: bool,
                     pub export_in_first_frame: bool, pub class_name: Option<String> }
pub struct Timeline { pub name: String, pub layers: Vec<Layer> }
pub struct Layer {
    pub name: String, pub kind: LayerKind, pub parent: Option<usize>,  // parentLayerIndex
    pub visible: bool, pub locked: bool, pub outline: bool, pub color: Rgba,
    pub frames: Vec<Frame>,                  // keyframe spans, sorted by index
}
pub enum LayerKind { Normal, Guide, Mask, Folder, Other(String) }      // camera etc. -> Other + warning
pub struct Frame {
    pub index: u32, pub duration: u32,
    pub label: Option<FrameLabel>,           // name + LabelKind {Name, Comment, Anchor}
    pub script: Option<String>,
    pub tween: Tween,
    pub elements: Vec<Element>,              // index 0 = bottom of the layer's stack (XFL order)
}
pub enum Tween { None, Motion(MotionTween), Shape(ShapeTween), Other(String) }
pub struct MotionTween { pub ease: f64 /* -100..100 */, pub custom_ease: Option<Vec<[f64; 2]>>,
                         pub rotate: RotateMode, pub rotate_times: u32, pub scale: bool,
                         pub orient_to_path: bool, pub sync: bool, pub snap: bool }
pub enum RotateMode { Auto, None, Clockwise, CounterClockwise }
pub struct ShapeTween { pub ease: f64, pub blend: String }
pub enum Element {
    Shape(Shape), Group(Group), Instance(SymbolInstance), Bitmap(BitmapInstance),
    Text(TextElement), Unsupported { tag: String },
}
pub struct Shape { pub matrix: Matrix, pub fills: Vec<FillStyle>, pub strokes: Vec<StrokeStyle>,
                   pub edges: Vec<Edge>, pub is_drawing_object: bool }
pub struct Edge { pub fill0: u32, pub fill1: u32, pub stroke: u32,   // 1-based, 0 = none
                  pub path: Vec<PathCmd> }                            // twips
pub enum PathCmd { MoveTo(Pt), LineTo(Pt), QuadTo(Pt, Pt) }
pub enum FillStyle { Solid(Rgba), Linear(Gradient), Radial(Gradient), Bitmap(BitmapFill) }
pub struct Gradient { pub matrix: Matrix, pub stops: Vec<GradientStop>, pub spread: Spread,
                      pub interpolation: Interp, pub focal_point: f64 }
pub struct StrokeStyle { pub weight: f64, pub fill: FillStyle, pub caps: Caps, pub joints: Joints,
                         pub miter_limit: f64, pub scale_mode: ScaleMode, pub pixel_hinting: bool,
                         pub hairline: bool, pub kind: String /* "SolidStroke", others -> warn */ }
pub struct Group { pub matrix: Option<Matrix>, pub members: Vec<Element> }
pub struct SymbolInstance {
    pub library_item: String, pub kind: SymbolKind, pub name: Option<String>,
    pub matrix: Matrix, pub transformation_point: Pt,
    pub color: ColorEffect,                   // normalised: mult[4] (0..1+), add[4] (-255..255)
    pub filters: Vec<Filter>, pub blend_mode: BlendMode, pub cache_as_bitmap: bool,
    pub graphic: Option<GraphicPlayback>,     // loop / play once / single frame + first_frame
    pub track_as_menu: bool,
    pub script: Option<String>,               // on()/onClipEvent() source, split by jazari-as2
}
pub struct BitmapInstance { pub library_item: String, pub matrix: Matrix }
pub struct BitmapItem { pub name: String, pub width: u32, pub height: u32,
                        pub data: BitmapData, pub smoothing: bool, pub linkage: Option<Linkage> }
pub enum BitmapData { Rgba { premultiplied: bool, pixels: Vec<u8> }, Jpeg { bytes: Vec<u8>, alpha: Option<Vec<u8>> } }
pub struct TextElement {
    pub kind: TextKind,                       // Static, Dynamic, Input
    pub matrix: Matrix, pub left: f64, pub width: f64, pub height: f64,
    pub runs: Vec<TextRun>, pub auto_expand: bool, pub line_type: LineType,
    pub selectable: bool, pub name: Option<String>, pub variable: Option<String>,
    pub max_chars: Option<u32>, pub border: bool, pub orientation: String,
}
pub struct TextRun { pub text: String, pub attrs: TextAttrs }
pub struct TextAttrs { pub face: String, pub size: f64 /* pt */, pub fill: Rgba,
                       pub align: Align, pub indent: f64, pub left_margin: f64, pub right_margin: f64,
                       pub line_spacing: f64, pub letter_spacing: f64, pub auto_kern: bool,
                       pub url: Option<String>, pub target: Option<String> }
pub struct Matrix { pub a: f64, pub b: f64, pub c: f64, pub d: f64, pub tx: f64, pub ty: f64 }  // tx/ty px
pub struct Rgba { pub r: u8, pub g: u8, pub b: u8, pub a: u8 }
```

The reader returns `(Document, ImportReport)`. `ImportReport` lists warnings
with a location: file, scene/symbol, layer, frame and element path. It never
aborts on unknown content: unknown elements become `Element::Unsupported`,
unknown layer types `LayerKind::Other`, unknown tweens `Tween::Other`.
`keyMode` is opaque: derive tween semantics from `tweenType` and explicit
attributes only. Bitmaps from `bin/*.dat` are decoded per JPEXS's format,
including the versioned lossless header (see `docs/research/xfl-format.md`,
corrections F24 and the missed point about `.dat` versions).

Helper functions live with the model: `Timeline::frame_count()`,
`Layer::frame_at(i) -> Option<&Frame>` (the keyframe span containing `i`),
`Matrix` maths in `f64` (multiply, invert, decompose and recompose for tweens).

## 6. Publisher (`jazari-publish`)

```rust
pub fn publish(doc: &Document, opts: &PublishOptions, fonts: &jazari_text::FontLibrary,
               as2: &jazari_as2::Compiler) -> PublishOutput;
pub struct PublishOptions { pub mode: BuildMode, pub compress: bool }
pub enum BuildMode { Release, Stage }
pub struct PublishOutput { pub swf: Option<Vec<u8>>, pub diagnostics: Vec<Diagnostic>,
                           pub frame_map: Vec<(usize /*scene*/, u32 /*frame*/)> }
```

Rules (details and citations in `docs/research/swf-writer.md`):

- **Header and tag order.** Use version 8 and `Compression::Zlib`. The first tag
  is FileAttributes (never AS3; UseNetwork from PublishSettings), then
  SetBackgroundColor. Definitions go just before first use, at root level.
  Write per frame, as Flash 8 does: RemoveObject2, DoAction (the scripts of
  several layers top layer first), FrameLabel, then each definition just
  before its first placement, PlaceObject2/3, ShowFrame. Never write
  `Tag::End` yourself.
- **Scenes** are concatenated into the root timeline, and `frame_map` records
  where each one lands. Nothing in the SWF names a scene. Scene gotos are
  resolved when compiling (`tests/fixtures/scenes`): frame scripts of a scene
  and button handlers on a scene timeline are compiled with a
  `jazari_as2::SceneContext` (every scene's name and first root frame), so
  `nextScene()`/`prevScene()` become a bare `GotoFrame` to the first frame of
  the next/previous scene (which stops the playhead, as in Flash), a bare
  `gotoAndPlay(n)` counts from the current scene's first frame,
  `gotoAndPlay("Scene", n)` from that scene's, and a scene-qualified label
  drops the scene. Symbol timelines and clip handlers get no scene context.
- **Depths.** Each timeline gets depth bands per layer, bottom layer lowest.
  Everything stays below 16384 (the AVM depth bias). Flattened graphic symbols
  use sub-bands inside their parent's band.
- **Instance identity across keyframes** follows Flash. The element at slot *k*
  of a layer persists across consecutive keyframes when the library item and
  kind are unchanged: emit a PlaceObject2 *Modify*. Otherwise emit Remove plus
  Place. Name, clip depth and clip actions are set at first placement only.
  When they change, re-place. A new placement of a movie clip after a
  timeline's first frame carries `ratio` = its 0-based frame (absolute on the
  root), as Flash 8 writes it; players use it on a backwards goto to replace
  a later instance with a fresh one (`display::mark_ratios`).
- **Shapes** become DefineShape3, or DefineShape4 when LineStyle2 or focal
  gradients are needed. `fill0/fill1/stroke` map 1:1. Re-specify all three
  selectors with every `new_styles`, and never emit an empty StyleChange.
  Split edges longer than 65535 twips. Cap gradients at 15 records. Bounds are
  the edge bounds grown by half the stroke width. Deduplicate identical shapes.
- **Groups** are flattened into their parent. In the XFL that JPEXS writes, a
  `DOMGroup` has no matrix and every member's matrix already includes all
  enclosing group transforms (`tests/fixtures/groups`), so member matrices
  are used as they are and never composed with a group matrix. A merge-mode
  raw shape, listed last by JPEXS, is drawn underneath its siblings, as Flash
  draws it. Grouped shapes have their transform baked into the edges and are
  placed with the identity matrix, as Flash 8 does.
- **Motion tweens.** Place on the keyframe, then one Modify per in-between
  frame. Interpolate in `f64`, decomposing into scale, skew, rotation and
  translation with the rotate mode and times, and round once to twips and
  Fixed16. Colour effects and matching filter lists interpolate too. Calibrate
  the ease curve and the decomposition against the matrices in the Flash 8
  reference SWFs of the `tweens` fixture: they must agree to within 1 twip.
  As calibrated (`src/tween/`, `tests/fixtures/tween-probes`): progress is
  the 16.16 value `round(k*65536/n)` and drives matrix, colour and filters;
  rotation uses Flash 8's quarter-degree sine table; the transformation point
  moves on a straight line; rotate *none* does not rotate; colour keyframes
  are rounded to SWF units before interpolating; filter lists are aligned
  like a diff by kind. The XFL-to-ease sign mapping lives in one place,
  `tween::ide_ease()` (JPEXS writes the sign flipped).
- **Graphic symbols** are flattened into the parent timeline frame by frame,
  honouring loop, play-once, single-frame and `first_frame`.
- **Movie clips** become DefineSprite (`num_frames` = number of ShowFrame tags).
  A clip that has neither scripts nor more than one frame may still be a
  sprite: keep it simple and correct.
- **Buttons** become DefineButton2 records built from frames 0 to 3 (up, over,
  down, hit) across all layers. An instance with `on()` handlers gets its own
  DefineButton2 copy carrying the condition actions.
- **Instance scripts.** `on()`/`onClipEvent()` on a movie clip become
  PlaceObject2 ClipActions: `all_event_flags` is the OR of the records, and
  `key_code` is set only for KEY_PRESS. Arabic characters cannot be keyPress
  codes: emit a diagnostic.
- **Masks.** As Flash 8 does (`tests/fixtures/masks`), only the backmost
  element of a mask layer keyframe is published (the others are reported as
  warnings). It is placed directly like ordinary content (its own DefineShape
  or sprite, with its instance name and motion tween) with `clip_depth` equal
  to the top depth of the masked layers. Only an element that would take more
  than one depth (a graphic symbol, a group holding instances) is wrapped in
  a generated one-frame sprite. A mask whose masked layers take no depth is
  left out. Guide layers are not published.
- **Bitmaps** become DefineBitsLossless2 (zlib, premultiplied ARGB), or
  DefineBitsLossless version 1 when opaque (as Flash 8 writes them), or
  DefineBitsJPEG2/3. `allowSmoothing` picks the smoothed or non-smoothed
  bitmap fill types. A bitmap instance becomes a bitmap-filled rectangle
  baked into the surrounding shape and placed with the identity matrix, as
  Flash does. The item's compression setting is not re-applied: the stored
  data is published as is.
- **Static text** is laid out with `jazari-text` and becomes one DefineFont3
  per face, holding the glyphs used by all texts: 20480 units per em,
  `HAS_WIDE_CODES | HAS_LAYOUT`, never `IS_ANSI`, and split before 60 KB. Each
  text becomes one DefineText, or DefineText2 when alpha is used. The first
  record sets font, height and colour. A record holds at most 255 glyphs.
  Marks with a y offset get their own record. As Flash 8 writes it: each
  text is followed by CSMTextSettings, the font name is NUL-terminated, a
  face's glyphs are split into DefineFont3 chunks of at most 60,000 bytes,
  and identical texts share one character. Fallback faces, `.notdef`
  glyphs, links, vertical text and range overflows are reported per element.
- **Dynamic and input text** becomes DefineEditText with device font, the
  initial text, the variable name and the layout. Arabic content gets a
  diagnostic: Ruffle and FP8 do not shape it. The text is always written as
  HTML, generated exactly as Flash 8 writes it, with a glyph-less DefineFont3
  per device face.
- **Filters and blend modes** become PlaceObject3, but only when used.
  Filter strengths and angles are truncated to fixed point, as Flash writes
  them. Adjust Color becomes a ColorMatrix filter computed with Flash's own
  algorithm (brightness, contrast table, saturation, hue, in that order;
  `filter::adjust_color_matrix`), equal to Flash 8's to f32 precision.
- **Frame scripts and handlers** are compiled through `jazari-as2`.
  Diagnostics carry the location (scene/symbol, layer, frame or instance).
  A failed script is skipped and the publish continues.
- **Output.** `PublishOutput.frame_map` has one entry per root SWF frame.
  Diagnostics are `jazari_as2::Diagnostic`, located by `Origin` (timeline =
  scene or symbol path, layer, 0-based keyframe start, instance or element
  description, or a file for document-level messages).
- **Stage build** (`BuildMode::Stage`), used by the editor's stage view:
  - every user script is omitted;
  - every DefineSprite gets `stop();` on frame 1;
  - root frame 1 gets `stop();` plus a callback registered with
    `ExternalInterface.addCallback("jz_goto", null, function (f) { _root.gotoAndStop(f); })`.
    The host calls it through `Player::call_internal_interface`. If Rascal
    cannot compile the fully qualified name, emit the bytecode with the `swf`
    crate's AVM1 writer.
- **Validation.** After writing, read the bytes back with
  `swf::decompress_swf` + `swf::parse_swf` and check what the writer
  truncates silently. Every emitted tag type has a round-trip unit test.
- **Calibration.** Before guessing how Flash orders tags, scripts or depths,
  parse the Flash 8 reference SWF of a fixture with the `swf` crate and copy
  what it does.

## 7. AS2 front-end (`jazari-as2`)

```rust
pub struct Compiler { /* swf_version: 8, class paths, include base dir, cache */ }
impl Compiler {
    pub fn new(opts: CompilerOptions) -> Self;
    pub fn compile_frame(&self, src: &str, origin: &Origin) -> Result<Vec<u8>, Vec<Diagnostic>>;
    pub fn compile_button_handlers(&self, src: &str, origin: &Origin) -> Result<Vec<ButtonHandler>, Vec<Diagnostic>>;
    pub fn compile_clip_handlers(&self, src: &str, origin: &Origin) -> Result<Vec<ClipHandler>, Vec<Diagnostic>>;
}
pub struct ButtonHandler { pub conditions: swf::ButtonActionCondition, pub key_code: Option<u8>, pub actions: Vec<u8> }
pub struct ClipHandler { pub events: swf::ClipEventFlag, pub key_code: Option<u8>, pub actions: Vec<u8> }
// Added at integration, for scripts of a scene timeline (scene gotos resolve
// at compile time, see section 6, Scenes):
pub struct SceneContext { pub scenes: Vec<(String, u32 /* first root frame */)>, pub current: usize }
impl Compiler {
    pub fn compile_frame_in(&self, src: &str, origin: &Origin, scenes: Option<&SceneContext>) -> ...;
    pub fn compile_button_handlers_in(&self, src: &str, origin: &Origin, scenes: Option<&SceneContext>) -> ...;
}
pub struct Diagnostic { pub severity: Severity, pub message: String, pub origin: Origin, pub line: u32, pub column: u32 }
```

- **Compiling.** Compile each snippet with Rascal at SWF version 8 from an
  in-memory source provider, then take the DoAction payload out of the
  resulting SWF (see `docs/research/rascal.md`).
- **Splitting handlers.** Our own tokenizer, which respects strings and
  comments, splits `on(...)` and `onClipEvent(...)` blocks. Each body is
  compiled with everything outside it blanked to spaces, so line and column
  numbers stay true. Snippets that still contain `on(`/`onClipEvent(` at top
  level are rejected.
- **Pre-checks.** Strip the BOM. Reject `@` outside strings and comments with
  a diagnostic, because it panics Rascal's lexer.
- **Isolation.** Wrap every compile in `catch_unwind`, so a panic becomes a
  Diagnostic.
- **Patches to `vendor/rascal`**, the high-value gaps, each with tests:
  1. postfix and prefix `++`/`--` on member and index expressions (`this._x++`);
  2. `do { } while ( )`;
  3. `\uXXXX`, `\xNN`, `\b`, `\f` and octal escapes in string literals;
  4. panics turned into errors where feasible.

  5. structured diagnostics (`Error::diagnostics`);
  6. scenes: `CompileOptions::with_scenes(Scenes)` resolves scene gotos in
     scripts through a new HIR variant, so builds without scenes are
     unchanged and Ruffle's `playerglobal_avm1.swf` stays byte-identical.

  Keep each patch small and upstreamable, and record it in
  `vendor/rascal/PATCHES.md`.
- **Caching.** Cache compiled output by a hash of the source and its includes.

## 8. Text (`jazari-text`)

```rust
pub struct FontLibrary { /* fontdb system scan + fonts/ next to exe and document + embedded */ }
impl FontLibrary {
    pub fn new() -> Self;
    pub fn resolve(&self, face: &str) -> FontId;
    pub fn glyph_outline(&self, font: FontId, glyph: u16) -> Vec<PathCmd>;    // em units, y down, quads only
    pub fn metrics(&self, font: FontId) -> FontMetrics;
}
pub fn layout(lib: &FontLibrary, block: &TextBlock) -> TextLayout;
// TextBlock: runs (text + attrs), width, auto_expand, line_type
// TextLayout: lines -> PositionedGlyph { font, glyph, x, y (twips f64), advance, code: u16 }
```

- **Shaping and bidi.**
  - Shape with harfrust per bidi run and script.
  - Use unicode-bidi for visual order and unicode-linebreak for greedy
    wrapping on logical advances, then reshape each line with its context.
  - Apply alignment, indents, margins, line spacing and letter spacing.
- **Outlines.** Read them with skrifa. TrueType quadratics pass through;
  CFF cubics are approximated with kurbo.
- **Font resolution.** Match the XFL face as a PostScript name first, then as
  a family name. Fall back to the embedded fonts:
  - Noto Sans Arabic and Noto Naskh Arabic for Arabic;
  - Noto Sans for Latin and common punctuation, which the Arabic Noto fonts
    lack.
- **Glyph codes** for the DefineFont3 code table: the cmap code, otherwise the
  Arabic Presentation Form, otherwise a Private Use code.
- **Advances** are whole twips, rounded as Flash 8 does: the nominal (`hmtx`)
  advance and the shaper's adjustment (kerning) are rounded separately and
  then added. The text fixture's Latin records equal Flash 8's exactly.
- **Tests.** Shape and position golden tests (CPU only) cover lam-alef, harakat
  and mixed digits.

## 9. Preview (`jazari-preview`)

```rust
pub fn wgpu_setup(mode: RenderMode) -> eframe::egui_wgpu::WgpuSetup;       // CPU: DeviceType::Cpu adapter
pub fn descriptors_from(rs: &eframe::egui_wgpu::RenderState) -> Arc<Descriptors>;
pub struct PreviewPlayer { .. }
impl PreviewPlayer {
    pub fn new(desc: Arc<Descriptors>, swf: Vec<u8>, size_px: (u32, u32), ppp: f32,
               log: std::sync::mpsc::Sender<LogLine>) -> anyhow::Result<Self>;
    pub fn tick(&mut self, dt: Duration);   pub fn render(&mut self);
    pub fn texture_view(&self) -> &wgpu::TextureView;  pub fn resize(&mut self, w: u32, h: u32, ppp: f32);
    pub fn goto_and_stop(&mut self, frame: u32);        // stage build: jz_goto
    pub fn set_playing(&mut self, on: bool);  pub fn handle_event(&mut self, ev: &egui::Event, rect: egui::Rect);
    pub fn is_halted(&self) -> bool;                    // AVM halted after a timeout -> offer restart
}
pub fn render_offscreen(swf: &[u8], frames: &[u32], scale: f32) -> anyhow::Result<Vec<image::RgbaImage>>;  // own WARP device
```

- **Device and target.** Share eframe's device: `WgpuSetupCreateNew` with
  Ruffle's device limits and features and the instance flags without
  `VALIDATION_INDIRECT_CALL`. Render through our own `RenderTarget`
  (Rgba8Unorm, RENDER_ATTACHMENT | TEXTURE_BINDING). Register the texture with
  egui once, and again on resize.
- **Input.** Forward pointer events in physical pixels, send `MouseLeave` when
  the pointer exits, and send key and TextControl events while the preview is
  focused (use the focus-lock filter for arrows and Tab).
- **Output panel.** It shows the LogBackend output plus a tracing layer for
  Ruffle's WARN-level messages, which never reach the LogBackend.
- **Repaints.** Request a repaint only while the player is playing or needs a
  render.
- **Fonts.** Register Noto Sans Arabic as a device font.
- **Rebuilds.** `catch_unwind` around Ruffle calls. On a panic or a halt,
  rebuild the player instead of poisoning a mutex.
- **Offscreen renderer** for the CLI and golden tests: it always uses the
  software adapter.

## 10. Application (`jazari-app`, binary `jazari`)

- **Startup.**
  - Read the settings: TOML in the portable data dir.
  - Resolve the RenderMode; `JAZARI_FORCE_CPU` wins.
  - Run eframe with `wgpu_setup`.
  - If GPU mode fails, fall back once to CPU and show a notice.
- **Panels** (egui_dock):
  - **Stage**: a PreviewPlayer of the stage build, letterboxed, with zoom.
  - **Timeline**: custom painted. Layer rows with type icons and hidden/locked
    state; frame cells, keyframe dots, tween arrows, labels and the playhead.
    Clicking sets the current frame, which calls `goto_and_stop`. Enter plays
    the stage timeline by stepping frames at the document frame rate. There is
    a scene selector.
  - **Library**: a tree of items with their kinds.
  - **Properties**: document size, fps, background and version; the selected
    layer, frame or tween (read-only).
  - **Actions**: the script of the selected frame (read-only in M1).
  - **Output**: traces, import report, compiler diagnostics and publish
    diagnostics, each clickable to its location where possible.
  - **Test Movie**: a PreviewPlayer of the release build.
- **Menus.**
  - File: Open… (a folder or a `.fla`), Recent, Publish… (save SWF), Exit.
  - Control: Test Movie (Ctrl+Enter), Play (Enter), Rewind.
  - Settings: Rendering GPU/CPU (needs a restart), shown with the active
    adapter name.
- **File dialogs** are asynchronous (rfd `AsyncFileDialog`, polled), so
  previews keep running while a dialog is open.
- **UI language** is English; every user-facing string goes through one
  `tr!()`/lookup function so it can be translated later. Layouts use
  start/end, not left/right, where egui allows it.

## 11. CLI (`jazari-cli`) and tests

```
jazari-cli dump <doc>                      # model summary + import report
jazari-cli publish <doc> <out.swf> [--stage]
jazari-cli render <file.swf> <outdir> [--frames 1,5,10|all] [--scale 1]   # PNG via software adapter
jazari-cli compare <doc> <reference.swf> <outdir>   # publish, render both, per-frame diff + trace diff
```

- **Golden tests** (`tests/golden.rs` in `jazari-cli`). For each
  `tests/fixtures/<name>`:
  1. Open `xfl/` and publish it.
  2. Render the selected frames of our SWF and of `flash8.swf` with the same
     Ruffle offscreen renderer.
  3. Compare the frames pixel by pixel with a tolerance.
  4. Compare the `trace()` sequences.

  Report the differences. Never "fix" a fixture to make a test pass.

  The table at the top of `golden.rs` holds, per fixture, the frames, the
  channel threshold, the tolerated share of differing pixels and whether the
  pixel and trace checks must pass. Every fixture whose output is right is
  `Pass`; a tolerance above zero is the measured difference rounded up, with
  its cause in the row's note. A row may list stage regions left out of the
  count where our output is right and Flash 8's is not (the text fixture's
  Arabic line); they are drawn blue in the diff panel and reviewed by eye.
  Groups of fixtures one level down (`tests/fixtures/tween-probes/<name>`)
  are discovered too.
- **Unit tests** in each crate run without Ruffle.

## 12. Acceptance for M1

- [x] `cargo build` and `cargo test` pass on Windows. Linux is checked later in CI.
- [x] `jazari-cli publish` succeeds on every fixture, and every output parses
      back with the `swf` crate.
- [x] Golden comparisons against Flash 8 output are within tolerance for
      shapes, gradients, symbols, tweens and bitmaps. `as2` traces match.
      `text` is visually correct, including Arabic: our output is correct
      where Flash 8's is wrong, so that case is reviewed by eye.
- [x] The app opens a fixture, shows stage, timeline and library, scrubs
      frames, runs Test Movie with traces, and publishes a SWF. All of this
      is verified in CPU mode.
- [x] `JAZARI_FORCE_CPU=1` keeps the GPU untouched by rendering work.

Checked at the end of the M1 integration (Windows only; GPU mode, Linux and
real mouse/keyboard input were not exercised).

## 13. Deviations accepted at integration

The crate builders' notes list every addition to the APIs above (extra model
fields such as `TextElement::top`, `FontLibrary` helpers, the preview's
`OffscreenRenderer`/`run_headless`, the app's developer flags). Those that
change behaviour, and are accepted:

- **jazari-xfl.** Lossless `.dat` bitmaps keep Flash's premultiplied plane as
  is (`BitmapData::Rgba { premultiplied: true }`); `MotionTween::ease` is the
  raw XFL acceleration (the sign is decided in `tween::ide_ease()`); an
  absent `motionTweenRotate` is *none*. A group member without a matrix
  cannot be told from one with the identity (no corpus has one).
- **jazari-as2.** Every compile runs on its own thread inside
  `catch_unwind`. There is no warnings channel on success, so constructs
  Flash compiles silently (an unknown scene name, for example) are silent
  here too.
- **jazari-text.** Only multi-line static text wraps; vertical metrics come
  from OS/2 (typo when `USE_TYPO_METRICS`, else win); alignment is absolute
  (left is the left edge even for right-to-left paragraphs); letter spacing
  is not applied to Arabic. The Arabic layout has no oracle: Flash 8 draws
  it unjoined, left to right.
- **jazari-preview.** `is_halted()` covers both halts and crashes; halts are
  not rebuilt automatically. Offscreen frame N means N `run_frame` calls, so
  timers do not fire offscreen.
- **jazari-publish.** Shapes equal up to scale are not shared as Flash 8
  shares them (a few anti-aliasing pixels, masks fixture); selectable,
  linked and device-font static text stay glyph runs where Flash 8 turns
  them into DefineEditText (reported as Info); `omit_trace` is not
  supported (Info).
- **jazari-app.** `--frame N` is a root frame of the movie and opens the
  scene that holds it. Hidden developer flags (`--test-movie`, `--test-swf`,
  `--stage-swf`, `--select-layer`, `--open-menu`, `--show-about`,
  `--dump-output`, `--screenshot-delay`, `--publish-to`) drive the app for
  automatic CPU-only checks. A Window menu toggles panels.
