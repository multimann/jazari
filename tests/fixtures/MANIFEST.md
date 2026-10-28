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
