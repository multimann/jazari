# Al-Jazari Flash 8 fixtures: MANIFEST

> **Where the files are now.** Each fixture lives in this repository as `tests/fixtures/NAME/`:
> `NAME.fla`, `flash8.swf` (the published `NAME.swf`), `flash8-trace.txt` (the Test Movie output
> captured as `tracecap\NAME\trace.txt`) and `xfl/`. Paths below that start with `C:\dev\samples` or
> `C:\dev\oracle` name the places where the fixtures were built. `smoke/` is the first hand-made
> fixture: a red box on a 20-frame motion tween with `trace("hello from Flash 8");`, built by
> `make-smoke.jsfl`. No Test Movie trace was captured for it.

These fixtures test milestone 1 (open XFL, publish SWF8, preview in Ruffle). Each fixture was built
by the real Macromedia Flash 8 IDE (WIN 8,0,0,478), which also saved and published it. JPEXS
FLA Convert 3.0.0 (`--format CS5 --container xfl --charset WINDOWS-1256`) then turned each FLA
into XFL. Built on 2026-10-28.

```
C:\dev\samples\fixtures\NAME\NAME.fla          saved by Flash 8
C:\dev\samples\fixtures\NAME\NAME.swf          published by Flash 8 (default profile: Player 8, AS2, compressed)
C:\dev\samples\fixtures\NAME\xfl\              JPEXS output: DOMDocument.xml, LIBRARY\, PublishSettings.xml, conversion-report.txt
C:\dev\samples\fixtures\_src\                  generated inputs: alpha-disc.png, bars.jpg (Pillow), grad-src.swf (see gradients)
```

Builders and tools are in `C:\dev\oracle`:

| file | purpose |
|---|---|
| `fixtures\NAME.jsfl` | builder for one fixture. Each step is wrapped in try/catch and logged to `fixtures\NAME.log`, which ends with `FAILED STEPS: n` and `DONE`. |
| `run-jsfl.ps1` | launcher. Parameters: `-Script -Log [-TimeoutSec 120] [-StallSec 20] [-QuitGraceSec 20] [-KeepFlashOpen]`. Starts Flash with its working directory set to its own folder, waits for `DONE`, and prints the log. Kills Flash on timeout (exit 2), on a blocking dialog (exit 3; prints the dialog title and texts), or when Flash exits without `DONE` (exit 4). |
| `build-all.ps1` | `[-Names a,b] [-SkipTrace]`. Runs build, then JPEXS conversion, then trace capture for every fixture, strictly one Flash instance at a time. |
| `capture-trace.ps1` | `-Name X [-PlaySec n]`. Copies the FLA to `tracecap\X\`, runs Flash 8 **Test Movie**, and saves the Output panel to `tracecap\X\trace.txt`. |
| `swfdump.py` | SWF 8 dumper. Covers the header, shapes (styles, LINESTYLE2, focal gradients, per-style edge usage), PlaceObject2/3 (filters, blend modes, cacheAsBitmap, clip actions), buttons, text, fonts (code tables), bitmaps and AVM1 disassembly. |
| `xfl-summary.py` | readable summary of an XFL folder: layers, frames, elements, styles, filters and colour transforms. |
| `verify-fixtures.py` | the header and presence checks below, plus tag census, JPEXS notes and captured traces. |
| `make-images.py`, `make-gradient-swf.py` | regenerate `_src`. |
| `probe\` | the probe scripts and outputs behind the Flash 8 findings below (for reference; they log to `C:\dev\oracle`). |

**How the expected traces were obtained:** each SWF was played in Flash 8's own Test Movie player,
without mouse input, and the Output panel was saved. For every fixture the Test Movie SWF is
byte-identical (after zlib decompression) to the published `NAME.swf`. The traces below are
therefore what Flash Player 8 printed for exactly these files.

---

## Verification (all fixtures)

The first bytes were read with Python. Stage, fps and frame count come from the decompressed SWF header.

| fixture | FLA bytes | SWF bytes | first 8 bytes | sig | ver | stage | fps | frames | xfl/DOMDocument.xml | result |
|---|---|---|---|---|---|---|---|---|---|---|
| shapes-basic | 24064 | 860 | `43 57 53 08 be 03 00 00` | CWS | 8 | 550x400 | 12 | 1 | yes (69056 B) | OK |
| gradients | 23040 | 702 | `43 57 53 08 0f 03 00 00` | CWS | 8 | 550x400 | 12 | 1 | yes (55955 B) | OK |
| symbols | 34304 | 1144 | `43 57 53 08 c3 06 00 00` | CWS | 8 | 640x480 | 12 | 1 | yes (54079 B) | OK |
| tweens | 34304 | 2814 | `43 57 53 08 88 17 00 00` | CWS | 8 | 550x400 | 24 | 60 | yes (53259 B) | OK |
| as2 | 28160 | 3682 | `43 57 53 08 4e 1b 00 00` | CWS | 8 | 400x300 | 12 | 12 | yes (40467 B) | OK |
| text | 23040 | 11764 | `43 57 53 08 d5 3b 00 00` | CWS | 8 | 550x400 | 12 | 1 | yes (42393 B) | OK |
| bitmaps | 33280 | 8362 | `43 57 53 08 4b 22 00 00` | CWS | 8 | 550x400 | 12 | 1 | yes (41312 B) | OK |
| groups | 29696 | 1252 | `43 57 53 08 34 06 00 00` | CWS | 8 | 550x400 | 12 | 1 | yes (69374 B) | OK |
| masks | 35840 | 1833 | `43 57 53 08 ea 0a 00 00` | CWS | 8 | 550x400 | 12 | 20 | yes (81317 B) | OK |
| scenes | 32256 | 734 | `43 57 53 08 a0 05 00 00` | CWS | 8 | 550x400 | 12 | 13 | yes (52442 B) | OK |

Every JSFL log ends with `FAILED STEPS: 0` (after the workarounds below). Every `xfl\` folder contains
`conversion-report.txt`.

---

## Flash 8 JSFL problems found (all fixtures)

| # | API / situation | what happens in Flash 8 | workaround used |
|---|---|---|---|
| J1 | Launching `Flash.exe script.jsfl` with a non-ASCII current directory | Flash blocks on a modal "Flash 8 Language" dialog. | `run-jsfl.ps1` starts Flash in its install folder. |
| J2 | `doc.getCustomFill()` / `getCustomStroke()` (toolbar) | Returns a stub (`{style:"noFill"}` / `{style:"noStroke"}`). The toolbar cannot be read back; only `style`, `color`, `thickness`, `capType` and `joinType` set on the stub are honoured. | Solid fills: stub + `style` + `color` works (`"#RRGGBBAA"` gives alpha). |
| J3 | `setCustomFill()` with `linearGradient`/`radialGradient` built on the stub | **Broken.** Flash stores uninitialised memory as the gradient (random stops, matrix, spread; the SWF shows it too). Number and string colours, object literals and explicit matrices all fail. | Import `_src\grad-src.swf` with `importFile`, read a *valid* Fill via `mouseClick` + `getCustomFill("selection")`, then modify `colorArray`, `posArray`, `matrix`, `focalPoint`, `overflow`, `linearRGB` and `style`. All of these work on a valid object. |
| J4 | Gradient matrix | The Fill's `matrix` is applied verbatim in stage coordinates. It is not fitted to the drawn shape. | Each gradient gets an explicit box: `a = width/1638.4`, `d = height/1638.4`, `tx,ty` = centre (rotation via `b,c`). |
| J5 | `doc.importSWF(uri)` | Never returned; the script hung. | `doc.importFile(uri, false)`. |
| J6 | `importFile` of a SWF | DefineShape4 shapes are silently dropped. Shapes arrive as groups and need one `breakApart()`. A variant with two breakApart passes hung Flash once; the cause was not isolated. | The source SWF uses DefineShape3 only (linear and radial), and focal gradients come from `focalPoint`. One breakApart. |
| J7 | Setting `miterLimit`, `scaleType` or `strokeHinting` on the stub Stroke | `exportSWF` crashes Flash (exit 0xC000041D) or hangs. Left unset, strokes are stored with **miter limit 0 and scale "none"**, which is not the UI default. | A *valid* Stroke is read from a throwaway line on a temporary layer, which is then deleted. That object is reused as a template; colour must also go into `stroke.shapeFill`. All fixture strokes now have miter limit 3 and scale "normal" (1.5 where stated). |
| J8 | `stroke.shapeFill` (gradient strokes) | Ignored on the stub. | Draw a solid stroke, select it (`mouseDblClk` or `doc.selection=[shape]`), take `getCustomStroke("selection")`, set `shapeFill` to a valid gradient Fill, then `setCustomStroke`. |
| J9 | Fill object and bitmaps | There is no bitmap style. `getCustomFill("selection")` on a bitmap-filled shape reports `solid #ff0000` with matrix a=20, so a bitmap fill cannot be assigned to a new shape. | Break apart a bitmap instance (this yields a bitmap fill). For a non-rectangular bitmap fill, cut it with an oval stroke in merge mode and delete the outside region. |
| J10 | `doc.addItem()` / `library.addItemToDocument()` | Often returns `true` but places nothing, especially as the first placement after drawing. | Retry up to 3x (`addItem`, then `addItemToDocument`). Success is checked on the layer's element list, not on `doc.selection`. |
| J11 | Current layer | `doc.selection=[el]`, `mouseClick`, `setInstance*` silently move Flash's current layer. `addItem` and drawing then go to the wrong layer. In the first gradients build, every gradient shape landed on the import layer and was deleted with it. | `useLayer(name)` (`setSelectedLayers` + `currentLayer`) before every placement or drawing; the current layer is restored after `mouseClick`. |
| J12 | `tl.addNewLayer` | Adds above the current layer. New layers already span the timeline's full length. | `newLayer()` always adds on top. `insertFrames(n, false, len)` extends the shorter layers. |
| J13 | `doc.addNewText(rect)` | The rect width is ignored; fields are auto-expanding. | Accepted; real bounds are logged. |
| J14 | `getCustomFill("selection")` / `getCustomStroke("selection")` | They work. Before J3 was understood they returned the garbage that was stored, which is how J3 was found. | Used for read-back in the logs. |
| J15 | Edited document published directly | Flash keeps unused fill and line styles of edited shapes in memory (the deleted donut centre, a replaced stroke) and writes them into the SWF. They are not saved in the FLA. | Every builder saves, closes, **reopens the FLA and publishes from it**. The SWF therefore matches what JPEXS and Al-Jazari read. |
| J16 | Not in Flash 8 (no workaround needed) | `addNewPrimitiveOval`, `addNewPolystar`, `setPlayerVersion`, `setActionScriptVersion` are undefined. | Defaults are used. `fixtures\shapes-basic.profile.xml` (exported profile) confirms `<Version>8</Version> <ActionScriptVersion>2</ActionScriptVersion> <CompressMovie>1</CompressMovie>`. |
| J17 | Automation hiccups | The "Macromedia Product Registration" nag appears at random, usually at `fl.quit`. One run of the bitmaps builder hung (no dialog) in the place + `breakApart` step and passed on retry. | The launcher recognises the nag (kills Flash immediately after `DONE`; counts it as a blocking dialog before `DONE`). Timeouts kill Flash; rerun the fixture. |

