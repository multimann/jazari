# Research: swf-writer (verified)

## Recommendation

Take `swf` from Ruffle git at the same rev as ruffle_core (07d3e430631333f80f754df01a968bdcd80d98c5). Add `[patch.crates-io] swf = { git = ..., rev = ... }` so that ruffle_core (which does `pub use swf;`), Rascal 0.3.6 (which needs crates.io `swf = "0.3.0"`) and the publisher all compile one shared copy. The pinned rev is API-identical to crates.io 0.3.0 plus two AVM1-writer bug fixes. The publisher crate should depend on `swf` directly, not on ruffle_core, so it stays light and testable. Everything is pure Rust with no runtime dependencies, which fits the portable app.

Publish with `swf::write_swf(&Header { compression: Compression::Zlib, version: 8, stage_size in twips, frame_rate: Fixed8, num_frames }, &tags, &mut Vec<u8>)`. Build all owned bytes and strings first, then a `Vec<Tag<'a>>` borrowing them. Never add `Tag::End` yourself.

Tag order, matching Flash CS6 output:
- FileAttributes (first, without IS_ACTION_SCRIPT_3), then [Metadata + HAS_METADATA], then SetBackgroundColor.
- Then per frame: RemoveObject2 for vanished depths, then definitions just before first use, all at root level.
- ExportAssets right after the definition it names; DoAction; PlaceObject2 (PlaceObject3 only for filters/blend/cacheAsBitmap); DoInitAction; ShowFrame.

Shapes:
- Use DefineShape3, or DefineShape4 when LineStyle2 or focal gradients are needed.
- Map XFL fillStyle0/fillStyle1/strokeStyle 1:1 to fill_style_0/fill_style_1/line_style (1-based, 0 = none).
- Re-specify all three selectors together with new_styles, and never emit an empty StyleChange.
- Split edges longer than 65535 twips and cap gradients at 15 records.
- Set shape_bounds = edge bounds grown by half the stroke width.

Motion tweens: emit Place on the first frame, then one PlaceObject2 Modify per frame carrying the interpolated, nearest-twip-rounded matrix and colour transform.

Graphic symbols: flatten them into the parent timeline with composed matrices rather than making sprites; this is proven for single-frame graphics and inferred for multi-frame loop/play-once/single-frame.

Movie clips: DefineSprite with num_frames equal to its ShowFrame count. Buttons: DefineButton2 with conditions mapped as Ruffle does. onClipEvent handlers: ClipActions with all_event_flags as the OR of the records, and key_code exactly for KEY_PRESS.

Text: DefineFont3 at a 20480 EM scale with HAS_WIDE_CODES | HAS_LAYOUT, plus HAS_WIDE_OFFSETS when the glyph table is over 64 KB. Render Arabic as pre-shaped DefineText (visual order, records of at most 255 glyphs, font_id and height together), because Ruffle cannot shape or reorder text in edit fields.

After every publish, run a validation pass for the writer's silent truncations, then read the file back with `decompress_swf` + `parse_swf` before handing the bytes to Ruffle.

## Dependencies