APIs that worked as documented include `fl.objectDrawingMode`, `addNewRectangle/Oval/Line`,
`mouseClick/mouseDblClk`, `deleteSelection`, `breakApart`, `library.addNewItem/editItem`, `exitEditMode`,
`setInstanceAlpha/Tint/Brightness`, `colorMode` and the advanced colour fields, `addFilter`/`setFilterProperty`/`getFilters`,
`setBlendMode`, `cacheAsBitmap`, `createMotionTween` and its frame properties (`motionTweenRotate`, `...Times`, `tweenEasing`),
`symbolInstance.loop/firstFrame`, `actionScript` on frames and instances, frame `name` labels, the Text `setTextAttr`/`textType`/
`variableName`/`lineType` properties, `importFile` for PNG and JPEG, `saveDocument`, `exportSWF`, `testMovie` and `outputPanel.save`.

## How Flash 8 published these SWFs (matters when comparing our output)

- **Raw shapes on adjacent layers become one shape.** All raw shapes on a timeline (across layers, with no
  symbol between them) go into one DefineShape character with one "new styles" block per layer, at one
  depth. Example: shapes-basic has 7 layers and produces 1 DefineShape4 with 6 new-style blocks, placed at depth 7.
- **Shape tag version.** Flash 8 chooses the version itself. Solid fill-only shapes use DefineShape (v1).
  Most stroked shapes and focal gradients use DefineShape4. The bitmaps scene (bitmap fills plus one
  round/round stroke) uses DefineShape2.
- **Bitmap instances.** They are published as bitmap-filled rectangles (clipped, non-smoothed fills)
  inside the merged scene shape; there is no separate bitmap placement. Each instance's style block also
  carries an unused placeholder fill whose bitmap id is 65535.
- **Graphic symbols.** They are flattened into the parent timeline: each inner keyframe becomes its own
  DefineShape, swapped per frame with `PlaceObject2 move char=N`. No DefineSprite is written for them.
- **PlaceObject version.** PlaceObject3 is used only when there are filters, a blend mode or cacheAsBitmap;
  otherwise PlaceObject2. Filters always have the CompositeSource flag set, and passes = quality
  (low 1, medium 2, high 3). BevelFilter colours are written highlight first, then shadow (the SWF spec
  says the reverse).
- **FIXED8 rounding.** Drop-shadow strength 80% becomes 0.796875. Alpha 40% becomes multiplier 102/256
  (0.3984). Tint 60% becomes mult 0.398 + add 153. Focal point 180 becomes 180/256.
- **Dynamic text.** The HTML flag is set and the initial text is wrapped in `<p><font ...>` even though
  "render as HTML" is off. A device font (`_sans`) is still written as a DefineFont3 with 0 glyphs
  (5.4 KB of layout data).
- **Bitmap data.** JPEG data is embedded unchanged (DefineBitsJPEG2). PNG becomes DefineBitsLossless2
  format 5 (premultiplied). Flash's PNG import is **not bit-exact**: several channel values are shifted
  by 1, for example (230,40,40) becomes (230,39,39), (40,90,230) becomes (39,89,230), and alpha 1 becomes 2,
  111 becomes 112, 128 becomes 129.

## JPEXS conversion (all fixtures)

- `conversion-report.txt` is the same for every fixture apart from one extra line in as2:
  - 10 `PUBLISH_PROPERTY ... Unmapped publish property retained in the source model` lines
    (`PublishRNWKProperties::mediaCopyright/mediaDescription/mediaKeywords/mediaTitle/mediaAuthor/showBitrateDlog`,
    `PublishFormatProperties::generatorDefaultName/generatorFileName/generator`, `Vector::Template`);
  - `EXPORT_SCOPE ... Experimental authoring export`;
  - as2 also has `FONT_FACE [Page 1:1557]: Archived family has no installed matching face: _sans`.
- `PublishSettings.xml` agrees with Flash's exported profile: Version 8, ActionScriptVersion 2, CompressMovie 1.
  Empty values are omitted, and `TemplateFileName` is reduced to `Default.html`.
- Library symbols are written as `LIBRARY/symbol-N.xml` (with `<Include href="symbol-N.xml"/>`), not
  `LIBRARY/<name>.xml` as Flash CS5 does. The item name is only in the `name` attribute, so readers must
  follow `href`.
- Problems found are listed per fixture. The most important is that **tween easing comes out with the
  sign inverted** (see tweens).

---

## shapes-basic

- **Covers:** raw shapes on the main timeline (no symbols); solid fills (including alpha fills `#00CC00`
  a=0x99 and `#33AA33` a=0x80); ovals (curves); a rounded rectangle; two shapes with holes; strokes of
  1/4/10/20 px with caps none, round and square; joins miter/round/bevel plus miter with limit 1.5;
  overlapping shapes on three layers.
- **Stage / frames:** 550x400, white, 12 fps, **1 frame**. Merge drawing mode.
- **Layers (top to bottom):**

  | layer | content |
  |---|---|
  | `actions` | frame script |
  | `lines` | 12 horizontal lines, 45 px long. Rows y=30/70/110 are caps none/round/square; columns x=20/90/160/230 are widths 1/4/10/20; join round. |
  | `joins` | 4 columns at x=20/85/150/215. Each has a 50x50 rect outline (12 px `#336699`, y 150-200) and a V polyline (two lines, 10 px `#993366`, tip at y=240, feet at y=300). Columns: miter (limit 3), round, bevel, miter limit 1.5 (the V tip, miter ratio about 2.6, is cut to a bevel). |
  | `solid` | Rect `#3366CC` (300,20)-(380,80) without stroke; rect `#FFCC00` (400,20)-(480,80) with a 2 px miter-join stroke; oval `#CC3366` (300,100)-(400,160) with a 3 px stroke; circle `#33AA33` a=50% (410,100)-(470,160). |
  | `holes` | Donut `#008080`: outer oval (300,180)-(400,280), hole (325,205)-(375,255), no stroke. Square frame `#996633` (420,180)-(520,280) with a 50x50 hole and 2 px strokes on both contours. Each hole was made by drawing an inner shape in another colour, `mouseClick`-ing it and deleting it. |
  | `front` | Rounded rect (radius 15) `#0000FF` (380,310)-(520,380), 4 px black stroke. |
  | `mid` | Oval `#00CC00` a=60% (320,290)-(440,390). |
  | `back` | Rect `#FF0000` (260,300)-(400,390). |

- **Expected trace:**
  ```
  shapes-basic: frame 1
  ```
- **JSFL problems / workarounds:** J2, J7 (stroke template, so miter limits are 3 or 1.5 and scale is
  normal), J11, J15. No step failed.
- **JPEXS:** no warnings beyond the routine ones. The XFL matches the build: each layer's edge bounding
  box equals the drawn geometry (lines (20,30)-(275,110), joins (20,150)-(265,300), holes
  (300,180)-(520,280) and so on); hole contours carry the fill on the opposite side (`fillStyle0`);
  caps, joins, miter limits 3/1.5, `scaleMode="normal"` and weights are all preserved; alpha comes out
  as 0.50196 and 0.6. Every stroke has `sharpCorners="true"`, which matches Flash's `breakAtCorners=true`
  and only affects authoring.
- **SWF:** a single DefineShape4 (874 bytes) for all layers, PlaceObject2 at depth 7.

## gradients

- **Covers:**
  - linear 3-stop;
  - linear with alpha stops (0 / 255 / 128) rotated 90 degrees;
  - linear with linearRGB interpolation and reflect spread (34 px box);
  - radial;
  - radial with alpha stops;
  - focal radial with focalPoint +180;
  - focal radial with focalPoint -120 and repeat spread (40 px box) on a rectangle;
  - a 15-stop linear gradient (the Flash 8 maximum);
  - a linear gradient stroke (14 px) and a radial gradient stroke (10 px);
  - black backdrop bars, to show the alpha.
- **Stage / frames:** 550x400, bg `#E0E0E0`, 12 fps, **1 frame**.
- **Layers (top to bottom):**

  | layer | content |
  |---|---|
  | `actions` | frame script |
  | `stroke-radial` | Oval outline (260,320)-(400,395), radial `#FFFF00` to `#FF00FF`. |
  | `strokes` | Rect outline (25,330)-(215,385), linear R/G/B. |
  | `fills` | Row 1 (y 90-170): R1, R2, R3 at x 20/190/360. Row 2 (y 180-310): O1-O3 ovals at x 20/170/320 and a 75x130 rect at x 465. 15-stop rect at (440,320)-(540,395). |
  | `backdrop` | Bars y 120-140 and x 225-245. |

  The source-import layer was deleted before saving.

- **Expected trace:**
  ```
  gradients: frame 1
  ```
- **JSFL problems / workarounds:** J3 (the main one; the first build stored garbage gradients), J4, J5,
  J6, J7, J8, J11 (one build drew every gradient onto the import layer), J14, J15. No step failed in the
  final build.
- **JPEXS:** no warnings beyond the routine ones. All gradients are correct in the XFL: spread, `interpolationMethod`,
  alpha stops, all 15 entries and both gradient strokes (`SolidStroke` with a gradient `<fill>`).
  - **Focal ratio:** the XFL writes `focalPointRatio = focalPoint/255` (0.70588, -0.47059), while Flash 8's
    SWF stores FIXED8 `focalPoint/256` (0.7031 = 180/256, -0.4688 = -120/256). A publisher computing
    `round(0.70588*256)` gets 181, not Flash's 180. The difference is 1/256.
- **SWF:** one DefineShape4 with fill types 0x10, 0x12 and 0x13 (focal), and LINESTYLE2 with a gradient fill.

## symbols

- **Covers:** nested movie clips with instance names; colour effects alpha, tint, brightness and
  advanced; drop shadow, blur, glow, bevel and a two-filter stack; blend modes multiply, screen and add;
  cacheAsBitmap.
- **Stage / frames:** 640x480, white, 12 fps, **1 frame**.
- **Library:**
  - `Inner` (MC): orange disc r=20 with a black marker.
  - `Outer` (MC): layer `shape` holds a rounded rect 120x70 (`#3366CC`, 2 px stroke); layer `inner` holds
    `innerA_mc` at (-25,0) and `innerB_mc` at (30,0) scaled 0.5.
  - `Box` (MC): 60x60 with red, green, blue and white quadrants and a 2 px black border.