- swf (git https://github.com/ruffle-rs/ruffle rev 07d3e430631333f80f754df01a968bdcd80d98c5; package version 0.3.0; default features flate2+lzma)
- [patch.crates-io] swf -> same git rev (so rascal 0.3.6, which requires swf = "0.3.0", shares the single copy)
- ruffle_core (same git rev) — only in the preview crate; it re-exports `swf`
- rascal 0.3.6 (AS2 bytecode; returns a full SWF via CompiledProgram::to_swf, so DoAction/DoInitAction bytes must be extracted and class sprite ids remapped)
- flate2 1.1.x (pure-Rust miniz_oxide backend) to zlib DefineBitsLossless payloads and JPEG3 alpha planes
- optional: bumpalo 3.20.3 as an arena for owned bytes/strings borrowed by Tag<'a>
- optional (Arabic static text): rustybuzz 0.20.1 or another HarfBuzz port for shaping before emitting DefineFont3 + DefineText

## Findings (verifier status in brackets; corrections override the claim)

- **F1** [confirmed] Write API: `pub fn write_swf<W: Write>(header: &Header, tags: &[Tag<'_>], output: W) -> swf::error::Result<()>`, re-exported as `swf::write_swf`. There is also `write_swf_raw_tags(header, tags: &[u8], output)` for a tag stream that is already serialized. The internal `Writer` is private, so no public function serializes a single tag. The whole movie must be passed as one `&[Tag]`; a DefineSprite holds its own `Vec<Tag>`. `Header { compression: Compression, version: u8, stage_size: Rectangle<Twips>, frame_rate: Fixed8, num_frames: u16 }`. `Compression::{None, Zlib, Lzma}` writes the signatures "FWS", "CWS" and "ZWS". The uncompressed length is body+8, little-endian. Zlib uses flate2 `Compression::best()`. `SwfBuf { header: HeaderExt, data: Vec<u8> }` exists only on the read side: `decompress_swf` returns it and `parse_swf(&SwfBuf) -> Swf<'_>` reads from it. Use them to read back and verify a written file.
  - evidence: C:\dev\ref\ruffle\swf\src\write.rs:35-44 (write_swf), :48-86 (write_swf_raw_tags, signatures :49-53, length :69-70), :89-96 (zlib), :234-248 (private Writer); C:\dev\ref\ruffle\swf\src\lib.rs:30-35 (re-exports); C:\dev\ref\ruffle\swf\src\types.rs:55-61 (SwfBuf), :69-75 (Header), :237-241 (Compression); C:\dev\ref\ruffle\swf\src\read.rs:28-35 (parse_swf), :72 (decompress_swf)
- **F2** [confirmed] Header units. The stage `Rectangle<Twips>` (Twips = i32 newtype, 20 per pixel) is written as a variable-bit RECT; 550x400 px gives x_max=11000, y_max=8000. `frame_rate` is Fixed8 (8.8), written as a little-endian i16; `Fixed8::from_f32(24.0)` gives raw 0x1800, exactly what Flash CS6 writes. `num_frames` is u16 and must equal the root ShowFrame count. Pitfalls: `Header::default_with_swf_version` and `Rectangle::default()` both produce `Rectangle::INVALID` (0x7ffffff twips), so always set the stage explicitly. `Twips::from_pixels` and `Fixed8/Fixed16::from_f32/from_f64` truncate toward zero instead of rounding, so build values with `Twips::new((px*20.0).round() as i32)` and `Fixed16::from_bits((v*65536.0).round() as i32)`. Zlib on SWF<6 only logs a warning when read.
  - evidence: C:\dev\ref\ruffle\swf\src\write.rs:61-63, :259-261 (write_fixed8 = write_i16); C:\dev\ref\ruffle\swf\src\types\twips.rs:19-23, :65, :102-104 (from_pixels truncates); C:\dev\ref\ruffle\swf\src\types\fixed.rs:48, :63-75, :358-359 (Fixed8=i16 8 frac bits, Fixed16=i32 16 frac bits); C:\dev\ref\ruffle\swf\src\types\rectangle.rs:84-89, :183-187 (Default = INVALID); C:\dev\ref\ruffle\swf\src\types.rs:77-86; real file dump: C:\dev\ref\ruffle\swf\tests\swfs\DefineButton2-CS6.swf has ver=8, rect=[0,11000,0,8000], fps raw 0x1800 (dumped with C:\Users\A04E~1\AppData\Local\Temp\claude\C--Users-----------CluadeCowork---------------\b1f4a133-7216-4e29-a076-c5e0c76642d5\scratchpad\swfdump.py)
- **F3** [confirmed] Lifetimes: `Tag<'a>` borrows all raw bytes and strings. DoAction is `&'a [u8]`; so are DoInitAction.action_data, ButtonAction.action_data, ClipAction.action_data and DefineBitsJpeg2/3 data. Names and labels are `&'a SwfStr`. Exceptions: `DefineBitsLossless.data` is `Cow<'a,[u8]>` and can own its bytes. Shape, Text, Glyph, FontLayout, LineStyle and FillStyle are fully owned (no lifetime). Pattern: build every compiled AVM1 buffer, instance name and bitmap first, in a struct, `Vec<Vec<u8>>`/`Vec<String>`, or a bumpalo arena. Then build `Vec<Tag<'a>>` that borrows from them, then call write_swf. `impl From<&'a str> for &'a SwfStr` and `SwfStr::from_utf8_str` make `"name".into()` work. Strings are UTF-8 for SWF6+, so Arabic instance names, labels and variables are fine.
  - evidence: C:\dev\ref\ruffle\swf\src\types.rs:511-610 (Tag enum), :1793 (DoAction alias), :455-459 (ClipAction), :1112-1115 (ButtonAction), :1703-1710 (DefineBitsLossless Cow), :1771-1777 (DefineBitsJpeg3); C:\dev\ref\ruffle\swf\src\string.rs:15-20, :75-77, :141-147 (UTF-8 for v>=6), :245-249 (From<&str>)
- **F4** [confirmed] `write_tag_list` always appends `Tag::End`, both for the root and for every DefineSprite body. Do not put `Tag::End` in your tag vectors, or you get a double End. Rascal does this inside its class sprites; it is harmless but sloppy. `parse_swf`/`read_tag_list` stop at End and leave it out of `tags`, so a read-back gives `parsed.tags.len() == tags.len()`.
  - evidence: C:\dev\ref\ruffle\swf\src\write.rs:2370-2378, :1316-1327; C:\dev\ref\ruffle\swf\src\read.rs:731-741; C:\dev\ref\rascal\crates\rascal\src\swf.rs (pcode_to_swf pushes `tags: vec![Tag::End]` in each class sprite)
- **F5** [confirmed] FileAttributes must be the first tag. Ruffle honours it only in that position. SetBackgroundColor is put in HeaderExt only if it appears within the next two tags, i.e. order FileAttributes, [Metadata], SetBackgroundColor; Flash CS6 writes exactly this order. Type: `bitflags FileAttributes: u8 { USE_DIRECT_BLIT=1<<6, USE_GPU=1<<5, HAS_METADATA=1<<4, IS_ACTION_SCRIPT_3=1<<3, USE_NETWORK_SANDBOX=1<<0 }`, written as a u32. For AS2 never set IS_ACTION_SCRIPT_3: Ruffle then ignores every DoAction/DoInitAction. Set HAS_METADATA only when a Metadata tag follows. USE_NETWORK_SANDBOX decides LocalWithNetwork vs LocalWithFile for file:// URLs. The Flash 8 default (local files only) means leaving it unset.
  - evidence: C:\dev\ref\ruffle\swf\src\read.rs:147-170; C:\dev\ref\ruffle\swf\src\types.rs:264-282; C:\dev\ref\ruffle\swf\src\write.rs:894-897; C:\dev\ref\ruffle\core\src\display_object\movie_clip.rs:4296-4298 and :643-645 (AS3 movies skip AVM1 tags); C:\dev\ref\ruffle\core\common\src\sandbox.rs:43-62; dumps: C:\dev\ref\ruffle\tests\tests\swfs\from_shumway\timeline\timeline_as2_5\test.swf (FileAttributes 0x10, Metadata, SetBackgroundColor), C:\dev\ref\ruffle\swf\tests\swfs\DefineButton2-CS6.swf (FileAttributes 0, SetBackgroundColor)
- **F6** [partly-wrong] DefineShape1-4 use one struct: `Shape { version: u8 (1..=4 selects the tag code), id, shape_bounds, edge_bounds, flags: ShapeFlag, styles: ShapeStyles { fill_styles: Vec<FillStyle>, line_styles: Vec<LineStyle> }, shape: Vec<ShapeRecord> }`. v1/v2 write colors as RGB, dropping alpha; v3+ write RGBA. StyleChange.new_styles requires v2+ (the writer errors otherwise). edge_bounds and `ShapeFlag {HAS_SCALING_STROKES=1, HAS_NON_SCALING_STROKES=2, NON_ZERO_WINDING_RULE=4}` are serialized only for v4. LineStyle2 (caps, joins, miter, scale flags, fill) also needs v4; a non-Color line fill in v<4 is an error. The reader sets edge_bounds=shape_bounds and flags=HAS_NON_SCALING_STROKES for v<4. NON_ZERO_WINDING_RULE is a Flash Player 10 feature, so leave it unset for SWF8. Ruffle honours it anyway. Flash itself picks the lowest version that fits: DefineShape for opaque solid fills, DefineShape4 for focal gradients and LineStyle2.
  - evidence: C:\dev\ref\ruffle\swf\src\types.rs:643-660, :718-721; C:\dev\ref\ruffle\swf\src\write.rs:1268-1305, :1486-1491, :1504-1588; C:\dev\ref\ruffle\swf\src\read.rs:1484-1518 (v<4 defaults at :1487-1494); C:\dev\ref\ruffle\render\src\shape_utils.rs:420-424; dump of C:\dev\ref\ruffle\tests\tests\swfs\visual\gradient_issue_9892\test.swf shows alternating DefineShape/DefineShape4
  - CORRECTION: The struct, version gating, v<4 read defaults (read.rs:1487-1494) and NON_ZERO handling (render\src\shape_utils.rs:420-424) are correct. Wrong: 'Flash picks the lowest version that fits'. In C:\dev\ref\ruffle\swf\tests\swfs\DefineButton2-CS6.swf, CS6 writes DefineShape4 for an opaque solid fill plus a plain 1px stroke: ShapeFlag=1 (HAS_SCALING_STROKES), LineStyle2 flags=0x0, width 20 (my own decode). gradient_issue_9892 uses DefineShape only for stroke-less shapes. The observed rule is DefineShape4 whenever any stroke exists (SWF>=8). Also, write_shape_styles writes the 0xFF+u16 extended style count regardless of shape version (write.rs:1379-1393), although SWF19 allows it only for DefineShape2+.
- **F7** [confirmed] FillStyle: `Color(Color)`, `LinearGradient(Gradient)`, `RadialGradient(Gradient)`, `FocalGradient { gradient, focal_point: Fixed8 }` (-1..1), `Bitmap { id, matrix, is_smoothed, is_repeating }`. `Gradient { matrix: Matrix, spread: GradientSpread {Pad=0, Reflect=1, Repeat=2}, interpolation: GradientInterpolation {Rgb=0, LinearRgb=1}, records: Vec<GradientRecord { ratio: u8 (0..255), color }> }`. The gradient matrix maps the gradient square of 32768 twips (-16384..16384, i.e. 1638.4 px) into shape space. The writer masks the record count to 4 bits: more than 15 records silently corrupts the tag (the spec limit is 8 for DefineShape1-3, 15 for DefineShape4). A gradient with 0 records is read back as black. A bitmap fill matrix maps bitmap pixels to twips, so 1:1 is a=d=20.0. is_smoothed is only encoded for SWF>=8 (types 0x42/0x43 mean unsmoothed).
  - evidence: C:\dev\ref\ruffle\swf\src\types.rs:756-819; C:\dev\ref\ruffle\swf\src\write.rs:1534-1551, :1590-1610 (count & 0b1111 at :1607); C:\dev\ref\ruffle\swf\src\read.rs:1586-1640, :1693-1698; C:\dev\ref\ruffle\render\src\tessellator.rs:331-351 (gradient /32768), :356-383 (bitmap *20/width)
- **F8** [partly-wrong] LineStyle fields are `pub(crate)`, so it must be built with methods: `LineStyle::new().with_width(Twips).with_color(Color)` or `.with_fill_style(FillStyle)` (sets HAS_FILL; v4 only), `.with_start_cap(LineCapStyle)`, `.with_end_cap(LineCapStyle)` where LineCapStyle is {Round=0, None=1, Square=2}, `.with_join_style(LineJoinStyle::{Round, Bevel, Miter(Fixed8)})`, `.with_allow_scale_x(bool)` (NO_H_SCALE), `.with_allow_scale_y(bool)` (NO_V_SCALE), `.with_is_pixel_hinted(bool)`, `.with_allow_close(bool)` (NO_CLOSE). LineStyle2 is written as width u16 (twips, cast silently), flags u16, Fixed8 miter only for Miter, then a FillStyle if HAS_FILL, else RGBA. XFL mapping: caps go to both start and end caps; scaleMode none clears both allow flags; horizontal sets NO_V_SCALE; vertical sets NO_H_SCALE. DefineShape4's ShapeFlag HAS_SCALING_STROKES/HAS_NON_SCALING_STROKES should match the line styles used.
  - evidence: C:\dev\ref\ruffle\swf\src\types.rs:822-958 (builders), :976-993 (LineStyleFlag), :1004-1024; C:\dev\ref\ruffle\swf\src\write.rs:1556-1588; C:\dev\ref\ruffle\swf\src\read.rs:1642-1686
  - CORRECTION: Builder API, field privacy (types.rs:822-827), LineCapStyle {Round=0, None=1, Square=2} (:1004-1009), LineJoinStyle (:1019-1024) and the LineStyle2 wire format (write.rs:1556-1588) are correct. with_fill_style(FillStyle::Color) clears HAS_FILL (types.rs:928-933). The scaleMode mapping is unverified and probably reversed. It comes only from the community schema text, which says horizontal = 'Scale strokes only horizontally' (C:\dev\ref\xfl-schema\xfl.xsd:763-790). Adobe's Flash UI semantics (Horizontal = do not scale thickness when scaled horizontally) would give horizontal -> NO_H_SCALE. Also, the schema declares default="none" (xfl.xsd:763), while Flash's default is normal. Treat a missing scaleMode as normal, and confirm the direction with a CS6 publish before relying on it.
- **F9** [partly-wrong] ShapeRecord: `StyleChange(Box<StyleChangeData { move_to: Option<Point<Twips>>, fill_style_0: Option<u32>, fill_style_1: Option<u32>, line_style: Option<u32>, new_styles: Option<ShapeStyles> }>)`, `StraightEdge { delta: PointDelta<Twips> }`, `CurvedEdge { control_delta, anchor_delta }` (quadratic Bezier). move_to is absolute from the shape origin; edge deltas are relative to the pen; all values in twips. Only fields that are Some are written, and the writer sizes the bit fields itself. Pitfalls: (a) a StyleChange with every field None is written as flags=0, which is the EndShapeRecord, so the rest of the shape is silently cut off. (b) Edge NumBits is a 4-bit field of (bits-2), so a delta must fit in 17 signed bits (|d| <= 65535 twips, 3276.75 px). bitstream-io then returns InvalidInput 'excessive value for bits written', so split long edges. (c) Any number of edges may share one style state, and edge order does not matter to Ruffle, which links the edge soup per style.
  - evidence: C:\dev\ref\ruffle\swf\src\types.rs:724-753; C:\dev\ref\ruffle\swf\src\write.rs:1406-1502 (edge bits :1415-1419, :1437-1442; flags :1452-1470); C:\dev\ref\ruffle\swf\src\read.rs:1726-1818 (empty flags => End at :1808-1811); https://raw.githubusercontent.com/tuffy/bitstream-io/master/src/write.rs lines ~1879-1906; C:\dev\ref\ruffle\render\src\shape_utils.rs:296-327 (segment linking)
  - CORRECTION: (a) and (b) are verified: an empty-flags StyleChange equals EndShapeRecord (write.rs:1452-1470, read.rs:1763-1811); edge NumBits is 4 bits of (n-2) (write.rs:1415-1419, :1437-1442); count_sbits at :2385-2395 allows -65536..65535. (c) is only half right. Fill segments are endpoint-linked regardless of order (render\src\shape_utils.rs:303-326). Strokes are NOT linked: flush_stroke uses push_path (shape_utils.rs:367-371), so edge order and contiguity decide how stroke joins render.
- **F10** [confirmed] Fill sides. FillStyle1 is the fill on the right of the edge's direction in y-down space, i.e. inside a contour drawn clockwise on screen. FillStyle0 is on the left. Ruffle adds fill1 segments as they are and reverses fill0 segments before linking. The XFL schema defines fillStyle0 = left and fillStyle1 = right, so the mapping is 1:1 with no flipping. Style indices are 1-based into the current fill/line list; 0 means none; an index above the count is treated as 0. NewStyles replaces the lists and starts a new drawing layer, but Ruffle keeps the previously selected index numbers. So in the same record as new_styles, always also set fill_style_0, fill_style_1 and line_style (Some(0) when unused). Drop non-stroked edges with fillStyle0 == fillStyle1. Font glyphs are the same: real Flash DefineFont3 glyphs use fill_style_0 = Some(1), and Ruffle renders glyphs with NON_ZERO_WINDING_RULE. TrueType outer contours (clockwise in y-up) become counter-clockwise after flipping y, so they go into fill0 with no reordering.
  - evidence: C:\dev\ref\ruffle\render\src\shape_utils.rs:222-228 (flip comment), :356-364, :442-484 (1-based check at :458-464), :591 (fill1 clockwise, fill0 CCW), :1213-1240 (glyph shape NON_ZERO, fill index 1); C:\dev\ref\xfl-schema\xfl.xsd:2124-2132; C:\dev\ref\ruffle\swf\src\test_data.rs:~540-575 (CS5.5 DefineFont3 glyphs with fill_style_0: Some(1)); C:\dev\ref\ruffle\swf\src\write.rs:1399-1401 (bit count = count_ubits(len))
- **F11** [confirmed] Bounds matter. Ruffle stores shape_bounds and edge_bounds from the tag and returns shape_bounds as the bounds with strokes (used for _width/_height, hitTest bounding boxes and culling); for v<4, edge_bounds comes back equal to shape_bounds. Compute edge bounds from the records, including curve extrema; `ruffle_render::shape_utils::calculate_shape_bounds` does this. Then shape_bounds = edge_bounds.grow(max_line_width/2). `Rectangle::grow` and `union` exist in swf.
  - evidence: C:\dev\ref\ruffle\core\src\display_object\graphic.rs:50-60, :185-191; C:\dev\ref\ruffle\render\src\shape_utils.rs:64-110; C:\dev\ref\ruffle\swf\src\types\rectangle.rs:134, :158
- **F12** [confirmed] `PlaceObject<'a>` has 17 public fields and no Default: version (1-4), action: PlaceObjectAction::{Place(CharacterId), Modify, Replace(CharacterId)}, depth: u16, matrix: Option<Matrix>, color_transform: Option<ColorTransform>, ratio: Option<u16>, name: Option<&SwfStr>, clip_depth: Option<u16>, class_name, filters: Option<Vec<Filter>>, background_color, blend_mode: Option<BlendMode>, clip_actions: Option<ClipActions>, has_image: bool, is_bitmap_cached: Option<bool>, is_visible, amf_data. The action sets the flags: Place = HAS_CHARACTER; Modify = MOVE; Replace = MOVE|HAS_CHARACTER. filters, blend_mode and is_bitmap_cached are written only for version>=3 (PlaceObject3, SWF8). `Matrix { a,b,c,d: Fixed16, tx,ty: Twips }`, with helpers `Matrix::IDENTITY/translate/scale/rotate`. `parent * child` applies child first (useful for flattening graphics). `ColorTransform { r/g/b/a_multiply: Fixed8, r/g/b/a_add: i16 }` with `IDENTITY`; PlaceObject2/3 always write the RGBA form. `Filter` variants: DropShadowFilter, BlurFilter, GlowFilter, BevelFilter, GradientGlowFilter, ConvolutionFilter, ColorMatrixFilter, GradientBevelFilter (each boxed). `BlendMode` is {Normal=0, Layer=2, Multiply=3 ... HardLight=14}. Avoid PlaceObject version 1: write_color_transform_no_alpha uses a_add in r_add's slot when sizing the bit field.
  - evidence: C:\dev\ref\ruffle\swf\src\types.rs:314-392; C:\dev\ref\ruffle\swf\src\write.rs:1639-1770, :335-340 (bug), :428-454 (matrix), :373-426; C:\dev\ref\ruffle\swf\src\types\matrix.rs:19-82, :112-137; C:\dev\ref\ruffle\swf\src\types\color_transform.rs:5-26, :75; dump C:\dev\ref\ruffle\swf\tests\swfs\PlaceObject3-theworks.swf (Flash writes PlaceObject3 when filters/blend/cacheAsBitmap are used)
- **F13** [partly-wrong] Clip actions (onClipEvent and on() on MovieClip instances): `ClipActions { all_event_flags: ClipEventFlag, records: Vec<ClipAction { events, key_code: Option<u8>, action_data }> }`. all_event_flags must be the OR of the record events. key_code must be Some exactly when events contains KEY_PRESS, because the writer adds the key byte whenever key_code is Some while the reader expects it only for KEY_PRESS. For SWF>=6 the flags are written as u32 and the list ends with a u32 0. ClipEventFlag bits: LOAD, ENTER_FRAME, UNLOAD, MOUSE_MOVE, MOUSE_DOWN, MOUSE_UP, KEY_DOWN, KEY_UP, DATA, INITIALIZE, PRESS, RELEASE, RELEASE_OUTSIDE, ROLL_OVER, ROLL_OUT, DRAG_OVER, DRAG_OUT, KEY_PRESS, CONSTRUCT. In real Flash output every action blob (DoAction, button, clip) ends with ActionEnd 0x00. Ruffle applies name, clip_depth and clip_actions only at first placement, and only to MovieClips.
  - evidence: C:\dev\ref\ruffle\swf\src\types.rs:455-497; C:\dev\ref\ruffle\swf\src\write.rs:1886-1916; C:\dev\ref\ruffle\swf\src\read.rs:2056-2109 (:2075-2081 key code); C:\dev\ref\ruffle\swf\src\test_data.rs:1916-1960 (CS6 data, action_data ends with 0); C:\dev\ref\ruffle\core\src\display_object\movie_clip.rs:1536-1561; C:\dev\ref\ruffle\core\src\display_object.rs:2635-2638
  - CORRECTION: Struct, flags, key_code handling and u32 terminator are verified (types.rs:455-497, write.rs:1886-1916, read.rs:2069-2092). Wrong: the claim that name and clip_depth are applied 'only to MovieClips'. instantiate_child sets name and clip_depth on ANY child (movie_clip.rs:1536-1545); only clip_actions require as_movie_clip() (:1546-1549). All three are first-placement only (display_object.rs:2635-2638).
- **F14** [confirmed] DefineButton2: `Button { id, is_track_as_menu: bool, records: Vec<ButtonRecord { states: ButtonState {UP=1, OVER=2, DOWN=4, HIT_TEST=8}, id, depth, matrix, color_transform, filters: Vec<Filter>, blend_mode }>, actions: Vec<ButtonAction { conditions: ButtonActionCondition, action_data }> }`. The writer computes the action offset (0 when there are no actions) and the per-action length (0 for the last one). Ruffle maps on() events to conditions: rollOver=IDLE_TO_OVER_UP, rollOut=OVER_UP_TO_IDLE, press=OVER_UP_TO_OVER_DOWN, release=OVER_DOWN_TO_OVER_UP, releaseOutside=OUT_DOWN_TO_IDLE, dragOver=OUT_DOWN_TO_OVER_DOWN, dragOut=OVER_DOWN_TO_OUT_DOWN. keyPress uses `ButtonActionCondition::from_key_code(k)` (bits 9-15); Ruffle's ButtonKeyCode has Left=1, Right=2, Home=3, End=4, Insert=5, Delete=6, Backspace=8, Return=13, Up=14, Down=15, PgUp=16, PgDown=17, Tab=18, Escape=19, then ASCII from 32. Several on() blocks with the same body can be one action with conditions ORed together (`matches()` is a contains-test). Without a HIT_TEST record the button cannot be clicked. Do not use DefineButton v1: `write_define_button` indexes `actions[0]` and panics when there are no actions.
  - evidence: C:\dev\ref\ruffle\swf\src\types.rs:1066-1156; C:\dev\ref\ruffle\swf\src\write.rs:941-983, :924-939 (panic at :934), :1342-1370; C:\dev\ref\ruffle\swf\src\read.rs:776-804, :896-918; C:\dev\ref\ruffle\core\src\display_object\avm1_button.rs:451-489; C:\dev\ref\ruffle\core\src\events.rs:684-700; C:\dev\ref\ruffle\swf\src\test_data.rs:228-281 (CS6 button: OVER_DOWN_TO_OVER_UP and from_key_code(3))
- **F15** [confirmed] DoAction and DoInitAction. `Tag::DoAction(&[u8])`; `Tag::DoInitAction { id: CharacterId, action_data }`. Ruffle queues DoAction to run after the frame's display-list tags, so its position inside a frame does not change AVM1 semantics; Flash CS6 actually writes DoAction before the frame's PlaceObjects. Ruffle runs DoInitAction immediately at preload time; Flash runs it before the frame's actions. Flash publishes an AS2 class as an empty DefineSprite plus `ExportAssets` named "__Packages.<Class>" plus `DoInitAction { id }`; Rascal reproduces this pattern. Real Flash puts DoInitAction after the frame's PlaceObject2 and before ShowFrame.
  - evidence: C:\dev\ref\ruffle\core\src\display_object\movie_clip.rs:4291-4314 (queue), :559 and :635-670 (init action at preload), :1404-1406; C:\dev\ref\ruffle\swf\src\write.rs:710-718; C:\dev\ref\rascal\crates\rascal\src\swf.rs (DefineSprite + ExportAssets __Packages + DoInitAction loop); dumps: C:\dev\ref\ruffle\swf\tests\swfs\DoInitAction-CS6.swf (PlaceObject2 then DoInitAction then ShowFrame), C:\dev\ref\ruffle\tests\tests\swfs\from_shumway\timeline\timeline_as2_5\test.swf (DoAction first in each frame)
- **F16** [confirmed] DefineSprite: `Sprite { id, num_frames: u16, tags: Vec<Tag> }`. Ruffle uses the header num_frames for nextFrame, goto range checks and _totalframes, but plays and loops using the actual ShowFrame count, and loops only if the End tag is present (the writer adds it). So num_frames must equal the ShowFrame count, and the same holds for the root header. A sprite with no ShowFrame is treated as one frame. Keep every definition tag at root level: Flash never nests definitions inside DefineSprite, and Ruffle's library is per-movie.
  - evidence: C:\dev\ref\ruffle\swf\src\types.rs:711-715; C:\dev\ref\ruffle\core\src\display_object\movie_clip.rs:3947-3990 (define_sprite), :835-849, :1318-1330 (header_frames use), :1355-1373 (looping needs has_end_tag), :606-613; all dumps show definitions at root only
- **F17** [confirmed] Other tags: `Tag::RemoveObject(RemoveObject { depth, character_id: None })` writes RemoveObject2. `Tag::FrameLabel(FrameLabel { label: &SwfStr, is_anchor: bool })` writes the anchor byte only when version>=6. `Tag::ExportAssets(Vec<ExportedAsset { id, name }>)`. `Tag::Metadata(&SwfStr)` is null-terminated XMP. `Tag::SetBackgroundColor(Color)` is written as RGB. `ImportAssets` is written as ImportAssets2 for v>=8. `Tag::ShowFrame`. End is implicit.
  - evidence: C:\dev\ref\ruffle\swf\src\write.rs:463, :465, :746-766, :773-782, :807-815, :899-908, :1329-1340; C:\dev\ref\ruffle\swf\src\types.rs:293-296, :612-624
- **F18** [confirmed] Bitmaps. `DefineBitsLossless { version: 2, id, format: BitmapFormat::{ColorMap8 { num_colors: palette_len-1 }, Rgb15, Rgb32}, width, height, data: Cow }`. data must already be zlib-compressed; the writer copies it verbatim. For version 2 with Rgb32, the pixels are ARGB, premultiplied by alpha, with no row padding. ColorMap8 rows are padded to 4 bytes and the palette entries are RGBA. Rgb15 is not valid in version 2. `DefineBitsJpeg2 { id, jpeg_data }`. `DefineBitsJpeg3(DefineBitsJpeg3 { id, version: 3, deblocking: Fixed8::ZERO, data: jpeg, alpha_data: zlib(alpha plane) })`; Ruffle expects the JPEG colors premultiplied. Do not use version 4: the writer still emits the Jpeg3 tag code and a length that leaves out the deblocking field. Flash defines the bitmap immediately before the shape that fills with it.
  - evidence: C:\dev\ref\ruffle\swf\src\write.rs:500-546 (JPEG3 v4 issue :506-519); C:\dev\ref\ruffle\swf\src\types.rs:1703-1717, :1771-1777; C:\dev\ref\ruffle\render\src\utils.rs:235-238, :270-358; dump C:\dev\ref\ruffle\swf\tests\swfs\DefineBitsLossless2.swf
- **F19** [partly-wrong] DefineFont3 is `Tag::DefineFont2(Box<Font { version: 3, id, name: &SwfStr, language: Language, layout: Option<FontLayout { ascent: u16, descent: u16, leading: i16, kerning: Vec<KerningRecord { left_code, right_code, adjustment: Twips }> }>, glyphs: Vec<Glyph { shape_records, code: u16, advance: u16, bounds: Option<Rectangle<Twips>> }>, flags: FontFlag }>)`. The EM square is 1024*20 = 20480 units for DF3. Ruffle scales glyphs, ascent, descent, leading and advances by height/20480. PITFALL: the writer writes `font.flags` verbatim, but picks the code width from `!IS_ANSI`, the offset width from the actual size (offsets above 0xFFFF become 32-bit), and whether to write layout from `layout.is_some()`. The reader trusts the flags. So set HAS_WIDE_CODES (and not IS_ANSI), set HAS_LAYOUT iff layout is Some, and set HAS_WIDE_OFFSETS iff the glyph table exceeds 64 KB. That last case is likely for an Arabic subset at 20480 units: write, read back, retry. With layout, every glyph needs bounds (the writer errors otherwise). `DefineFontAlignZones { id, thickness: FontThickness, zones: Vec<FontAlignZone { left, width, bottom, height: i16 (raw f16 bits) }> }` and `DefineFontName { id, name, copyright_info }` are written correctly, but Ruffle core ignores both. Flash order: DefineFont3, DefineFontAlignZones, DefineFontName, DefineEditText/DefineText, CsmTextSettings.
  - evidence: C:\dev\ref\ruffle\swf\src\types.rs:1191-1245, :1659-1678; C:\dev\ref\ruffle\swf\src\write.rs:1998-2117 (flags :2042, codes :2010, wide offsets :2026-2028, bounds error :2095-2099), :643-673; C:\dev\ref\ruffle\swf\src\read.rs:997-1127; C:\dev\ref\ruffle\core\src\font.rs:345-355, :393-395 (scale 20480 for v>=3), :280-298; grep shows no DefineFontAlignZones/DefineFontName handling in C:\dev\ref\ruffle\core\src; dump C:\dev\ref\ruffle\swf\tests\swfs\DefineFont3-CS6.swf
  - CORRECTION: Types (types.rs:1191-1245), flags written verbatim (write.rs:2042), codes taken from !IS_ANSI (:2010), the bounds error (:2095-2099) and the Ruffle 20480 scale (core\src\font.rs:393-395) are all correct. The wide-offset rule is stated wrongly. The writer switches to 32-bit offsets only when some glyph's START offset inside the raw shape table exceeds 0xFFFF (write.rs:2025-2028). It never checks offset+init_offset or code_table_offset = (n+1)*2 + shape_buf.len(); in 16-bit mode both are cast with `as u16` (write.rs:2058-2071). A table near 64 KB therefore corrupts silently even when the flags match. The caller cannot force wide mode; it must split the font or pad so a glyph starts past 0xFFFF. Also, glyphs are written with a fixed context: shape_version 1, num_fill_bits 1, num_line_bits 0 (write.rs:2015-2020). Glyph records may use only fill_style_0/1 in {0,1}, no line_style above 0, and no new_styles (an error for shape_version<2, write.rs:1484-1491).
- **F20** [confirmed] DefineText/DefineText2: `Text { id, bounds, matrix, records: Vec<TextRecord { font_id: Option<u16>, color: Option<Color>, x_offset: Option<Twips>, y_offset: Option<Twips>, height: Option<Twips>, glyphs: Vec<GlyphEntry { index: u32, advance: i32 }> }> }`. DefineText writes RGB colors, DefineText2 RGBA. font_id and height share one flag, so both must be Some or both None. x_offset and y_offset are absolute pen positions (y is the baseline), i16 twips, and carry over between records, as do color, font and height. Advances are signed twips, and the bit count is computed with count_sbits, so negative advances work. At most 255 glyphs per record: the count is written as u8, so longer runs must be split. For Arabic: shape with HarfBuzz/rustybuzz, embed the shaped glyph IDs as DF3 glyphs (codes from U+FE70..FEFF presentation forms, or the PUA for ligatures), and emit records in visual left-to-right order. Place marks with a new record carrying x/y offsets, or with negative advances. Ruffle renders glyphs by index, so no code lookup is involved for static text.
  - evidence: C:\dev\ref\ruffle\swf\src\types.rs:1276-1297; C:\dev\ref\ruffle\swf\src\write.rs:2202-2266 (u8 count :2250, i16 :2242-2245, sbits :2215-2220); C:\dev\ref\ruffle\swf\src\read.rs:2319-2380; C:\dev\ref\ruffle\core\src\display_object\text.rs:157-190
- **F21** [confirmed] DefineEditText fields are pub(crate), so build it with methods: `EditText::new().with_id().with_bounds(Rectangle)` (bounds MUST be set: the default is INVALID), then `.with_font_id(id, height_twips)` or `.with_font_class()`, `.with_color(Option<Color>)` (RGBA), `.with_max_length()`, `.with_layout(Option<TextLayout { align: TextAlign {Left, Right, Center, Justify}, left_margin, right_margin, indent, leading: Twips }>)`, `.with_variable_name(&SwfStr)`, `.with_initial_text(Option<&SwfStr>)`, `.with_is_html()`, `.with_is_multiline()`, `.with_is_word_wrap()`, `.with_is_password()`, `.with_is_read_only()`, `.with_is_selectable()`, `.with_has_border()`, `.with_use_outlines()` (embedded font), `.with_was_static()`, `.with_is_auto_size()`. Flash CS6 publishes device-font text, even static text, as DefineEditText that references a zero-glyph DefineFont3. Ruffle has no bidi or shaping crate, so Arabic in dynamic/input fields will display unshaped and in left-to-right order. Only pre-shaped static text (F20) is reliable.
  - evidence: C:\dev\ref\ruffle\swf\src\types.rs:1300-1650; C:\dev\ref\ruffle\swf\src\write.rs:2268-2315; dump of C:\dev\ref\ruffle\tests\tests\swfs\avm1\default_names\test.swf frame 2 (DefineFont3 len=13, DefineEditText for a DOMStaticText with fontRenderingMode=device); no bidi/rustybuzz/harfbuzz in C:\dev\ref\ruffle\core\Cargo.toml
- **F22** [confirmed] Classic motion tweens are baked per frame. Source: CS6 AS2 file, symbol with an 8-frame motion tween. Frame 1 is `PlaceObject2 Place(id) depth 1 + matrix`; every following frame, including the end keyframe that holds the same symbol, is `PlaceObject2 Modify depth 1` with only the changed matrix and/or color transform. The values are interpolated and rounded to the nearest twip (-220 to -4239 in steps of -502.375 gives -722, -1225, ...). No ratio is written for tweens. A keyframe with different content starts with RemoveObject2 for the old depths, then new Place tags. Flash writes `ratio` on objects placed in later keyframes (ratio = 0-based frame index), and Ruffle compares ratio, id and clip_depth to decide whether an object survives a goto/rewind. Ruffle ignores the matrix and color transform from PlaceObject once a script has moved the clip, as Flash does. The authoring tool must therefore compute easing and rotation interpolation itself and emit one Modify per frame per tweened depth (or per flattened element, see F23).
  - evidence: C:\dev\ref\ruffle\tests\tests\swfs\from_shumway\timeline\timeline_as2_5\test.fla (LIBRARY/Symbol 1.xml: DOMFrame index=0 duration=8 tweenType=motion) and test.swf dump (sprite 3: Place tx=-220, then Modify tx=-722, -1225, ..., -4239); C:\dev\ref\ruffle\tests\tests\swfs\avm1\default_names\test.swf (RemoveObject2 at frame start; ratio=1/2 on later placements); C:\dev\ref\ruffle\core\src\display_object\movie_clip.rs:4481-4512, :1908-1953 (survives_rewind uses ratio), :4840-4900; C:\dev\ref\ruffle\core\src\display_object.rs:2591-2640 (transformed_by_script)
- **F23** [confirmed] Flash does not publish graphic symbols as sprites. It flattens them into the parent timeline: the graphic's content tags are placed directly in the parent at their own depths, with matrices composed (instance matrix * inner matrix). Evidence: a CS6 AS2 file whose frame 2 holds `DOMSymbolInstance symbolType="graphic" loop="loop"` of a one-shape graphic at (5.45, 50) published as `DefineShape id=3` plus `PlaceObject2 depth 1 tx=109 ty=1000`, with no DefineSprite. Six single-frame graphic symbols in another CS6 file became plain DefineShape placements. For multi-frame graphics, by inference: each parent frame emits Place/Modify/Remove for the inner elements at the inner frame chosen by loop mode. loop: (firstFrame + t) mod len. play once: min(firstFrame + t, len-1). single frame: firstFrame. Here t counts frames since the instance's keyframe. Scripts inside graphics are dropped. Ruffle has no notion of graphic symbols; it plays whatever tags it is given. Publishing graphics as sprites would also render, but loses timeline sync and goto semantics.
  - evidence: C:\dev\ref\ruffle\tests\tests\swfs\avm1\default_names\test.fla (DOMDocument.xml DOMFrame index=1; LIBRARY/Symbol 1.xml symbolType=graphic) and test.swf dump; C:\dev\ref\ruffle\tests\tests\swfs\visual\gradient_issue_9892\test.fla + test.swf (6 graphic symbols, 0 DefineSprite); no multi-frame graphic sample exists among the local test FLAs (scanned with scratchpad graphics.py)
- **F24** [partly-wrong] Validation the writer must satisfy, from Ruffle's reader and player. (1) Header version must not be 0, and the declared length must be >= 8; a length mismatch only warns. (2) Character IDs must be unique: a duplicate is rejected and the first definition wins. (3) Define before use: placing an unknown ID logs 'Unable to instantiate display node id' and places nothing; ExportAssets naming an undefined ID is skipped. (4) Place on an occupied depth fails ('Failed to place object at depth'); Modify or Replace on an empty depth is a silent no-op. (5) A PlaceObject2 with neither MOVE nor HAS_CHARACTER is a parse error. (6) A KEY_PRESS clip action without a key code is a parse error. (7) Out-of-range style indices are treated as no fill. (8) LineStyle2 join/cap value 3 is warned about and cleared. (9) Under the AS3 flag, AVM1 tags are ignored. (10) num_frames vs ShowFrame count, and End needed for looping (F16). (11) DefineFont flag consistency (F19). (12) Text font_id/height pairing (F20).
  - evidence: C:\dev\ref\ruffle\swf\src\read.rs:88-96, :128-131, :1939, :2079, :1653-1666; C:\dev\ref\ruffle\core\src\library.rs:120-135, :139-147; C:\dev\ref\ruffle\core\src\display_object\movie_clip.rs:1503-1512, :1583-1587, :4093-4097, :4497-4508; C:\dev\ref\ruffle\render\src\shape_utils.rs:458-484
  - CORRECTION: Items 1-5 and 7-12 are verified: read.rs:88-96 and :131-133; library.rs:120-147; movie_clip.rs:1510-1513 and :1583-1587; Modify/Replace no-op at :4497-4508; read.rs:1929-1939; LineStyle2 value-3 clearing at read.rs:1653-1666. Item 6 is imprecise. A KEY_PRESS record without a key byte is a parse error only when its action length is 0 (checked_sub at read.rs:2076-2079). Otherwise the reader silently takes the first action byte as the key code and misparses the bytecode.
- **F25** [confirmed] Dependency facts. ruffle_core re-exports the crate (`pub use swf;` at lib.rs:68) and depends on it by path, so with a git dependency on Ruffle the swf package's source is the same git rev. crates.io swf 0.3.0 was published 2026-09-06 from Ruffle commit 7436b8448 ('swf: Release 0.3.0'). The pinned rev has only two later swf commits, both inside avm1/write.rs (GotoFrame2 flags fix; error instead of truncating oversized AVM1 lengths), so the API is identical. Rascal 0.3.6 depends on crates.io `swf = "0.3.0"` and returns `swf::error::Result<Vec<u8>>` from `CompiledProgram::to_swf`. Without a patch the build would contain two incompatible copies of swf. swf's dependencies are pure Rust (bitflags, bitstream-io, encoding_rs, flate2 with the miniz_oxide default backend, lzma-rs, enum-map, simple_asn1), so there is no C zlib and no runtime dependency, which fits the portable, no-install requirement. The crate uses edition 2024 and let-chains, which need Rust >= 1.88.
  - evidence: C:\dev\ref\ruffle\core\src\lib.rs:68-69; C:\dev\ref\ruffle\core\Cargo.toml:29, :82; C:\dev\ref\ruffle\swf\Cargo.toml (version 0.3.0, default = [flate2, lzma]); git log -- swf/ at C:\dev\ref\ruffle: 7436b8448 (2026-09-06 Release 0.3.0), 5113031b1, bfb540427 (both avm1/write.rs only); https://crates.io/api/v1/crates/swf (max_version 0.3.0, 2026-09-06); C:\dev\ref\rascal\crates\rascal\Cargo.toml (swf = "0.3.0"); C:\dev\ref\rascal\crates\rascal\src\program.rs:148-150; C:\dev\ref\ruffle\swf\src\write.rs:1698-1699 (let-chain); C:\dev\ref\ruffle\Cargo.toml:47, :79
- **F26** [confirmed] Silent truncations in the writer that the publisher must guard against itself: line width as u16 twips; TextRecord glyph count as u8; text x/y offsets as i16; edit-text margins and indent as u16, leading as i16; gradient record count masked to 4 bits; DefineButton2 action lengths and offsets as u16. Edge deltas and bit-field overflows return an io error instead (bitstream-io range check). Recommendation: a validation pass that splits or clamps these values before calling write_swf, plus a mandatory read-back (decompress_swf + parse_swf) after every publish.
  - evidence: C:\dev\ref\ruffle\swf\src\write.rs:1558, :2250, :2242-2248, :2298-2302, :1607, :962, :970

## Risks

- Without [patch.crates-io], Rascal pulls in a second crates.io copy of swf 0.3.0. Its types and errors would be incompatible with ruffle_core's git copy, and compile time doubles.
- Silent truncation in the writer (line width u16, TextRecord glyph count u8, text offsets i16, gradient count 4 bits, button action offsets u16) produces corrupt SWFs that still parse partly. A validation pass plus a read-back after every publish is needed.
- An all-None StyleChange becomes an EndShapeRecord and truncates the shape without any error. This is easy to hit when converting XFL edges that change nothing.
- DefineFont3 flags are written verbatim and not derived from the data. HAS_WIDE_OFFSETS is needed once the glyph table exceeds 64 KB, which is likely for Arabic subsets at the 20480 EM scale. A wrong flag breaks the font silently.
- Ruffle has no bidi or shaping. Arabic in DefineEditText (dynamic/input text, device fonts) will render unshaped and in left-to-right order. Only pre-shaped static DefineText is correct in the preview, and Flash Player behaviour for embedded-font edit text is the same.
- Multi-frame graphic symbol flattening (loop / play once / firstFrame) is inferred from single-frame evidence only. Depth allocation for nested flattened graphics has to be designed; the depths Flash uses have gaps (1, 3, 5, 9) and the exact scheme is unknown.
- The evidence for how Flash structures files comes from Flash CS6 output (SWF 5-15), not Flash 8 itself; minor tag-order differences are possible.
- Ruffle executes DoInitAction at preload time, not at the frame where the tag sits. Init-order bugs may differ between the preview and real Flash Player 8.
- The PlaceObject1 / DefineButtonCxform colour-transform writer has a bug (a_add used for r_add when sizing bits), and DefineBitsJpeg3 version 4 is written inconsistently. Avoid these paths.
- Twips::from_pixels and Fixed*::from_f* truncate; using them directly accumulates off-by-one-twip drift in tweens compared with Flash, which rounds to nearest.
- Ruffle git rev and crate pinning: a later `cargo update` cannot move a git rev, but [patch] must be kept in sync whenever the Ruffle rev changes.

## Open questions

- Does Flash 8 flatten MULTI-frame graphic symbols exactly as inferred (per-frame Place/Modify of inner elements, firstFrame/loop arithmetic), and how does it allocate depths for them? This needs a JPEXS dump of a real Flash 8 publish; no local sample exists.
- What is the exact Flash rule for writing `ratio` on non-morph PlaceObject2 (observed ratio = 0-based keyframe index on objects placed after frame 1, but not on plain shapes)? Should we emulate it so goto/rewind behaves like Flash in Ruffle's survives_rewind?
- Does Flash Player 8 require DefineFont3 code tables sorted ascending (SWF19 says so)? Ruffle uses a hash map and does not care.
- For on(dragOver)/on(dragOut), does Flash set extra condition bits (IDLE_TO_OVER_DOWN / OVER_DOWN_TO_IDLE, used with trackAsMenu)? Ruffle fires only OUT_DOWN_TO_OVER_DOWN / OVER_DOWN_TO_OUT_DOWN.
- Should M1 emit DefineFontAlignZones (raw half-float i16 values, ignored by Ruffle) for Flash Player advanced anti-aliasing, or skip it?
- XFL edge coordinate units and the bitmap-fill matrix convention (believed to be twips, with a=20 for 1:1 bitmaps) should be confirmed by the XFL-parsing research before the shape converter is written.
- Rascal integration: is extracting DoAction/DoInitAction bytes from Rascal's whole-SWF output acceptable, or should Rascal be extended upstream to return per-script pcode bytes? This is the Rascal topic's call.

## Verifier: problems in the code sketch

- I found nothing that would fail to compile against the pinned source. Checks: every struct literal has the real field names and counts (PlaceObject 17 fields types.rs:314-332, ButtonRecord types.rs:1074-1082, Header types.rs:69-75); constructors exist (Point::new and PointDelta::new at types\point.rs:50/:167, Neg for Twips at twips.rs:228, Color::from_rgb(u32,u8) at color.rs:61, Matrix::scale/translate as const fns); &Vec<u8> coerces to &[u8] at the tuple-variant and field coercion sites; decompress_swf(&out[..]) and parse_swf(&buf) match read.rs:72 and :28.
- The comment 'Modify (MOVE flag only)' is inaccurate. The frame-2 tag is written with MOVE|HAS_MATRIX|HAS_COLOR_TRANSFORM (write.rs:1649-1666). 'Modify' means only that HAS_CHARACTER is absent.
- po() hard-codes version: 2. If the real publisher later sets filters, blend_mode or is_bitmap_cached on such a PlaceObject, the fields are dropped silently: the flags and data are written only for version>=3 (write.rs:1676-1690, :1718-1738). The helper should bump to 3 automatically when any of them is Some.
- rect_shape uses DefineShape3 for a stroked shape. That is valid and renders in Ruffle, but it does not match CS6, which writes DefineShape4 with ShapeFlag HAS_SCALING_STROKES and LineStyle2 for any stroked shape (DefineButton2-CS6.swf). A v3 read-back also synthesizes flags=HAS_NON_SCALING_STROKES (read.rs:1493), so round-trip comparisons of shapes differ.
- The read-back check only compares tag counts (parsed.tags.len() == tags.len()). It would not catch silent truncations such as gradient counts, u16/u8 casts or font offsets (F26). Tag implements PartialEq, so compare parsed tags structurally for every type except v<4 shapes, or re-encode and compare bytes.
- The two ButtonActions share one body. Ruffle's matches() is a contains-test plus a key-code compare (types.rs:1143-1152), so a single action with conditions OVER_DOWN_TO_OVER_UP | from_key_code(13) is equivalent, smaller, and closer to how Flash merges identical on() handlers. This is not a bug.
- The Cargo section relies on Cargo unifying three references to the same git source (workspace dep, [patch.crates-io], and ruffle_core's internal path dep). That holds only if the git URL strings are identical, or canonicalize identically; otherwise two copies of swf result. ruffle_core's swf is a path dep inside the Ruffle git checkout, so the publisher's swf must use the exact same git URL+rev to unify with ruffle_core::swf.
- ExportAssets("Box") is unnecessary for a sprite placed on the timeline. It is harmless, but for AS2 classes the real pattern is __Packages.<Class> plus DoInitAction (Rascal swf.rs:58-73). Remember to remap Rascal's sprite ids 1..n, which collide with the ids hard-coded in the sketch (SHAPE=1, BOX_MC=2).

## Verifier: missed points

- Z-order inside one DefineShape: Ruffle draws each layer's fills in fill-style index order, not record order, then all strokes of that layer on top (render\src\shape_utils.rs:537-575); a new_styles record starts a new layer (:441-452). So overlapping XFL content in one frame cannot be emitted as one style layer if stacking matters. That includes several DOMShape elements, drawing objects, and strokes that must sit under later fills. Use separate DefineShapes at separate depths or new_styles layers in back-to-front order.
- Timeline depths must stay below 16384. Ruffle treats objects at depth >= AVM_DEPTH_BIAS (16384, core\src\avm1\globals.rs:793) as script-depth objects in survives_rewind (movie_clip.rs:1917-1921), so they would not be removed or reset on goto or loop. The XFL layer-to-depth allocation, including flattened graphics, needs a scheme that stays inside 1..16383.
- Header form: Flash writes long RECORDHEADERs for definition tags even when they are short. In DefineBitsLossless2.swf, DefineBitsLossless2 (22 bytes), DefineShape (32 bytes) and the 86/82/76 tags all use the long form (my own header scan). The swf writer always uses the short form below 63 bytes (write.rs:2359-2361). Ruffle accepts either, but compatibility with real Flash Player 8 for short-header DefineBits*/DefineBitsLossless* tags is unverified. If external-player compatibility matters, post-process the stream or build tags with write_swf_raw_tags.
- The writer does no version gating. The publisher must restrict itself to the SWF8 tag and field set: no PlaceObject3 class_name/has_image/is_visible/background_color, no SymbolClass, DoAbc, DefineFont4 or PlaceObject4, and no FileAttributes IS_ACTION_SCRIPT_3. The writer emits all of these without complaint (many TODO version asserts, e.g. write.rs:898, :1908), and Ruffle honours some of them only for SWF>=11 (display_object.rs:2617).
- Instance properties cannot change through Modify. name, clip_depth and clip_actions are applied only at first placement (display_object.rs:2635-2638), and once a script has moved a clip every later PlaceObject is ignored entirely (display_object.rs:2593). When the instance name, mask depth or clip events change between XFL keyframes on the same layer, the publisher must emit RemoveObject2 plus a new Place (with a new ratio, as Flash does), not a Modify.
- Static text color default: Ruffle starts DefineText rendering with color a=0 (core\src\display_object\text.rs:150-155), and font_id and height share one flag (write.rs:2226-2249). Every text block must open with a record that sets font_id, height and color. For Arabic glyph runs, each record also needs an explicit x_offset/y_offset because advances accumulate from the last offset (text.rs:160-188).
- Arabic and keyboard limits: on(keyPress) key codes are 7-bit (types.rs:1135, KEY_PRESS = 0b1111111 << 9), so Arabic characters cannot be button keyPress triggers. Clip-event KEY_PRESS uses one u8 (types.rs:458). The editor should reject or warn on such handlers instead of truncating.
- The swf API's own helpers truncate as well: Matrix::rotate (matrix.rs:64-71), ColorTransform::multiply_from and set_mult_color (color_transform.rs:28-36, :60-65), and the Matrix * Matrix fixed-point product (matrix.rs:112-136). Do tween interpolation and graphic-symbol matrix composition in f64, then round once into Fixed16/Twips. Chaining swf::Matrix multiplications through nested graphics accumulates error.

## Code sketch (researcher; check against corrections above)
```rust
// ---------------- Cargo.toml (workspace root) ----------------
// [workspace.dependencies]
// swf         = { git = "https://github.com/ruffle-rs/ruffle", rev = "07d3e430631333f80f754df01a968bdcd80d98c5" }
// ruffle_core = { git = "https://github.com/ruffle-rs/ruffle", rev = "07d3e430631333f80f754df01a968bdcd80d98c5" }
// rascal      = "0.3.6"
// flate2      = "1.1"      # zlib for DefineBitsLossless payloads (pure-Rust backend)
//
// # Make Rascal's crates.io `swf = "0.3.0"` resolve to the SAME package as ruffle_core's swf
// [patch.crates-io]
// swf = { git = "https://github.com/ruffle-rs/ruffle", rev = "07d3e430631333f80f754df01a968bdcd80d98c5" }
//
// The publisher crate depends on `swf` directly (no ruffle_core needed there);
// the preview crate can equally use `ruffle_core::swf`. Cargo unifies them because the source is identical.

use swf::*;

/// px -> twips, ROUNDED (Twips::from_pixels truncates toward zero).
fn tw(px: f64) -> Twips { Twips::new((px * 20.0).round() as i32) }
fn fx16(v: f64) -> Fixed16 { Fixed16::from_bits((v * 65536.0).round() as i32) }
fn fx8(v: f32) -> Fixed8 { Fixed8::from_bits((v * 256.0).round() as i16) }

/// PlaceObject has 17 fields and no Default.
fn po<'a>(depth: Depth, action: PlaceObjectAction) -> PlaceObject<'a> {
    PlaceObject {
        version: 2, action, depth,
        matrix: None, color_transform: None, ratio: None, name: None, clip_depth: None,
        class_name: None, filters: None, background_color: None, blend_mode: None,
        clip_actions: None, has_image: false, is_bitmap_cached: None, is_visible: None,
        amf_data: None,
    }
}

/// Rectangle centred on the origin: fill style 1 + line style 1 (1-based indices, 0 = none).
fn rect_shape(id: CharacterId, w: f64, h: f64, fill: Color, stroke: Color, stroke_px: f64) -> Shape {
    let (x0, y0, wt, ht) = (tw(-w / 2.0), tw(-h / 2.0), tw(w), tw(h));
    let edges = Rectangle { x_min: x0, y_min: y0, x_max: x0 + wt, y_max: y0 + ht };
    let half_stroke = Twips::new((tw(stroke_px).get() + 1) / 2);
    Shape {
        version: 3,                          // RGBA colours; use 4 for LineStyle2/caps/joins/focal
        id,
        shape_bounds: edges.grow(half_stroke), // Ruffle uses this for bounds/hit tests
        edge_bounds: edges,                  // only serialised when version == 4
        flags: ShapeFlag::empty(),           // NON_ZERO_WINDING_RULE is FP10+, leave off for SWF8
        styles: ShapeStyles {
            fill_styles: vec![FillStyle::Color(fill)],
            line_styles: vec![LineStyle::new().with_width(tw(stroke_px)).with_color(stroke)],
        },
        shape: vec![
            ShapeRecord::StyleChange(Box::new(StyleChangeData {
                move_to: Some(Point::new(x0, y0)), // absolute
                fill_style_0: None,
                fill_style_1: Some(1), // clockwise on screen (y down) => interior on the RIGHT => fill 1
                line_style: Some(1),
                new_styles: None,      // NEVER emit a StyleChange with all fields None (= EndShape)
            })),
            ShapeRecord::StraightEdge { delta: PointDelta::new(wt, Twips::ZERO) },  // relative deltas
            ShapeRecord::StraightEdge { delta: PointDelta::new(Twips::ZERO, ht) },
            ShapeRecord::StraightEdge { delta: PointDelta::new(-wt, Twips::ZERO) },
            ShapeRecord::StraightEdge { delta: PointDelta::new(Twips::ZERO, -ht) },
        ],
    }
}

pub fn publish_demo() -> swf::error::Result<Vec<u8>> {
    // ---- 1. OWNED data first: every Tag<'a> borrows from these ----
    // In the real tool these come from Rascal (extract DoAction/DoInitAction bytes from its
    // to_swf() output and REMAP its class-sprite ids). Each block ends with ActionEnd 0x00.
    let root_f1: Vec<u8>   = vec![0x96, 4, 0, 0, b'h', b'i', 0, 0x26, 0x00]; // trace("hi");
    let sprite_f1: Vec<u8> = vec![0x07, 0x00];                               // stop();
    let on_release: Vec<u8> = vec![0x81, 2, 0, 0, 0, 0x06, 0x00];           // GotoFrame(0); Play  (= gotoAndPlay(1))
    let on_enter_frame: Vec<u8> = vec![0x00];                                // empty onClipEvent(enterFrame)
    let instance_name = String::from("box_mc");                             // UTF-8 (SWF6+): Arabic is OK

    const SHAPE: CharacterId = 1;
    const BOX_MC: CharacterId = 2;
    const BTN: CharacterId = 3;

    // ---- 2. Sprite (MovieClip) with a frame script ----
    let sprite = Sprite {
        id: BOX_MC,
        num_frames: 1, // MUST equal the ShowFrame count below
        tags: vec![
            Tag::DoAction(&sprite_f1), // Flash puts DoAction first in the frame; Ruffle queues it anyway
            Tag::PlaceObject(Box::new(PlaceObject { matrix: Some(Matrix::IDENTITY), ..po(1, PlaceObjectAction::Place(SHAPE)) })),
            Tag::ShowFrame,
            // no Tag::End: write_tag_list appends it
        ],
    };

    // ---- 3. Button with on(release) and on(keyPress "<Enter>") ----
    let button = Button {
        id: BTN,
        is_track_as_menu: false,
        records: vec![
            ButtonRecord {
                states: ButtonState::UP | ButtonState::OVER | ButtonState::DOWN | ButtonState::HIT_TEST,
                id: SHAPE, depth: 1, matrix: Matrix::IDENTITY,
                color_transform: ColorTransform::IDENTITY, filters: vec![], blend_mode: BlendMode::Normal,
            },
            ButtonRecord {
                states: ButtonState::OVER | ButtonState::DOWN,
                id: SHAPE, depth: 2, matrix: Matrix::scale(fx16(0.9), fx16(0.9)),
                color_transform: ColorTransform { r_add: 60, ..ColorTransform::IDENTITY },
                filters: vec![], blend_mode: BlendMode::Normal,
            },
        ],
        actions: vec![
            ButtonAction { conditions: ButtonActionCondition::OVER_DOWN_TO_OVER_UP, action_data: &on_release },
            ButtonAction { conditions: ButtonActionCondition::from_key_code(13), action_data: &on_release },
        ],
    };

    // ---- 4. Root timeline: 2 frames, motion tween = Place then Modify ----
    let tags: Vec<Tag<'_>> = vec![
        Tag::FileAttributes(FileAttributes::empty()), // MUST be first; AS2 => no IS_ACTION_SCRIPT_3
        Tag::SetBackgroundColor(Color::WHITE),        // within the first 3 tags
        // frame 1: define just before first use (Flash does the same)
        Tag::DefineShape(Box::new(rect_shape(SHAPE, 100.0, 60.0, Color::from_rgb(0x3366CC, 255), Color::BLACK, 2.0))),
        Tag::DefineSprite(sprite),
        Tag::ExportAssets(vec![ExportedAsset { id: BOX_MC, name: "Box".into() }]), // after the definition
        Tag::DefineButton2(Box::new(button)),
        Tag::FrameLabel(FrameLabel { label: "start".into(), is_anchor: false }),
        Tag::DoAction(&root_f1),
        Tag::PlaceObject(Box::new(PlaceObject {
            matrix: Some(Matrix::translate(tw(100.0), tw(200.0))),
            name: Some(SwfStr::from_utf8_str(&instance_name)),
            clip_actions: Some(ClipActions {
                all_event_flags: ClipEventFlag::ENTER_FRAME, // OR of all records
                records: vec![ClipAction { events: ClipEventFlag::ENTER_FRAME, key_code: None, action_data: &on_enter_frame }],
            }),
            ..po(1, PlaceObjectAction::Place(BOX_MC))
        })),
        Tag::PlaceObject(Box::new(PlaceObject {
            matrix: Some(Matrix::translate(tw(450.0), tw(350.0))),
            ..po(3, PlaceObjectAction::Place(BTN))
        })),
        Tag::ShowFrame,
        // frame 2: one tween step = Modify (MOVE flag only) with the interpolated matrix + cxform
        Tag::PlaceObject(Box::new(PlaceObject {
            matrix: Some(Matrix { a: fx16(1.5), d: fx16(1.5), ..Matrix::translate(tw(300.0), tw(200.0)) }),
            color_transform: Some(ColorTransform { a_multiply: fx8(0.5), ..ColorTransform::IDENTITY }),
            ..po(1, PlaceObjectAction::Modify)
        })),
        Tag::ShowFrame,
    ];

    let header = Header {
        compression: Compression::Zlib, // "CWS"
        version: 8,
        stage_size: Rectangle { x_min: Twips::ZERO, x_max: tw(550.0), y_min: Twips::ZERO, y_max: tw(400.0) },
        frame_rate: Fixed8::from_f32(24.0), // 8.8 => 0x1800
        num_frames: 2,                       // MUST equal root ShowFrame count
    };

    let mut out = Vec::new();
    write_swf(&header, &tags, &mut out)?;

    // ---- 5. Mandatory self-check with the same crate ----
    let buf = swf::decompress_swf(&out[..])?;
    let parsed = swf::parse_swf(&buf)?; // stops at End, End not included
    assert_eq!(parsed.tags.len(), tags.len());
    // (Shape round-trips are not byte-equal for v<4: edge_bounds/flags are synthesised on read.)
    Ok(out)
}

// Graphic-symbol flattening helper (Flash behaviour, F23):
//   world_matrix = graphic_instance.matrix * inner_element.matrix   (swf::Matrix: lhs * rhs = rhs first)
//   world_cx     = graphic_instance.cxform * inner_element.cxform
//   inner_frame  = match loop_mode { Loop => (first + t) % len, PlayOnce => (first + t).min(len - 1), SingleFrame => first }
```