- **Main timeline (top to bottom):**

  | layer | content |
  |---|---|
  | `actions` | frame script |
  | `blend` | y=339.95 (Flash's placement rounding): `normal_mc` x90, `multiply_mc` x190, `screen_mc` x290, `add_mc` x390, `cache_mc` x560 (cacheAsBitmap). |
  | `filters` | y=200: `drop_mc` x80 (distance 8, angle 45, blur 6, strength 80%, high, `#333333`); `blur_mc` x200 (10x4, medium); `glow_mc` x320 (red, blur 12, strength 200%, low); `bevel_mc` x440 (inner, distance 5, angle 45, blur 5, strength 100%, highlight white / shadow black); `multi_mc` x560 (inner yellow glow blur 8, then drop shadow distance 6). |
  | `color` | y=70: `plain_mc` x220; `alpha_mc` x310 (40%); `tint_mc` x400 (`#0000FF` 60%); `bright_mc` x490 (-50%); `advanced_mc` x580 (R 50%+100, G 100%-60, B 20%+0, A 80%). |
  | `nested` | `outer_mc` at (90,70). |
  | `backdrop` | 8 vertical colour stripes (40-440, y 290-390) under the blend row. |

- **Expected trace:**
  ```
  symbols: frame 1
  nested: innerA_mc, innerB_mc
  innerB_mc._xscale = 50
  alpha_mc._alpha = 40
  drop_mc.filters.length = 1
  multi_mc.filters.length = 2
  multiply_mc.blendMode = multiply
  add_mc.blendMode = add
  cache_mc.cacheAsBitmap = true
  ```
- **JSFL problems / workarounds:** J7, J10 (several placements needed 2 to 4 attempts), J11, J15. No step failed.
- **JPEXS:** no warnings beyond the routine ones. Names, matrices, nesting, `blendMode`,
  `cacheAsBitmap`, filter order and parameters (quality 1/2/3, bevel `type=inner`) and colour
  transforms (`tintColor/tintMultiplier`, `brightness=-0.5`, advanced multipliers and offsets) are
  preserved. Two notes:
  - multipliers keep Flash's x/256 values: `alphaMultiplier=0.3984375`, not 0.4, and 0.80078125 for 80%;
  - the drop-shadow `strength` is 0.8 in the XFL but 0.796875 in the SWF.
- **SWF:**
  - PlaceObject2 for the colour-effect instances (cxform only); PlaceObject3 for filters, blend modes and cache;
  - `Inner` and the stripes are DefineShape (v1); `Outer`'s rect and `Box` are DefineShape4.

## tweens

- **Covers:** classic motion tweens with clockwise x1, counter-clockwise x2 and auto rotation; scale
  with ease-in -100 and ease-out +100; alpha 100 to 0 to 100; consecutive tweens on one layer (easing
  0, +50, -50); a 10-frame graphic symbol placed as loop (firstFrame 0), loop (firstFrame 7),
  play once (firstFrame 3) and single frame (firstFrame 6).
- **Stage / frames:** 550x400, white, 24 fps, **60 frames**. Frame 60 calls `stop()`.
- **Library:**
  - `Arrow` (MC): blue body (-30,-10)-(20,10) with a red tip (20,-5)-(30,5).
  - `Counter` (graphic): 10 keyframes. Frame n shows a bar from x=-40, `8*(n+1)` px wide and 16 px tall,
    in its own colour (`#E6194B`, `#3CB44B`, `#FFE119`, `#4363D8`, `#F58231`, `#911EB4`, `#46F0F0`,
    `#F032E6`, `#BCF60C`, `#000000`).
- **Layers (keyframes are 0-based):**

  | layer | content |
  |---|---|
  | `rotate_cw` | 0 (60,50), CW x1 to 23 (260,50), CCW x2 to 47 (60,50), hold to 59. |
  | `rotate_auto` | 0 (330,50), auto to 23 (500,50) rotated 90, hold. |
  | `scale_ease` | 0 (100,150) x1, ease -100 to 23 at x2, ease +100 to 47 at x0.5, hold. |
  | `alpha` | 0 at alpha 100, tween to 29 at alpha 0, tween to 59 at alpha 100 (330,150). |
  | `consecutive` | 0 (60,250) to 15 (200,250) [ease 0], to 30 (200,320) rot 90 [+50], to 45 (60,320) rot 180 [-50], hold. |
  | `graphics` | Counter instances at tx 106, 236, 366, 496, y 374.95 (addItem centres frame 0's bounds). |
  | `actions` | scripts on frames 0 and 59. |

- **Expected trace:**
  ```
  tweens: frame 1
  tweens: frame 60
  ```
- **JSFL problems / workarounds:** J10, J11, J12. No step failed.
- **JPEXS:** no warnings beyond the routine ones.
  - **Problem: easing sign inverted.** Flash 8 reports `tweenEasing = -100` (ease in). The SWF confirms a
    slow start: scale 1.0019, 1.0076, 1.0170, 1.0302 ... JPEXS writes `acceleration="100"`. Likewise +100
    becomes -100, +50 becomes -50 and -50 becomes +50. XFL convention, as also documented in
    `C:\dev\ref\fla-viewer\AGENTS.md`, is negative = ease in. Until this is fixed, an XFL-driven
    publisher will ease these tweens the wrong way.
  - Everything else is correct: rotate direction and count, `motionTweenScale`, keyframe spans, alpha
    colour, `loop`/`firstFrame`.
- **SWF:** tweens are baked into per-frame PlaceObject2 moves (402 PlaceObject2, 65 RemoveObject2,
  61 ShowFrame). `Counter` is flattened into 10 DefineShape characters swapped per frame, and `Arrow` is DefineSprite id 6.

## as2

- **Covers:** frame scripts (variables, functions, trace); frame labels with `gotoAndPlay("loop")`;
  `stop()` at the end; a 4-state button with `on(release)`; a clip with `onClipEvent(load)` and
  `onClipEvent(enterFrame)` that traces only three times; a dynamic text field bound to the variable `scoreText`.
- **Stage / frames:** 400x300, white, 12 fps, **12 frames**. Labels: `loop` = frame 5, `end` = frame 12.
- **Library:**
  - `Btn` (button): Up is green, Over yellow and Down red. Each is a 100x40 rect with radius 8 and a
    2 px stroke. Hit is a black 110x50 rect.
  - `Ticker` (MC): purple disc r=25.
- **Main timeline:**

  | layer | content |
  |---|---|
  | `actions` | scripts on frames 1, 5, 8 and 12 |
  | `labels` | `loop`, `end` |
  | `text` | dynamic field at (20,20), `_sans` 20 px, single line, var `scoreText`, initial text `score: ?` |
  | `clip` | `ticker_mc` at (300,220) |
  | `button` | `my_btn` at (110,220) |

- **Scripts:**
  - **Frame 1:**
    ```
    var passes = 0; function greet(who) {...} function add(a, b) {...}
    trace greet / add / typeof my_btn + ticker_mc._name
    scoreText = "score: 0"
    ```
  - **Frame 5:** `passes++`, a trace, `scoreText = "score: " + passes*10`.
  - **Frame 8:** `if (passes < 2) { trace; gotoAndPlay("loop") } else trace`.
  - **Frame 12:** trace `_currentframe` and `scoreText`, then `stop()`.
  - **`ticker_mc`:** `onClipEvent(load)` sets `this.n = 0` and traces; `onClipEvent(enterFrame)` traces
    while `n < 3`.
  - **`my_btn`:** `on (release) { trace("my_btn: release"); }`.

  The full source is in `fixtures\as2.jsfl` and the XFL.
- **Expected trace (no input):**
  ```
  frame 1: Hello, Flash 8!
  frame 1: add(2, 3) = 5
  frame 1: typeof my_btn = object, ticker_mc._name = ticker_mc
  ticker_mc: load
  ticker_mc: enterFrame 1
  ticker_mc: enterFrame 2
  ticker_mc: enterFrame 3
  frame 5 [loop]: pass 1
  frame 8: gotoAndPlay("loop")
  frame 5 [loop]: pass 2
  frame 8: continue
  frame 12 [end]: _currentframe = 12, scoreText = score: 20
  ```
  The frame-1 script runs before `onClipEvent(load)`. A click on the button would add `my_btn: release`.
- **JSFL problems / workarounds:** J7, J10, J11, J15. No step failed.
- **JPEXS:**
  - Warning `FONT_FACE: Archived family has no installed matching face: _sans`; the family name is kept.
  - All scripts, both labels, the 4 button frames, the clip actions, `variableName`, `lineType` and
    `autoExpand` are preserved.
- **SWF:**
  - functions compile to DefineFunction2;
  - clip actions are in PlaceObject2 (events `load` and `enterFrame`);
  - DefineButton2 uses condition `overDownToOverUp` (= release);
  - DefineEditText has the HTML flag set with initial text
    `<p align="left"><font face="_sans" size="20" ...>score: ?</font></p>`.

## text

- **Covers:**
  - static English text in Arial and Times New Roman, with multi-line content, left/center/right
    alignment, sizes 12/18/24/30, colours, and bold italic;
  - static Arabic text in Tahoma;
  - a dynamic text field with English text.
- **Stage / frames:** 550x400, white, 12 fps, **1 frame**.
- **Content** (bounds as Flash reported them; all fields are auto-expanding):

  | layer | content |
  |---|---|
  | `static` | `Left aligned, Arial 18\rsecond line of static text`: Arial 18, black, left, bounds (20,20) 196x46. |
  | | `Centered, Times 24\rline two`: TNR 24, `#CC0000`, center, (290,20) 191x59. |
  | | `Right aligned, Arial 12\ra third line follows\rand a fourth`: Arial 12, `#0033CC`, right, (20,110) 121x48. |
  | | `Bold Italic 30`: TNR 30, bold italic, `#008800`, (290,110) 173x37. |
  | `arabic` | U+0645 U+0631 U+062D U+0628 U+0627 U+0020 U+0628 U+0627 U+0644 U+0639 U+0627 U+0644 U+0645 ("marhaban bil-alam"): Tahoma 28, black, (20,200) 192x38. |
  | `dynamic` | `dyn_txt`: dynamic, single line, Arial 16, `#333333`, (20,290) 131x33, text `Dynamic text field`. |

- **Expected trace:**
  ```
  text: frame 1
  dyn_txt.text = Dynamic text field
  ```
- **Arabic, as expected:** Flash 8 stores the 13 base letters in logical order, exactly as typed
  (no presentation forms). It embeds Tahoma glyphs for U+0020 U+0627 U+0628 U+062D U+0631 U+0639 U+0644
  U+0645 and lays them out **left to right as isolated, unjoined letters**. The DefineText glyph
  sequence follows logical order (م ر ح ب ا space ب ا ل ع ا ل م).
- **JSFL problems / workarounds:** J13. No step failed.
- **JPEXS:** no warnings beyond the routine ones. Text, `\r` breaks, alignment, sizes, colours and the
  Arabic code points are preserved. Two notes:
  - Faces come out as PostScript names, with bold and italic folded into the name (the CS5 convention):
    `ArialMT`, `TimesNewRomanPSMT`, `TimesNewRomanPS-BoldItalicMT`, `Tahoma`. A reader needs a
    PostScript-to-family mapping.
  - XFL `width` excludes Flash's 2 px gutters, and the matrix tx/ty = left/top + 2.
- **SWF:**
  - 4 DefineFont3 tags containing only the glyphs used: Arial 25, TNR 17, TNR Bold Italic 12, Tahoma 8;
  - 5 DefineText tags plus the edit text, each followed by CSMTextSettings (6 in all; FlashType advanced anti-aliasing);
  - the dynamic field references the embedded Arial (font id 1) without `useOutlines`, so it renders as device text.

## bitmaps

- **Covers:**
  - an imported PNG with transparency and an imported JPEG;
  - plain placements;
  - a scaled and rotated placement of each;
  - a bitmap fill (a broken-apart JPEG);
  - a rotated, bitmap-filled oval with a stroke.
- **Stage / frames:** 550x400, bg `#CCE0FF` (shows the PNG alpha), 12 fps, **1 frame**.
- **Library:**
  - `alpha-disc.png`: 128x128 RGBA, lossless, smoothing off. Opaque quadrants, a 50% white ring and
    an alpha falloff.
  - `bars.jpg`: 160x120, "photo", imported JPEG data, smoothing off.
- **Placements:**

  | layer | content |
  |---|---|
  | `bitmaps` | PNG at matrix (1,0,0,1,26,26). PNG scaled 1.25 and rotated 30 (1.0825,0.625,-0.625,1.0825,100.7,180.7). JPEG at (1,0,0,1,220,20). JPEG scaled 0.75 and rotated -15 (0.7244,-0.1941,0.1941,0.7244,230.35,222.05). |
  | `bitmapfill` | JPEG broken apart at (385,20): a shape with a bitmap fill, matrix a=d=20. |
  | `bitmapfill-oval` | JPEG broken apart, cut by an oval stroke (400,235)-(540,335), outside deleted, rotated 20 degrees. Bitmap-fill matrix (18.79,6.84,-6.84,18.79,415.4,201.3), 2 px black stroke. |

- **Expected trace:**
  ```
  bitmaps: frame 1
  ```
- **JSFL problems / workarounds:** J7, J9 (no bitmap Fill object; break-apart workaround), J10, J17
  (one hang, passed on retry). No step failed in the final build.
- **JPEXS:** no warnings beyond the routine ones.
  - Media is written as `LIBRARY/bitmap-1.png` and `LIBRARY/bitmap-2.jpg`, plus `bin/bitmap1.dat` and
    `bin/bitmap2.dat`. `DOMBitmapItem` keeps `compressionType`, `allowSmoothing=false`,
    `useImportedJPEGData=true` and `sourceExternalFilepath="..\_src\..."`.
  - The JPEG is byte-identical to the source.
  - The PNG equals the pixels Flash 8 stored, not the source file, because of Flash's import shift
    (see the publishing notes). Compare against the SWF's DefineBitsLossless2, not `_src\alpha-disc.png`.
  - `DOMBitmapInstance` matrices are correct.
  - Both bitmap-filled shapes come out as `BitmapFill` with `bitmapIsClipped="false"` (the SWF uses
    repeating fills), with the right matrices.
- **SWF:**
  - DefineBitsJPEG2 (3744 bytes = the source JPEG);
  - DefineBitsLossless2 128x128 format 5;
  - one DefineShape2 holding everything. The four instances use clipped fills, each with the extra
    placeholder fill whose bitmap id is 65535. The two break-apart shapes use repeating fills.

## groups

- **Covers:** raw shapes grouped, one group per layer: untransformed, moved, scaled, rotated and skewed
  after grouping; three levels of nested groups, each transformed after grouping, the innermost holding a
  named movie clip; a group holding a movie clip that was scaled and rotated before grouping; two groups, a
  Flash 8 drawing object (rotated) and a raw shape on one layer; a group with a named instance inside a
  movie clip. Every group has its own colours, so SWF fill styles can be matched to XFL shapes. Purpose:
  settle how the XFL that JPEXS writes stores group member matrices.
- **Stage / frames:** 550x400, white, 12 fps, **1 frame**. Merge drawing mode, except the one drawing object.
- **Library:**
  - `Marker` (MC): blue body (-20,-10)-(12,10) `#3366CC` and red tip (12,-5)-(20,5) `#FF0000`.
  - `GroupClip` (MC): one group of a rect `#9966CC` (-40,-20)-(20,20), an oval `#FFCC00` (25,-15)-(45,5)
    and `Marker` `inclip_mc` at (0,30), rotated 30 after grouping.
- **Main timeline (top to bottom).** The right-hand column is the group's own matrix as Flash 8 reports it
  (JSFL `element.matrix` after the transform; Flash transforms about the group's centre, hence the
  translations).

  | layer | content: as drawn, then the transform applied after grouping | Flash's group matrix |
  |---|---|---|
  | `actions` | frame script | |
  | `clip` | `GroupClip` `clip_mc` at (340,330), scaled 0.8 and rotated 10: (0.7878,0.1389,-0.1389,0.7878,337.05,326.35) | (an instance) |
  | `stack` | group Ga: rect `#FF00FF` (40,290)-(140,370). Group Gb: oval `#00FFFF` (100,300)-(200,380), moved (10,0). Drawing object: oval `#FFAA00` (150,280)-(250,330), rotated 20. Raw rect `#DDDDDD` (20,270)-(260,390), drawn last. | Ga identity; Gb (1,0,0,1,10,0); drawing object (0.9397,0.342,-0.342,0.9397,116.35,-50) |
  | `symbol` | `Marker` `ingroup_mc` placed at (470,180), scaled 1.5 and rotated 90: (0,1.5,-1.5,0,470,180); raw rect `#999999` (400,150)-(420,210). Grouped, rotated 30, moved (0,10). | (0.866,0.5,-0.5,0.866,149.3,-187.15) |
  | `nested` | N1 = rect `#FF3333` (170,150)-(210,170) + `Marker` `nested_mc` at (230,160), rotated 45. N2 = N1 + oval `#3333FF` (250,140)-(290,180), scaled (1.25,0.8). N3 = N2 + rect `#33CC33` (160,200)-(300,215), moved (5,5), rotated -15. | N1 (0.7071,0.7071,-0.7071,0.7071,174.65,-101.65); N2 (1.25,0,0,0.8,-58.05,32); N3 (0.9659,-0.2588,0.2588,0.9659,-31.9,71.05) |
  | `skewed` | rect `#6633CC` (20,160)-(70,200) + oval `#CCCC00` (75,165)-(115,205), skewed (20,-10) | (1,0.1763,-0.364,1,66.4,-11.9) |
  | `rotated` | rect `#009999` (430,40)-(480,80) + oval `#996600` (485,45)-(525,85), rotated 30 | (0.866,0.5,-0.5,0.866,95.2,-230.4) |
  | `scaled` | rect `#CC0066` (290,40)-(330,80) + oval `#66CC00` (335,45)-(375,85), scaled (1.5,0.75) | (1.5,0,0,0.75,-166.25,15.65) |
  | `moved` | rect `#0066FF` (150,30)-(200,70) + oval `#FF9900` (205,35)-(245,75), moved (10,20) | (1,0,0,1,10,20) |
  | `plain` | rect `#FF0000` (20,40)-(70,80) + oval `#00AA00` (75,45)-(115,85) | identity |

- **Expected trace** (the script rounds to 2 decimals):
  ```
  groups: frame 1
  ingroup_mc: x=466.35 y=203.75 xscale=150 yscale=150 rotation=120
  nested_mc: x=227 y=179 xscale=104.94 yscale=104.94 rotation=17.62
  clip_mc: x=337.05 y=326.35 xscale=80 yscale=80 rotation=10
  inclip_mc: x=-9.65 y=26.1 xscale=100 yscale=100 rotation=30
  ```
- **The group member matrix rule (JPEXS XFL), with evidence:**
  - A `DOMGroup` never has a `<matrix>` or a `<transformationPoint>`. Flash's own group matrix (right-hand
    column above) is not written anywhere.
  - Every member has an explicit `<matrix>`, the identity included. That matrix is already the product of
    all enclosing group transforms and the member's own transform. It maps the member's local coordinates
    (its edges stay exactly as drawn) straight into the space of the timeline that holds the outermost group.
    Examples: in `nested`, `#33CC33` carries N3 = (0.965927,-0.25882,0.25882,0.965927,-31.9,71.05);
    `#3333FF` carries N3·N2 = (1.20741,-0.323517,0.207047,0.772736,-79.7,117); `#FF3333` carries N3·N2·N1 =
    (1.00017,0.317642,-0.707352,0.775162,110.1,-18.05) and `nested_mc` N3·N2·N1·translate(230,160) =
    (1.00017,0.317642,-0.707352,0.775162,227,179). In `symbol`, `ingroup_mc` carries G·own =
    (-0.75,1.29904,-1.29904,-0.75,466.35,203.75). Raw shapes inside groups therefore have non-identity matrices.
  - **Rule: use a member's matrix as it is and never multiply it by the enclosing groups** (there is nothing
    to multiply by). The absent `DOMGroup` matrix means identity. A member without a matrix never occurs in
    JPEXS output; reading it as identity agrees with everything here, while the fla-viewer rule "a member
    without a matrix inherits the group matrix" could not be tested. Real CS5/CS6 saves are not verified.
  - Member matrix applied to the XFL edges, against the edge bounds of the published shapes
    (`swfdump.py --shapes-full`; Flash bakes the transforms into the edges). All agree within 1 twip:

    | member | XFL edges (as drawn) | XFL member matrix applied | SWF |
    |---|---|---|---|
    | plain `#FF0000` | (20,40)-(70,80) | (20,40)-(70,80) | same |
    | moved `#0066FF` | (150,30)-(200,70) | (160,50)-(210,90) | same |
    | scaled `#CC0066` | (290,40)-(330,80) | (268.75,45.65)-(328.75,75.65) | same |
    | rotated `#009999` | (430,40)-(480,80) | (427.6,19.25)-(490.9,78.9) | same |
    | skewed `#6633CC` | (20,160)-(70,200) | (13.6,151.65)-(78.15,200.45) | same |
    | nested `#33CC33` | (160,200)-(300,215) | (174.4,186.6)-(313.5,237.3) | (174.4,186.6)-(313.55,237.3) |
    | nested `#3333FF` | (250,140)-(290,180) | (253.55,135.15)-(305.3,171.45) | same |
    | nested `#FF3333` | (170,150)-(210,170) | (159.9,152.2)-(214.05,180.45) | same |
    | symbol `#999999` | (400,150)-(420,210) | (390.7,142.75)-(438.05,204.7) | same |
    | stack drawing object `#FFAA00` | (150,280)-(250,330) | (149.45,274.45)-(250.5,335.6) | (149.45,274.45)-(250.45,335.6) |
    | `GroupClip` `#9966CC` | (-40,-20)-(20,20) | (-39.3,-37.2)-(32.65,27.4) | same |

    The PlaceObject2 matrices of the instances equal their XFL member matrices: `nested_mc`
    (1.0002,0.3176,-0.7074,0.7752,227,179), `ingroup_mc` (-0.75,1.299,-1.299,-0.75,466.35,203.75),
    `inclip_mc` (0.866,0.5,-0.5,0.866,-9.65,26.1). Composing with the group matrix again would be wrong: for
    `rotated`, Flash's group matrix times the member matrix gives (438.6,10.1)-(498.2,73.4).
  - **Drawing order.** JPEXS lists the merge-mode raw shape of a frame or group (a `DOMShape` without
    `isDrawingObject`) **after** the groups, drawing objects and instances beside it, but Flash draws it
    **underneath** them. In `stack` the XFL order is Ga, Gb, drawing object, `#DDDDDD`; the SWF draws
    `#DDDDDD`, `#FF00FF`, `#00FFFF`, `#FFAA00` from back to front (and JSFL lists the raw shape first). Inside
    groups it is the same: `symbol` lists (`ingroup_mc`, `#999999`) and publishes `#999999` at depth 11 under
    `ingroup_mc` at 12; `nested` lists (N2, `#33CC33`) and draws `#33CC33` first; `GroupClip` lists
    (`inclip_mc`, raw) and publishes the raw shape at depth 1, `inclip_mc` at 2. Everything else follows
    document order, the first element being the backmost. A reader or renderer must draw the raw shape first.
    Whether real CS5 XFL orders it the same way is not known.
- **JSFL problems / workarounds:** J10 (`Marker` and `GroupClip` placements needed 2 or 3 attempts), J11,
  J19 (members cannot be read back). Each group was made by locking every other layer, `selectAll()` and
  `group()`; Gb was selected with `mouseClick`. `moveSelectionBy`, `scaleSelection`, `rotateSelection`,
  `skewSelection` and `fl.objectDrawingMode` work. No step failed.
- **JPEXS:** no warnings beyond the routine ones. Problems: the group's own matrix and transformation point
  are dropped (members are right, so publishing is unaffected, but an editor cannot recover Flash's group
  box); the raw shape is listed last (see drawing order). The drawing object is `DOMShape
  isDrawingObject="true"` with its own matrix and its edges as drawn.
- **SWF:**
  - Group transforms are baked into the edges and the shapes are placed with the identity matrix; no sprite
    and no separate shape per group.
  - Raw content (raw shapes, group contents, drawing objects) is merged across layers into one shape until
    the next instance, with one new-styles block per group, drawing object or raw shape: id 1, DefineShape2
    with 8 blocks (plain, moved, scaled, rotated, skewed, `#33CC33`, `#3333FF`, `#FF3333`) at depth 8; then
    `nested_mc` at 9; `#999999` alone (DefineShape) at 11; `ingroup_mc` at 12; the `stack` layer (DefineShape2,
    4 blocks) at 17; `clip_mc` at 18. DefineShape2 when there are several blocks (DefineShape has no new
    styles), DefineShape for one.
  - Depths: each piece of raw content takes one depth and the merged shape sits at the depth of its topmost
    piece (pieces 1-8 at 8, 14-17 at 17); each `Marker` instance takes two depths (9-10, 12-13). Inside
    `GroupClip`: raw shape at 1, `inclip_mc` at 2.
  - Instance names inside groups are kept, and AS2 reaches them (see the trace).

## masks

- **Covers:** a raw-shape mask over one masked layer (A); a mask over two masked layers, raw stripes and a
  movie clip (B); a movie clip instance as the mask (C); a mask layer holding a raw shape and an instance
  (D); a motion-tweened mask, a movie clip (E); a masked layer holding a motion tween (F); a movie clip with
  its own mask over two masked layers (G); a mask layer holding two movie clip instances (H).
- **Stage / frames:** 550x400, white, 12 fps, **20 frames**; frame 20 calls `stop()`. The cases sit in 4
  columns x 2 rows of 125x170 cells (A-D on top, E-H below).
- **Library:**
  - `Box` (MC): 40x40, quadrants red `#FF0000`, green `#00CC00`, blue `#0000FF`, black, centred.
  - `Disc` (MC): grey `#666666` disc, r=45.
  - `MaskedClip` (MC), layers top to bottom: `mask` (mask: raw oval (-50,-70)-(50,70)); `box` (masked:
    `Box` `inner_box_mc` at (25,30), scaled 1.5); `bars` (masked: 8 horizontal bars `#911EB4`/`#46F0F0` in
    (-60,-80)-(60,80)).
- **Main timeline (top to bottom).** Stripe and grid colours: `#E6194B`, `#3CB44B`, `#FFE119`, `#4363D8`, `#F58231`.

  | layer | type | content |
  |---|---|---|
  | `actions` | normal | scripts on frames 1 and 20 |
  | `h_mask` | mask | H: `Disc` `h_disc1_mc` at (450,265) and then `Disc` `h_disc2_mc` at (505,330), both scaled 0.5 |
  | `h_art` | masked by `h_mask` | 5 horizontal stripes in (415,210)-(540,380) |
  | `g_clip` | normal | G: `MaskedClip` `maskedclip_mc` at (342,295) |
  | `f_mask` | mask | F: raw rect (160,250)-(255,350) |
  | `f_art` | masked by `f_mask` | `Box` `tweenbox_mc`, motion tween from (165,230) at x1 (frame 1) to (250,360) at x2 (frame 20) |
  | `e_mask` | mask | E: `Disc` `tweenmask_mc` scaled 0.5, motion tween from (40,295) (frame 1) to (105,295) (frame 20) |
  | `e_art` | masked by `e_mask` | 3x4 grid in (10,210)-(135,380) |
  | `d_mask` | mask | D: raw rect (425,35)-(475,95) and `Disc` `maskdisc_mc` at (495,140), scaled 0.6 |
  | `d_art` | masked by `d_mask` | 5 vertical stripes in (415,20)-(540,190) |
  | `c_mask` | mask | C: `Disc` `maskclip_mc` at (342,105) |
  | `c_art` | masked by `c_mask` | 3x4 grid in (280,20)-(405,190) |
  | `b_mask` | mask | B: raw rounded rect (160,50)-(255,160), radius 20 |
  | `b_art2` | masked by `b_mask` | `Box` `b_box_mc` at (245,160), scaled 1.5 (partly outside the mask) |
  | `b_art1` | masked by `b_mask` | 6 horizontal stripes in (145,20)-(270,190) |
  | `a_mask` | mask | A: raw oval (20,40)-(125,170) |
  | `a_art` | masked by `a_mask` | 5 vertical stripes in (10,20)-(135,190) |

- **Expected trace:**
  ```
  masks: frame 1
  maskclip_mc: movieclip, _x = 342
  maskdisc_mc._xscale = undefined
  tweenmask_mc._x = 40, tweenbox_mc._x = 165
  maskedclip_mc.inner_box_mc: inner_box_mc
  h_disc1_mc: movieclip, h_disc2_mc: undefined
  masks: frame 20, tweenmask_mc._x = 105, tweenbox_mc._x = 250
  ```
  `maskdisc_mc` and `h_disc2_mc` do not exist at run time, because Flash 8 did not publish them (see SWF).
- **JSFL problems / workarounds:** J10 (placements needed 2 or 3 attempts), J11, J12 (layers extended with
  `insertFrames`). The content was drawn first; then `layer.layerType = "mask"` on the mask layer and
  `"masked"` on the layers directly below it. Flash 8 has `layer.parentLayer`, and it already pointed at the
  mask. No step failed.
- **JPEXS:** no warnings beyond the routine ones. Mask layers are `layerType="mask"`. Masked layers have no
  `layerType` (normal) and `parentLayerIndex` = the index of their mask layer in the same `<layers>` list
  (in `MaskedClip`, `box` and `bars` have 0). Both elements of `d_mask` and of `h_mask` are kept (on `d_mask`
  the instance comes first and the raw shape last, the order described under groups). The tweens are classic
  motion tweens (`keyMode="7681"`, acceleration 0).
- **SWF (how Flash 8 publishes masks):**
  - The mask content is placed **directly** with a PlaceObject2 `clipDepth`; there is no wrapper sprite. A
    raw mask is a DefineShape; a movie clip mask is the clip's own DefineSprite, placed with its instance
    name (`maskclip_mc`, `tweenmask_mc`, `h_disc1_mc`).
  - **Only one element of a mask layer is published: the backmost.** On `d_mask` the raw rect (raw shapes
    are always underneath) is the mask and `maskdisc_mc` is dropped. On `h_mask` `h_disc1_mc` (placed first)
    is the mask and `h_disc2_mc` is dropped. Nothing warns; the trace shows `undefined` for both.
    ARCHITECTURE section 6 says to wrap the union of the mask layer's elements in a generated sprite; Flash 8
    does not do that. What the Flash 8 stage shows for such a layer was not checked.
  - Depths and clip depths:

    | case | mask: depth, clipDepth | masked content: depth |
    |---|---|---|
    | A | raw oval: 1, 3 | stripes 2 |
    | B | raw rounded rect: 4, 8 | stripes 5, `b_box_mc` 6 |
    | C | `maskclip_mc`: 9, 12 | grid 11 |
    | D | raw rect: 14, 16 | stripes 15 |
    | E | `tweenmask_mc`: 17, 20 | grid 19 |
    | F | raw rect: 22, 25 | `tweenbox_mc` 23 |
    | G | inside `MaskedClip` (placed at depth 26): raw oval 1, 5 | bars 2, `inner_box_mc` 3 |
    | H | `h_disc1_mc`: 32, 35 | stripes 34 |

    The clip depth is always one above the highest depth that the masked layers use, counting the second
    depth that each instance takes. `maskedclip_mc` takes depths 26-31, which matches 1 + the 5 depths its
    sprite uses. The gaps at 13 and 21 follow the two cases whose mask is an instance and are not explained.
    A publisher needs the order and the ranges, not Flash's exact numbers.
  - A tweened mask is one PlaceObject2 Modify per frame at the mask's depth (`clipDepth` only on the first
    placement); a tweened masked clip is like any other tween.
  - Masks inside a movie clip follow the same pattern inside its DefineSprite.
  - Flash 8 reuses one DefineShape for shapes that are equal up to scale and translation, through the
    placement matrix: the `a_mask` oval is `MaskedClip`'s mask oval (char 1) placed with
    (1.05,0,0,0.9286,72.5,105); the `a_art` stripes are the `d_art` stripes (char 2) with tx=-405; the
    `d_mask` rect is the `f_mask` rect (char 10) with (0.5263,0,0,0.6,340.8,-115). An XFL-driven publisher
    will write more DefineShape tags.
  - Every shape is DefineShape (v1: fills only, one style block). Frame 1's DoAction comes first, before
    any definition.

## scenes

- **Covers:** three scenes with frame labels and frame scripts that trace the scene, the frame and
  `_currentframe`; `nextScene()` and `prevScene()` (each followed by `play()`); an unqualified
  `gotoAndPlay(4)` in the second scene; `gotoAndPlay("Intro", 3)`; `gotoAndPlay("finale")`, a label in
  another scene; `gotoAndPlay("Outro", "outro")`; `stop()` at the end. A global counter `hops` makes the
  path deterministic and finite.
- **Stage / frames:** 550x400, white, 12 fps. Scenes `Intro` (4 frames), `Main` (5) and `Outro` (4): **13
  frames** in the SWF.
- **Library:** `Dot` (MC): black disc, r=12.
- **Each scene, layers top to bottom:** `actions`; `labels`; `dot` (a `Dot` motion-tweened from (75,y) to
  (475,y) over the scene, y = 120 / 200 / 280); `art` (raw rect (25,25)-(525,375) in `#FFD8D8` / `#D8FFD8` /
  `#D8D8FF`).
- **Labels and scripts** (frame numbers within the scene, root frames in brackets):

  | scene | labels | scripts |
  |---|---|---|
  | `Intro` [1-4] | `intro` = 1 | 1: `hops = 0` if undefined, trace. 3: trace. 4: trace, `nextScene(); play();` |
  | `Main` [5-9] | `main` = 1, `mainLoop` = 4 | 1: trace. 2: if `hops == 0`, set 1 and `gotoAndPlay(4)`, else trace. 4: trace. 5: `hops` 1 -> 2 and `gotoAndPlay("Intro", 3)`; 2 -> 3 and `gotoAndPlay("finale")`; otherwise `gotoAndPlay("Outro", "outro")`. |
  | `Outro` [10-13] | `outro` = 1, `finale` = 3 | 1: trace. 3: trace. 4: `hops` 3 -> 4 and `prevScene(); play();`, otherwise trace and `stop()`. |

  The full source is in `tools\oracle\fixtures\scenes.jsfl` and the XFL.
- **Expected trace:**
  ```
  Intro 1 [intro]: root 1
  Intro 3: root 3, hops=0
  Intro 4: nextScene(); play()
  Main 1 [main]: root 5
  Main 2: gotoAndPlay(4)
  Main 4 [mainLoop]: root 8, hops=1
  Main 5: gotoAndPlay("Intro", 3)
  Intro 3: root 3, hops=2
  Intro 4: nextScene(); play()
  Main 1 [main]: root 5
  Main 2: root 6, hops=2
  Main 4 [mainLoop]: root 8, hops=2
  Main 5: gotoAndPlay("finale")
  Outro 3 [finale]: root 12, hops=3
  Outro 4: prevScene(); play()
  Main 1 [main]: root 5
  Main 2: root 6, hops=4
  Main 4 [mainLoop]: root 8, hops=4
  Main 5: gotoAndPlay("Outro", "outro")
  Outro 1 [outro]: root 10
  Outro 3 [finale]: root 12, hops=4
  Outro 4: stop(), root 13
  ```
- **JSFL problems / workarounds:** `doc.renameScene(name)`, `doc.addNewScene(name)` (adds after the current
  scene and makes it current), `doc.editScene(i)` and `doc.timelines` work. J10 (2 placement attempts per
  scene), J12, J18 (the start-up language dialog blocked the first build and one trace capture; reruns
  passed). No step failed.
- **JPEXS:** no warnings beyond the routine ones. One `DOMTimeline` per scene, in order, named `Intro`,
  `Main` and `Outro`; labels are `name` + `labelType="name"`; scripts are preserved. `DOMDocument` has
  `currentTimeline="1"` and `nextSceneIdentifier="4"`.
- **SWF (how Flash 8 lays out scenes and compiles the gotos):**
  - The scenes are concatenated into the root timeline in order: `Intro` = root frames 1-4, `Main` = 5-9,
    `Outro` = 10-13. Nothing in the SWF names a scene: no FrameLabel for scene names and no
    DefineSceneAndFrameLabelData.
  - Frame labels are ordinary FrameLabel tags at the absolute frame: `intro` 1, `main` 5, `mainLoop` 8,
    `outro` 10, `finale` 12. `_currentframe` is the absolute root frame.
  - The gotos are resolved at compile time (GotoFrame takes a 0-based frame):

    | source, in scene | AVM1 |
    |---|---|
    | `nextScene()` in `Intro` | `GotoFrame 4` (the first frame of `Main`) |
    | `prevScene()` in `Outro` | `GotoFrame 4` (the first frame of `Main`) |
    | `gotoAndPlay(4)` in `Main` | `GotoFrame 7`, `Play`: a bare number is relative to the current scene |
    | `gotoAndPlay("Intro", 3)` in `Main` | `GotoFrame 2`, `Play` |
    | `gotoAndPlay("finale")` in `Main` | `GoToLabel 'finale'`, `Play` |
    | `gotoAndPlay("Outro", "outro")` in `Main` | `GoToLabel 'outro'`, `Play`: the scene name is dropped |
    | `stop()` | `Stop` |

  - `nextScene()` and `prevScene()` compile to a bare GotoFrame, which **stops** the playhead. The first
    build of this fixture called `nextScene()` without `play()`, and its Test Movie trace ended after
    `Main 1 [main]: root 5`. That is why the scripts add `play()`.
  - Because a scene-qualified goto with a label compiles to a plain GoToLabel, labels act as names on the
    whole root timeline; a label used in two scenes would resolve to one of them (not tested).
  - At a scene change the new scene's content takes the same depths: root frame 5 holds RemoveObject2 depth
    2 (the `Intro` dot), DoAction, FrameLabel `main`, DefineShape (the `Main` background), a PlaceObject2
    Modify with a new character at depth 1 (a replace) and a PlaceObject2 of the `Main` dot at depth 2 with
    `ratio=4`. Every scene uses depth 1 (art) and depth 2 (dot).
  - Order within frame 1: DoAction, FrameLabel, then the definitions and placements.

## Flash 8 JSFL problems found (groups, masks, scenes)

| # | API / situation | what happens in Flash 8 | workaround used |
|---|---|---|---|
| J18 | Starting Flash soon after another instance exited | Twice in this run Flash stopped at start-up on the modal "Flash 8 Language" dialog, although it was started in its own folder (J1). The script never started. | Rerun. `run-jsfl.ps1` (repository copy) now recognises the dialog before the script's log exists, kills Flash and relaunches after 10 s, up to 3 attempts, and every launcher waits 3 s after Flash exits before it releases the lock. The retry was added after both failures and has not been exercised yet. |
| J19 | Group members | `shape.members` is undefined in Flash 8, so group members and their matrices cannot be read back. `element.isGroup` was true for every shape on the groups fixture's `stack` layer, the raw shape and the drawing object included. `layer.frames[0].elements` lists the raw shape first. | Groups are made by locking every other layer, `selectAll()` and `group()`; the new group is found on the layer. The XFL and the SWF are the evidence for member matrices. |

## Repository tooling (tools/oracle)

The builders and tools listed at the top now also live in `tools/oracle/` of this repository (see its
`README.md`); `C:\dev\oracle` is unchanged. Differences:

- Builders get their paths from a prelude that `build-all.ps1` writes (`JZ_OUT`, `JZ_SRC`, `JZ_LOG`,
  `JZ_WORK`) and save straight into `tests/fixtures/NAME/` as `NAME.fla` and `flash8.swf`;
  `capture-trace.ps1` installs `flash8-trace.txt`. Logs, generated scripts and Test Movie copies go to
  `target/oracle/`.
- Every Flash launch takes the lock directory `target/flash8.lock` (one Flash at a time across agents).
- `build-all.ps1` needs `-Names` or `-All`, and `-OutRoot` builds into another folder.
- `swfdump.py --shapes-full` prints per-style edge bounds; GotoFrame2 shows its scene bias.
  `xfl-summary.py` prints group matrices, `parentLayerIndex` and per-style edge bounds.
  `verify-fixtures.py` reads the repository layout and checks that the Test Movie SWF equals `flash8.swf`.
- Check of the copy: gradients, bitmaps and smoke rebuilt with it into a scratch folder gave SWFs identical
  (after decompression) to the committed ones. groups, masks and scenes were built with it, and for each
  the Test Movie SWF equals `flash8.swf` after decompression.

---

## tween-probes

Small Flash 8 probes that pin down how the IDE bakes classic motion tweens, where the `tweens` and
`smoke` fixtures do not. They live in `tests/fixtures/tween-probes/NAME/` (`NAME.fla`, `flash8.swf`,
`xfl/`; no trace, the probes have no scripts) and are **not** picked up by the all-fixtures tests
(`tween-probes/` itself has no `xfl/`). `crates/jazari-publish/tests/tween_oracle.rs` compares our
output with each `flash8.swf` frame by frame. Built on 2026-10-28 with the same Flash 8 IDE and
JPEXS FLA Convert 3.0.0 as the other fixtures.

- **Builders:** `tests/fixtures/tween-probes/builders/NAME.jsfl`, run by
  `powershell -NoProfile -ExecutionPolicy Bypass -File tests\fixtures\tween-probes\builders\build.ps1 -Names NAME`
  (prelude + `tools/oracle/run-jsfl.ps1`, which takes the Flash lock; then JPEXS). Like the other builders
  they save, reopen and publish from the reopened FLA.

| probe | stage / fps / frames | covers |
|---|---|---|
| `matrices` | 800x600, 24, 121 | One `Arrow` clip per layer, one tween 0 → 120 between matrices set with `element.matrix`: rotation 0→6°, 42→48°, 84→90°, 0→90°, 0→−90°, CW x1 360°, 10→100°, 170→−170° (auto), scale x2 with 90°, skew x 40°, skew y 40°, mixed scales and skews, mirrored (−1,0,0,1) → 90°, "scale" off, anisotropic scale with 60°, CCW the long way, CW x2, rotate none, ease −40 on a 90° turn. |
| `tpoint` | 550x400, 24, 31 | Transformation point: `Arrow` (centred) and `Corner` (registration point at its top-left corner, so the transformation point is the centre (35, 10)) rotated 90°, scaled (2, 0.5), eased −100, CW x1 with 45° and x1.5, rotated about a corner; all with `rotateSelection` / `scaleSelection` / `moveSelectionBy`. |
| `ease` | 550x600, 24, 21 | Classic ease −100, −70, −30, 30, 70, 100 on a 400 px move; −50, 50, −100, 100 on a x3 scale (precise 16.16 data); ease 60 on a clip that turns 90°, doubles and fades to 20 %; custom ease curves: one segment overshooting (Flash clamps y to 1), two segments with a dip, a steep start, and a curve with a classic ease also set. |
| `color` | 600x400, 24, 21 | Colour effects: none → tint, tint → tint, brightness −50 → 70, alpha → tint, advanced → advanced, tint → none, alpha 100 → 7. Filters: drop shadow (all fields), glow, blur with quality low → high, bevel, drop shadow → drop shadow + blur, none → glow, glow → none, [blur, glow] → [glow, blur], Adjust Color 0 → (50, 30, −40, 60), drop shadow flags off → on. |
| `graphics` | 550x400, 12, 41 | The 10-frame `Counter` graphic (as in `tweens`) tweened 0 → 15: loop, loop with first frame 3, play once, single frame (6 → 2 at the end keyframe), loop → play once, sync on and off; two static layers with a second keyframe. |

**What Flash 8 does (all reproduced by `crates/jazari-publish/src/tween/`):**

- **Progress.** Step k of n is `round(k·65536/n)` in 16.16. The classic ease is
  `t + s·t − s·t²` (`s` = IDE ease / 100, −100 = ease in) in 16.16 integers: `s` and `t²` rounded to 16.16,
  `s·t` rounded, the sum rounded. A custom ease curve is evaluated at the exact `k/n` and truncated to
  16.16. One progress drives the matrix, the colour and the filters.
- **Rotation uses a quarter-degree sine table** with a quirk: between entries the value moves by
  `(T[i+1] − T[i]) · (x − i/4)` with the distance in degrees, so within each 0.25° cell the sine climbs at a
  quarter of the right rate and jumps at the next entry (0.05° → 14/65536, 0.25° → 286). `cos x = sin(90° − x)`.
  Each sine/cosine is rounded to 16.16, then multiplied by the interpolated scale and rounded again.
- **Decomposition:** `sx = |(a,b)|`, `sy = |(c,d)|`, rotation `atan2(b,a)`, skew angle `atan2(−c,d)`, scales
  always positive (a mirrored keyframe is 180° apart). The rotation turns by the rotate mode (auto = short way;
  CW/CCW = that direction plus whole turns); the skew angle turns by the same amount plus the short-way skew change.
  **Rotate "none" does not rotate at all**: the angle stays at the start keyframe's until the end keyframe.
  "Scale" off keeps the start scales.
- **The transformation point moves on a straight line** between its two keyframe positions; `tx, ty` are what
  puts the start keyframe's local transformation point there. With the point at the registration point this is a
  plain lerp of `tx, ty` (the `tweens` fixture); with `Corner` the old lerp was up to 82 px off.
- **Colour:** both keyframes are first rounded to SWF units (multipliers ×256, offsets); each of the eight numbers
  then moves linearly with halves rounded up. Tint, brightness, alpha and advanced all behave the same; a change of
  mode (alpha → tint) is just the same interpolation.
- **Filters:** lists are aligned by kind like a diff; filters only at one end fade from/to a neutral copy (blur,
  strength and distance 0, colour and flags kept). Strength moves in whole percent, truncated; colour channels
  (alpha included) move by a truncated delta; quality (passes) by a truncated delta (low → high: 1 until halfway,
  then 2); blur, distance and angle (degrees, then truncated to radians 16.16) are linear; flags keep the start
  values. Adjust Color interpolates its parameters (the matrix itself is not calibrated).
- **Graphic symbols:** in a tween the graphic keeps playing from the start keyframe's first frame; at the end
  keyframe the end instance's own loop mode and first frame apply. Inserting a keyframe after a graphic makes the
  IDE store the continued first frame (0 + 15 → 5), so "sync" changes nothing at publish time.

**Known residuals** (bounds asserted per layer in `tween_oracle.rs`):

- Keyframe angles that are not axis-aligned are stored as truncated 16.16 matrices; Flash 8's own decomposition of
  them differs from `atan2` by ~1e-4°, so an in-between angle that lands exactly on a 0.25° table entry can fall on
  the other side (one frame off by up to ~370/65536), and their scales can differ by 1–3/65536. Axis-aligned
  keyframes (0°, ±90°, 180°, full turns) are exact.
- Translations with the transformation point off the registration point are within 1–2 twips (3 in `matrices`,
  whose JSFL-set matrices left the point far off). The fixture's 2 → 0.5 ease-out scale is 1/65536 off on 8 frames;
  Flash 8 also moved the still `scale_ease` clip of `tweens` by 1 twip on frame 36 (18 pixels differ there).
- Per-property custom ease curves (`useSingleEaseCurve = false`) are not probed: Flash 8 hung on
  `setCustomEase("position", …)` with a single curve off. JPEXS writes the five per-property curves (linear) plus
  `all` for single-curve tweens; the reader uses `all`.

**JSFL problems found:**

| # | API / situation | what happens in Flash 8 | workaround used |
|---|---|---|---|
| J20 | `element.getTransformationPoint` / `setTransformationPoint` | Not defined in Flash 8 (CS3+). | A symbol whose registration point is a corner (`Corner`): `addItem` puts the transformation point at the bounds centre. |
| J21 | Setting `element.matrix` | The transformation point stays where it was **on the stage** (its local position changes), so a tween between such keyframes does not move. The matrix is stored truncated to 16.16 (cos 6° → 65176/65536). | `matrices` accepts it (rotation data only); `tpoint` and `ease` move with `moveSelectionBy`, `rotateSelection`, `scaleSelection`. |
| J22 | `doc.getFilters()` on an instance without filters | Flash hung (killed after the timeout). | Only call it on instances known to have filters. |
| J23 | `frame.setCustomEase("position", …)` with `useSingleEaseCurve = false` | Flash hung. | Per-property curves left out. |

**JPEXS:** routine conversion reports only. The classic ease sign is inverted for every strength (IDE −70 is written
`acceleration="70"`), as in `tweens`; custom curves, rotate modes, scale, sync, colour effects, filters,
transformation points and graphic loop settings are exported correctly.
