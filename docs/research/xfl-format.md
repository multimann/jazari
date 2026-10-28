# Research: xfl-format (verified)

## Recommendation

MILESTONE-1 XFL READ SPEC (CS5 to CS6, AS2/SWF8). Sources: xfl.xsd v2.1.0 (C:\dev\ref\xfl-schema) as the primary source; fla-viewer read only as a reference; JPEXS and xfl2svg sources on GitHub for the edge, .dat and number formats. Each item below is backed by an id in the findings list.

0. GENERAL PARSE POLICY
Match elements by local name and ignore the namespace (http://ns.adobe.com/xfl/2008/; it may be missing). Accept attributes and children in any order. On an unknown element, skip its subtree and record a warning; keep unknown attributes in a side map so files can be saved back without loss. Unknown enum values map to Unknown(String). Booleans are 'true'/'false'. Colours are #RRGGBB. Numbers are '.'-decimal. Apply schema defaults when an attribute is absent, except where §11 overrides them. Collect all warnings in an "import report" and never abort on Animate-era content (§10). If xflVersion > 2.2, warn that the file was saved by Flash CC or Animate.

1. CONTAINERS (F01-F03)
- ZIP .fla: magic 50 4B 03 04.
- Folder: contains DOMDocument.xml. A *.xfl stub of 64 bytes or less, whose text is 'PROXY-CS5', means "open the parent folder".
- Pre-CS5 binary .fla: magic D0 CF 11 E0 A1 B1 1A E1. Refuse it with the message "re-save as CS5 or later".
- Tree: DOMDocument.xml, PublishSettings.xml, MobileSettings.xml (ignore), META-INF/metadata.xml (XMP, ignore), LIBRARY/**.xml (one DOMSymbolItem per file), bin/*.dat (media), bin/SymDepend.cache (ignore).
- Lookup: normalise '\' to '/'. Try exact, then case-insensitive, then basename matching. Index every LIBRARY/**/*.xml and key symbols by the DOMSymbolItem@name inside the file (the Include href is only a hint; Arabic names may be mangled in ZIP entry names).

2. DOMDocument (F04-F05)
<DOMDocument xmlns="http://ns.adobe.com/xfl/2008/" width="550" height="400" frameRate="24" backgroundColor="#FFFFFF" currentTimeline="1" xflVersion="2.2">
  <folders><DOMFolderItem name="art" isExpanded="true"/></folders>
  <fonts>…DOMFontItem…</fonts>
  <media>…DOMBitmapItem / DOMSoundItem / DOMVideoItem…</media>
  <symbols><Include href="art/Ball.xml" itemIcon="1" loadImmediate="false" itemID="…" lastModified="…"/></symbols>
  <timelines><DOMTimeline name="Scene 1">…</DOMTimeline></timelines>
</DOMDocument>
Defaults: width 550 and height 400 (px, float); frameRate 24 (fps, float; SWF needs it as 8.8 fixed point); backgroundColor #FFFFFF. Each DOMTimeline is a scene, in order. currentTimeline (1-based) is editor state. xflVersion: 2.0 = CS5, 2.1 = CS5.5, 2.2 = CS6.

3. TIMELINE / LAYER / FRAME (F06-F10)
<DOMTimeline name="Scene 1"><layers>
  <DOMLayer name="mask" color="#4FFF4F" layerType="mask" locked="true">…</DOMLayer>
  <DOMLayer name="art" color="#9933CC" parentLayerIndex="0">   <!-- masked by layer 0 -->
    <frames>
      <DOMFrame index="0" duration="10" tweenType="motion" motionTweenSnap="true" keyMode="22017" acceleration="-50" name="intro" labelType="name">
        <Actionscript><script><![CDATA[stop();]]></script></Actionscript>
        <elements>…</elements>
      </DOMFrame>
      <DOMFrame index="10" duration="5" keyMode="9728"><elements/></DOMFrame>   <!-- blank keyframe -->
    </frames></DOMLayer>
</layers></DOMTimeline>
- DOMLayer: name and color required. layerType is normal | folder | mask | guide (default normal). Membership comes from parentLayerIndex, a 0-based index into the same <layers> list: a parent that is a mask makes the layer masked, a guide makes it guided (a motion guide), a folder makes it a folder child. Walk the chain through folders to find the effective mask or guide. visible defaults to true; locked, outline, open, autoNamed, current, isSelected and heightMultiplier are editor state. Hidden layers are still published when PublishSettings InvisibleLayer=1. Guide-layer content is never published.
- Drawing order: the first DOMLayer is the TOP, so render layers last-to-first. Elements within a frame and members of a group are drawn back to front in document order.
- DOMFrame: index (0-based, required) and duration (default 1) form the keyframe span [index, index+duration). An empty <elements/> is a blank keyframe. Tolerate gaps between spans (show nothing).
  tweenType: none | motion | shape | 'motion object' | 'IK pose'.
  keyMode: redundant bitmask (9728 = none; 0x4001 | 0x0100 orient | 0x0200 scale | 0x0400 rotate | 0x0800 sync | 0x1000 snap; 17922 = shape). Read it only as a fallback, but write it when saving.
  Classic-tween attributes: acceleration (-100..100; sign disputed, see the open questions), motionTweenRotate = none | auto | clockwise | counter-clockwise plus motionTweenRotateTimes, motionTweenScale (absent: keyMode & 0x0200), motionTweenOrientToPath, motionTweenSync, motionTweenSnap, hasCustomEase, useSingleEaseCurve (default true), and <tweens><CustomEase target="all|position|rotation|scale|color|filters"><Point x y/>…</CustomEase></tweens> (points in 0..1, a 3n+1 piecewise cubic).
  Labels: name plus labelType = name | comment | anchor (a comment is not exported).
  Sound (note only): soundName (= DOMSoundItem@name), soundSync = event | start | stop | stream, soundLoop, soundLoopMode = repeat | loop, inPoint44 and outPoint44 (44.1 kHz samples), <SoundEnvelope>.
- Scripts: <Actionscript><script>CDATA</script></Actionscript> on DOMFrame (frame actions) and on DOMSymbolInstance (on(...) for buttons, onClipEvent(...) for movie clips). Join the text and CDATA nodes, and normalise CR and CRLF to LF before handing the code to Rascal.
- Classic tweens will be baked at publish time into per-frame PlaceObject matrices and colour transforms. Interpolate decomposed scale, skew/rotation and translation, not raw a..d values (fla-viewer AGENTS.md:112-125).

4. SHAPES (F11-F19)
<DOMShape isDrawingObject="false">
  <fills><FillStyle index="1"><SolidColor color="#FF6C00" alpha="1"/></FillStyle>
         <FillStyle index="2"><RadialGradient focalPointRatio="0.3" spreadMethod="reflect" interpolationMethod="linearRGB">
           <matrix><Matrix a="0.05" d="0.05" tx="50" ty="25"/></matrix>
           <GradientEntry color="#FFFFFF" alpha="1" ratio="0"/><GradientEntry color="#0033FF" ratio="1"/></RadialGradient></FillStyle>
         <FillStyle index="3"><BitmapFill bitmapPath="art/photo.jpg"><matrix><Matrix a="20" d="20" tx="-27.5" ty="-27.5"/></matrix></BitmapFill></FillStyle></fills>
  <strokes><StrokeStyle index="1"><SolidStroke weight="2" caps="none" joints="miter" miterLimit="4" scaleMode="normal" pixelHinting="true"><fill><SolidColor color="#000000"/></fill></SolidStroke></StrokeStyle></strokes>
  <edges><Edge fillStyle1="1" strokeStyle="1" edges="!0 0S1|2000 0|2000 #3E8.80[1000 #7D0.40 0 1000|0 0"/></edges>
</DOMShape>
- Matrix: defaults a=1, b=0, c=0, d=1, tx=0, ty=0. a..d are unitless; tx/ty are PIXELS (SWF tx = round(tx*20)). transformationPoint/Point x,y are local pixels (default 0).
- EDGES grammar: moveto (moveto | lineto | quadto)*.
  '!' x y = moveTo; '|' x y or '/' x y = lineTo; '[' cx cy x y or ']' cx cy x y = quadratic curve.
  'S' followed by a digit (1-7) is an editor selection hint: skip it. A bare '/' with no numbers: tolerate as close-path.
  Coordinates are absolute TWIPS. A number is either decimal ('-12.5') or '#' hex 24.8 fixed point: left-pad the integer part to 6 hex digits and right-pad the fraction to 2, read as int32, divide by 256. Hex digits are uppercase in practice; accept lowercase too.
- WORKED EXAMPLE, edges="!0 0S1|2000 0|2000 #3E8.80[1000 #7D0.40 0 1000|0 0" (twips; px = /20):
  !0 0 → moveTo (0,0) px. S1 → ignored.
  |2000 0 → lineTo (100,0) px.
  |2000 #3E8.80 → '0003E8'+'80' = 0x0003E880 = 256128, /256 = 1000.5 tw → lineTo (100, 50.025) px.
  [1000 #7D0.40 0 1000 → control (1000, 0x0007D040/256 = 2000.25 tw) = (50, 100.0125) px; end (0, 1000) tw = (0, 50) px.
  |0 0 → lineTo (0,0).
  The path runs clockwise on a y-down screen, so the interior is on the RIGHT of travel: fillStyle1. A negative example: '#FFFF38.40' = -200 + 0.25 = -199.75 tw (not -200.25).
- Styles on an Edge: fillStyle0, fillStyle1 and strokeStyle are 1-based indices into FillStyle@index and StrokeStyle@index (0 = none) and equal SWF FillStyle0, FillStyle1 and LineStyle exactly. Wording: fillStyle0 = left, fillStyle1 = right of the direction of travel in y-down space. Operationally: for own tessellation, use fillStyle1 segments as written and reverse fillStyle0 segments, then stitch by endpoints across ALL Edge elements of the shape. An Edge string may contain several moveTo subpaths; every one of them shares the Edge's styles. For SWF, emit per Edge one StyleChange (styles plus moveTo), then StraightEdge/CurvedEdge deltas computed from ROUNDED absolute twips. No stitching is needed; Flash Player and Ruffle stitch. Skip Edges with no fill and no stroke.
- cubics: an editor-only cubic copy of the curve. Ignore it for display and publish, keep it for saving. If an Edge has cubics but no edges, approximate it with quadratics.
- Fills:
  SolidColor: color #000000, alpha 1.
  Linear/RadialGradient: a required matrix that maps the gradient square ±819.2 px (±16384 twips) to shape px. a..d are the same numbers as in SWF (Fixed16); tx/ty are px (SWF ×20). GradientEntry ratio is in 0..1 (SWF u8 = round(ratio*255)); at most 15 entries for SWF8. spreadMethod pad | reflect | repeat; interpolationMethod rgb | linearRGB; focalPointRatio -1..1 (non-zero → SWF FocalGradient).
  BitmapFill: bitmapPath = DOMBitmapItem@name. Treat bitmapIsClipped absent as FALSE (repeat). Smoothing = DOMBitmapItem@allowSmoothing. Matrix a=d=20 means 1:1; tx/ty units are uncertain (probably px).
  ClippedBitmapFill: a variant element seen in files; read it like BitmapFill with clipped=true.
- Strokes: SolidStroke weight (px, default 1; hairline is solidStyle='hairline'), caps none | round | square (default round), joints miter | round | bevel (default round), miterLimit (default 3), scaleMode normal | horizontal | vertical | none (horizontal = SWF NO_V_SCALE, vertical = NO_H_SCALE, none = both), pixelHinting (default false), and a required inner <fill> (non-solid → SWF8 LineStyle2 HAS_FILL). Dashed, Dotted, Ragged, Stipple and Hatched strokes: publish as solid and warn. VariablePointWidth: ignore (constant weight) and warn.
- DOMGroup: matrix, transformationPoint and <members> (recursive). Use fla-viewer's rule: a member with its own matrix keeps it (parent space); a member without one inherits the group matrix (unverified, test).
- DOMRectangleObject / DOMOvalObject (CS4+, can appear in CS5/CS6 files): rebuild the outline from objectWidth, objectHeight, x, y and the corner radii (or startAngle, endAngle, innerRadius, closePath), with a singular <fill> and <stroke>.
- MorphShape (shape tween): MorphSegment and MorphCurves points are 'x, y' strings in edge-number twips. Milestone 1 may publish shape tweens as baked per-frame shapes, or defer them and warn.

5. SYMBOL INSTANCES (F20-F22)
<DOMSymbolInstance libraryItemName="ui/PlayBtn" symbolType="button" name="play_btn" trackAsMenu="false" blendMode="screen" cacheAsBitmap="true">
  <matrix><Matrix a="1.5" d="1.5" tx="275" ty="200"/></matrix>
  <transformationPoint><Point x="40" y="15"/></transformationPoint>
  <Actionscript><script><![CDATA[on (release) { gotoAndPlay("intro"); }]]></script></Actionscript>
  <filters><DropShadowFilter blurX="4" blurY="4" distance="3" strength="0.6" color="#000000"/><GlowFilter color="#FFCC00" isEnabled="false"/></filters>
  <color><Color alphaMultiplier="0.5" redOffset="20"/></color>
</DOMSymbolInstance>
- symbolType: an absent value means movie clip; graphic and button are written explicitly.
- name is the instance name (movie clips and buttons).
- Graphics: loop = loop | 'play once' | 'single frame' (default loop) and firstFrame (0-based, default 0). Tolerate lastFrame and the 'reverse' loop modes (Animate).
- Buttons: trackAsMenu. Button symbol frames 0-3 are Up, Over, Down and Hit.
- blendMode (14 values) maps to SWF8 BlendMode: normal 0, layer 2, multiply 3, screen 4, lighten 5, darken 6, difference 7, add 8, subtract 9, invert 10, alpha 11, erase 12, overlay 13, hardlight 14. cacheAsBitmap maps to PlaceObject3. isVisible=false (CS5.5): not expressible in SWF8 PlaceObject; emulate with _visible=false or warn. centerPoint3D*, rotationX/Y/Z and matrix3D: ignore (2D matrix only) and warn when non-zero.
- Color: multipliers are floats (default 1), offsets are integers (-255..255, default 0). If only the brightness or tint shorthand is present, derive the transform: brightness b ≥ 0 → mult 1-b, offset 255b; b < 0 → mult 1+b. Tint t with colour C → mult 1-t, offset C·t. Prefer explicit multiplier and offset attributes when they are present.
- Filters: DropShadow, Blur, Glow, Bevel, GradientGlow and GradientBevel (with GradientEntry children), AdjustColor (→ ColorMatrix), ColorMatrixFilter (matrix = 20 comma-separated values) and ConvolutionFilter. Defaults: blur 5 px, quality 1, angle 45°, distance 5 px, strength 1.0 (a ratio; SWF stores Fixed8 strength), knockout, inner and hideObject false, isEnabled true (a disabled filter is kept but not published). Default Glow colour is disputed: use #FF0000 (fla-viewer, which says it matches flacomdoc) until a sample decides. Bevel: shadowColor #000000, highlightColor #FFFFFF, type inner | outer | full.

6. BITMAPS (F23-F26)
<DOMBitmapItem name="art/photo.jpg" itemID="…" href="art/photo.jpg" bitmapDataHRef="M 2 1615711026.dat" frameRight="11000" frameBottom="8000" quality="80" compressionType="photo" originalCompressionType="photo" isJPEG="true" allowSmoothing="true" useImportedJPEGData="true" sourceExternalFilepath="./LIBRARY/photo.jpg" sourceLastImported="…"/>
<DOMBitmapInstance libraryItemName="art/photo.jpg" name=""><matrix><Matrix tx="10" ty="20"/></matrix></DOMBitmapInstance>
- Pixel size = frameRight/20 × frameBottom/20 (twips). Bytes: bin/<bitmapDataHRef>, falling back to LIBRARY/<href> (folder saves). Sniff the format by magic bytes:
  03 05 = 32-bit lossless; 03 03 = 8-bit palette; FF D8 FF = JPEG (plus a 16-byte bounds trailer); 89 50 4E 47 = PNG; GIF8 = GIF.
- 03 05 layout (little-endian): u16 rowBytes (4w) | u16 w | u16 h | 4×u32 bounds in twips (0, 20w, 0, 20h) | u8 hasAlpha | u8 compressed (1 = chunked zlib, 0 = raw) | then [u16 len ≤ 2048][len bytes] … u16 0. Concatenate the chunks to get one zlib stream; it inflates to w·h·4 bytes of A,R,G,B premultiplied. Unpremultiply with alpha-1 for 0 < a < 255. If inflate fails, retry with a 32 KiB zero preset dictionary. Ignore bytes past the expected pixel count.
- 03 03 layout: same header to byte 24; byte 25 = palette count (0 means 256); 2 bytes of unknown purpose; count × RGBA entries (alpha last, straight); then chunked-deflate indices with rows padded to rowBytes.
- Publish: JPEG → DefineBitsJPEG2 (strip the trailer). Lossless → DefineBitsLossless2 format 5 (the premultiplied ARGB plane can be re-zlibbed as is). Honour compressionType and quality when recompression is wanted.

7. TEXT (F27-F29)
<DOMStaticText fontRenderingMode="standard" left="-60" width="120" height="22" autoExpand="true" isSelectable="false" orientation="horizontal">
  <matrix><Matrix tx="200" ty="150"/></matrix>
  <textRuns><DOMTextRun><characters>مرحبا بكم&#xD;</characters>
    <textAttrs><DOMTextAttrs face="Tahoma" size="16" bitmapSize="320" fillColor="#333333" alpha="1" alignment="right" lineSpacing="2" letterSpacing="0" autoKern="true" aliasText="false" indent="0" leftMargin="0" rightMargin="0"/></textAttrs>
  </DOMTextRun></textRuns>
</DOMStaticText>
<DOMDynamicText name="score_txt" variableName="_root.score" lineType="multiline" renderAsHTML="false" border="true" left="0" width="100" height="20">…</DOMDynamicText>
<DOMInputText name="name_txt" lineType="single line" maxCharacters="20" …>…</DOMInputText>
- Geometry: left, top, width and height in px (default 0), in the text's local space under <matrix>. autoExpand=true means the box auto-sizes. fontRenderingMode is device | bitmap | standard | advanced | customThicknessSharpness; there is no attribute named 'renderingMode'.
- Runs: <characters> holds logical-order Unicode. CR (&#xD;) separates paragraphs, and a run usually ends with one. Paragraph attributes (alignment, indent, margins, lineSpacing) are repeated on every run.
- Units: size in pt (= px at 1:1; parse as f32 even though the xsd says integer), bitmapSize in twips, lineSpacing (leading) and letterSpacing in pt, url and target for links, characterPosition normal | superscript | subscript.
- RTL: classic text has NO direction attribute; 'vertical right to left' is a vertical writing mode, not bidi. Infer the base direction (first strong character, overridable) and store our choice in <persistentData><PD n="jazari.direction" t="s" v="rtl"/></persistentData>, a schema-sanctioned extension point that Flash preserves. Shape Arabic ourselves at publish time.
- DOMTLFText (CS5-CS6 TLF; has direction='rtl' in its TextFlow markup): TLF needs AS3 and FP10, so it is invalid for an AS2/SWF8 target. Import it as static text from its spans and warn.

8. LIBRARY (F30-F31)
LIBRARY/art/Ball.xml:
<DOMSymbolItem xmlns="http://ns.adobe.com/xfl/2008/" name="art/Ball" itemID="…" symbolType="graphic" linkageExportForAS="true" linkageIdentifier="ball_id" linkageExportInFirstFrame="true" lastModified="…">
  <timeline><DOMTimeline name="Ball"><layers>…</layers></DOMTimeline></timeline>
</DOMSymbolItem>
- symbolType: absent = movie clip; graphic; button; 'ik container' (treat as graphic).
- Linkage: linkageExportForAS, linkageIdentifier (AS2 attachMovie → SWF ExportAssets), linkageClassName (AS2 class → registerClass in DoInitAction), linkageExportInFirstFrame (default true), and the runtime-sharing flags (ignore). 9-slice: scaleGridLeft/Right/Top/Bottom (xsd) or scalingGrid plus scalingGridRect in twips (fla-viewer); read both and map to SWF DefineScalingGrid.
- DOMFolderItem: name (with '/'), isExpanded.
- DOMFontItem: name, font, size, id, embedRanges ('1|2|3'), embeddedCharacters, linkage. Treat it as a request to embed those glyphs.
- DOMSoundItem (note only): soundDataHRef in bin/, format (playback description, not the codec), sampleCount, exportFormat, exportBits, dataLength (trim MP3 to this length; sniff FF Ex or ID3).
- DOMVideoItem, DOMCompiledClipItem, DOMComponentItem: keep and warn.

9. PublishSettings.xml (F32)
<flash_profiles><flash_profile version="1.0" name="Default" current="true"><PublishFlashProperties enabled="true"><Version>8</Version><ActionScriptVersion>2</ActionScriptVersion><CompressMovie>1</CompressMovie><Quality>80</Quality><InvisibleLayer>1</InvisibleLayer><OmitTraceActions>0</OmitTraceActions>…</PublishFlashProperties></flash_profile></flash_profiles>
The tags are taken from the schema; the values in this example are typical but unverified. All values are strings and may be empty, so parse the leading integer. If the file is missing or a value is empty, use Version 8, ActionScriptVersion 2 and CompressMovie 1 (CWS). For SWF8, ignore CompressionType=LZMA, because ZWS requires FP11. Frame rate always comes from DOMDocument@frameRate.

10. ANIMATE-ERA CONTENT TO TOLERATE (F33)
Ignore these and warn, without failing:
- layerType='camera' and the __Camera__ symbol (do not draw that layer); cameraLayerEnabled, layerDepthEnabled, attachedToCamera, frameZDepth, transparent, alphaPercent.
- Layer parenting (normal → normal parentLayerIndex): treat the child as a top-level normal layer; its stored matrices are already world-space.
- Object motion tweens ('motion object', <motionObjectXML>), CS4+ and possibly present in CS5/CS6 files: show the first pose, warn, and plan baking.
- IK (animationType/tweenType 'IK pose', IKTree, betweenFrameMatrixList); 3D (rotationX/Y/Z, matrix3D, centerPoint3DZ, viewAngle3D); <Ease method>; lastFrame and the reverse loops; VariablePointWidth.
- DOMTLFText, components and compiled clips, video.
- Document-level extras: swatchLists, PrinterSettings, publishHistory, swcCache, scripts, persistentData, GUIDs.
- 'Advanced Layers' has no documented attribute; it falls under the ignore-unknown rule.

11. DEFAULT OVERRIDES CHOSEN FOR NOW (revisit with a real CS5/CS6 corpus)
- bitmapIsClipped absent = false.
- Stroke scaleMode absent = normal.
- Glow colour absent = #FF0000.
- motionTweenRotate absent = auto when keyMode & 0x0400, else none.
- acceleration sign follows fla-viewer (negative = ease in) pending a test.

NEXT STEP: gather 10 to 20 real CS5 and CS6 saves (shapes with hex coordinates, masks, guides, button on() handlers, onClipEvent, dynamic and input text with Arabic, JPEG and PNG bitmaps, classic tweens with ease ±100 and a custom ease) and turn the open questions into golden tests before the parser is frozen.

## Dependencies

- quick-xml 0.37.5: XML reader; same version as ruffle_core, so no duplicate crate (C:\dev\ref\ruffle\Cargo.toml:90)
- flate2 1.1.9: zlib inflate for bin/*.dat and deflate for SWF/DefineBitsLossless2; matches Ruffle (C:\dev\ref\ruffle\Cargo.toml:79)
- zip 9.0.5 with default-features=false and features=['deflate']: reads .fla archives; matches Ruffle's workspace pin (C:\dev\ref\ruffle\Cargo.toml:57; frontend-utils/Cargo.toml:35)
- swf (ruffle git rev 07d3e430631333f80f754df01a968bdcd80d98c5): ShapeRecord, FillStyle, LineStyle, Filter and BlendMode are the targets of the XFL mapping
- A JPEG decoder that ignores trailing bytes after EOI, only for in-app thumbnails; SWF output passes JPEG bytes through
- No cfb crate needed: detecting the OLE2 magic is enough to refuse pre-CS5 files

## Findings (verifier status in brackets; corrections override the claim)

- **F01** [confirmed] Telling the containers apart. A CS5+ .fla is a ZIP that starts with 50 4B 03 04. An uncompressed XFL is a folder with DOMDocument.xml at its root plus a stub '<name>.xfl' whose whole content is the ASCII text 'PROXY-CS5'; the rest of the tree is the same as inside the ZIP. A pre-CS5 binary .fla is an OLE2/MS-CFB file that starts with D0 CF 11 E0 A1 B1 1A E1.
  - evidence: C:\dev\ref\fla-viewer\src\xfl-folder.ts:3-9,17 (XFL_STUB_CONTENT = 'PROXY-CS5'),50-53 (stub check: at most 64 bytes, trimmed text equals PROXY-CS5); C:\dev\ref\fla-viewer\AGENTS.md:7-22 (archive layout),975-976,1058-1060 (OLE2 magic); C:\dev\ref\fla-viewer\src\ole2-reader.ts:21,52-58; C:\dev\ref\fla-viewer\src\fla-parser.ts:233-256 (sniff order: OLE2, then stub, then ZIP). I read the magic bytes of the local fixtures myself: easing-tokens.fla = 50 4b 03 04; binary-*.fla and btnstrob.fla = d0 cf 11 e0 a1 b1 1a e1.
- **F02** [confirmed] Archive contents: DOMDocument.xml, PublishSettings.xml, MobileSettings.xml, META-INF/metadata.xml (XMP), LIBRARY/*.xml (one DOMSymbolItem per file, with subfolders that mirror library folders), and bin/ (M*.dat media blobs plus SymDepend.cache). Only DOMDocument.xml, LIBRARY/*.xml, bin/*.dat and PublishSettings.xml are needed for milestone 1.
  - evidence: C:\dev\ref\fla-viewer\AGENTS.md:7-22; C:\dev\ref\xfl-schema\xfl.xsd:3479-3482 (bitmapDataHRef example 'M 2 1615711026.dat' inside bin)
- **F03** [partly-wrong] Make lookup inside the ZIP tolerant. Entry names may use backslashes. The Include href of a library item may fail to match its stored file name, because names with non-ASCII characters suffer encoding problems. fla-viewer therefore also loads every LIBRARY/*.xml it finds and keys each symbol by the DOMSymbolItem@name inside the file. Damaged ZIPs are recovered by searching for the EOCD record.
  - evidence: C:\dev\ref\fla-viewer\src\path-utils.ts:1-12; C:\dev\ref\fla-viewer\src\fla-parser.ts:466-490 ('Also scan all XML files in LIBRARY folder directly (handles encoding issues)'),526-527,317-330 (EOCD 0x06054b50 search),415-452 (path variants and basename fallback)
  - CORRECTION: The substance is right: path-utils.ts:1-12, the LIBRARY scan at fla-parser.ts:466-490 and the path variants plus basename fallback at :413-452. The EOCD line numbers are wrong: the search for signature 0x06054b50 is at C:\dev\ref\fla-viewer\src\fla-parser.ts:343-379 (tryRepairZip starts at 332), not 317-330.
- **F04** [partly-wrong] DOMDocument root. Namespace http://ns.adobe.com/xfl/2008/. Attributes and defaults: width=550 and height=400 (float, pixels), frameRate=24 (float, fps), backgroundColor=#ffffff (#RRGGBB), currentTimeline=1 (1-based scene index, editor state only), xflVersion required (2.0 = CS5, 2.1 = CS5.5, 2.2 = CS6, 2.4 = Flash CC, 2.6 and later = Animate). Children in schema order: folders, fonts, media, symbols (Include), timelines (one or more DOMTimeline, each one a scene), then swatchLists, persistentData, PrinterSettings and others, which can be ignored.
  - evidence: C:\dev\ref\xfl-schema\xfl.xsd:3-5,3352-3782,3917-3940,3942-3966 (xflVersion table),4015 (frameRate default 24),195-204 (ColorType pattern)
  - CORRECTION: Defaults are confirmed at xfl.xsd:3917 (width 550.0), 3922 (height 400.0), 3927 (backgroundColor #ffffff), 4015 (frameRate 24) and 3942 (xflVersion required). Three corrections. (1) currentTimeline is documented only as 'Identifier for the current timeline' (xsd:3937-3940); '1-based scene index' is an interpretation. (2) xflVersion is xs:string, and Animate 2022-2024 write '22.0', '23.0' and '24.0' (xsd:3961-3963), so it must be parsed numerically, never compared as text. (3) The schema puts DOMCompiledClipItem inside <media> (xsd:3442 onward, DOMCompiledClipItem near 3676), and the child list also includes extendedSwatchLists, scripts, publishHistory and swcCache (xsd:3790-3900).
- **F05** [confirmed] Include@href is a path relative to LIBRARY/, for example 'Symbol 1.xml'. The library item's real name is the @name attribute inside that file; library folder names use '/'. Include also carries itemIcon, loadImmediate, itemID and lastModified, all editor metadata.
  - evidence: C:\dev\ref\xfl-schema\xfl.xsd:3741-3769,3360-3363,4268-4272; C:\dev\ref\fla-viewer\src\fla-parser.ts:472
- **F06** [confirmed] DOMLayer attributes: name and color are required. Defaults: layerType=normal, visible=true, locked=false, outline=false, open=true, autoNamed=true, current=false, isSelected=false, animationType=none, heightMultiplier=1. In the schema, layerType is only normal | folder | mask | guide. 'Masked' and 'guided' are not written as layer types: such a layer is a normal layer whose parentLayerIndex (0-based index into the same timeline's layers list) points at a mask or guide layer. Folder membership uses the same parentLayerIndex, so the chain must be walked through folders to find the mask.
  - evidence: C:\dev\ref\xfl-schema\xfl.xsd:3155-3250 (layerType enum 3185-3197; parentLayerIndex 3198-3202: 'sublayer (in folders, masks or guides)'); C:\dev\ref\fla-viewer\src\layer-utils.ts:29-75 (getMaskLayerIndex walks parentLayerIndex through folders); C:\dev\ref\fla-viewer\src\fla-parser.ts:828-838 (masked is derived, never read)
- **F07** [confirmed] Drawing order: the first DOMLayer is the top layer, so layers are drawn in reverse document order. Within one frame, elements (and DOMGroup members) are drawn in document order, the first element being the backmost.
  - evidence: C:\dev\ref\fla-viewer\src\renderer.ts:98 (layerOrder default 'reverse'),100 (elementOrder 'forward'),1041-1042
- **F08** [partly-wrong] DOMFrame. index (0-based, required) and duration (default 1) together define a keyframe span. keyMode is a required integer bitmask that duplicates other attributes: 9728 (0x2600) means no tween; a classic tween is 0x4001 plus flags 0x0100 orientToPath, 0x0200 scale, 0x0400 rotate other than none, 0x0800 sync, 0x1000 snap; 17922 (0x4602) is a shape tween. tweenType is none | motion | shape | motion object | IK pose (default none). acceleration is an integer from -100 to 100 (default 0). motionTweenRotate is none | auto | clockwise | counter-clockwise (schema default none), with motionTweenRotateTimes. motionTweenScale (absent: use keyMode bit 0x0200), motionTweenSnap, motionTweenSync and motionTweenOrientToPath are booleans. useSingleEaseCurve defaults to true and hasCustomEase to false. A label is written as name plus labelType = none | name | comment | anchor. Sound attributes: soundName, soundSync = event | start | stop | stream, soundLoop, soundLoopMode = repeat | loop, inPoint44 and outPoint44 (counted in 44.1 kHz samples). Child elements in schema order: Actionscript, MorphShape, SoundEnvelope, tweens, motionObjectXML, elements, betweenFrameMatrixList, IKTree.
  - evidence: C:\dev\ref\xfl-schema\xfl.xsd:2555-2563,2714-2781,2884-3121 (keyMode table 2897-2940; acceleration 2967-2982; rotate 3003-3020; sound 3031-3096; label 3097-3115); C:\dev\ref\fla-viewer\AGENTS.md:931-932 (rotate values are spelled out)
  - CORRECTION: The schema facts are confirmed (xfl.xsd:2884-3121). However, the xsd note says the default classic tween is 'only motionTweenSnap = true' (0x5001 = 20481), not 22017, and keyMode for 'motion object' is 0x2003 + 0x0800, which the report omits. In real-world converted files keyMode also takes values outside the table: C:\dev\samples\f8\smoke-xfl\DOMDocument.xml and C:\dev\samples\fixtures\shapes-basic\xfl\DOMDocument.xml use keyMode="1536" (0x0600) together with motionTweenRotate="auto" on untweened frames. Never use keyMode as a fallback for tween semantics; trust tweenType.
- **F09** [confirmed] The sign of acceleration is disputed. The schema says '-100 here = 100 out in IDE, 100 here = -100 in in IDE'. fla-viewer says 'negative = ease-in, positive = ease-out'. CustomEase holds Points whose x and y lie in 0..1, with target = position | rotation | scale | color | filters | all; fla-viewer treats it as a piecewise cubic Bezier of 3n+1 points.
  - evidence: C:\dev\ref\xfl-schema\xfl.xsd:2967-2975,2720-2776; C:\dev\ref\fla-viewer\AGENTS.md:80-110
- **F10** [confirmed] Frame scripts sit in DOMFrame/Actionscript/script as text, normally CDATA. Instances carry the same Actionscript element: button on() handlers on button instances, onClipEvent() handlers on movie clip instances. ActionscriptType also has optional breakpoints, offsets and href attributes, which can be ignored. Join all text and CDATA nodes of <script> together, and normalise CR and CRLF to LF before compiling.
  - evidence: C:\dev\ref\xfl-schema\xfl.xsd:2558-2563,4398-4403,4659-4663; C:\dev\ref\fla-viewer\src\fla-parser.ts:892
- **F11** [confirmed] Grammar of the Edge@edges attribute. The string is moveto followed by (moveto | lineto | quadto)*. '!' x y is moveTo. '|' x y and '/' x y are lineTo. '[' cx cy x y and ']' cx cy x y are quadTo (control point, then end point). 'S' followed by a digit 1-7 is an editor-only selection hint that may follow a moveto and must be skipped. All coordinates are absolute and in twips (1/20 px). fla-viewer additionally reads a bare '/' with no numbers as close-path, which should be tolerated.
  - evidence: https://raw.githubusercontent.com/PluieElectrique/xfl2svg/master/xfl2svg/shape/edge.py (grammar comment: start/moveto/lineto ('|'|'/')/quadto ('['|']')/select S[1-7]; parse_number divides by 20); C:\dev\ref\fla-viewer\AGENTS.md:322-330; C:\dev\ref\fla-viewer\src\edge-decoder.ts:521-545 ('/x y' is a lineTo; a bare '/' closes the path),502-519 (S token)
- **F12** [confirmed] Number format in edges. A decimal number such as '-12.5' is in twips. A hex number '#III.FF' is a signed 32-bit 24.8 fixed-point value: left-pad the integer digits with '0' to 6 hex digits, right-pad the fraction digits with '0' to 2, read the 8 digits as a big-endian two's-complement int32, then divide by 256 to get twips. Example: '#FFFF38.40' = -200 + 0.25 = -199.75 twips, not -200.25. JPEXS writes whole values as decimal integers and fractional values as '#' + the last 6 uppercase hex digits of floor(v) + '.' + %02X of floor(frac*256). fla-viewer gets negative hex values wrong: it subtracts the fraction.
  - evidence: https://raw.githubusercontent.com/PluieElectrique/xfl2svg/master/xfl2svg/shape/edge.py (parse_number: '{:>06}{:<02}', int.from_bytes(..., signed=True), /256, /20); https://raw.githubusercontent.com/jindrapetrik/jpexs-decompiler/master/libsrc/ffdec_lib/src/com/jpexs/decompiler/flash/xfl/XFLConverter.java (numEdgeToString: (long) Math.floor(value), Long.toHexString, last 6 digits, %02X); C:\dev\ref\fla-viewer\src\edge-decoder.ts:71-94 (intPart - fracPart for negatives)
- **F13** [confirmed] Edge style indices. fillStyle0, fillStyle1 and strokeStyle are 1-based indices into the shape's FillStyle@index and StrokeStyle@index lists; 0 or an absent attribute means none. They correspond one-to-one to SWF FillStyle0, FillStyle1 and LineStyle: JPEXS writes the SWF values unchanged for ordinary shapes, swapping them only for morph shapes. The schema and xfl2svg call fillStyle0 the LEFT side and fillStyle1 the RIGHT side of the direction of travel (in Flash's y-down space). fla-viewer's AGENTS.md uses the opposite wording, but every source agrees on what to do: when stitching contours, use fillStyle1 segments as written and reverse fillStyle0 segments (Ruffle flips fill_style_0). When publishing to SWF, copy the indices through unchanged and let the player stitch.
  - evidence: C:\dev\ref\xfl-schema\xfl.xsd:2119-2133; https://raw.githubusercontent.com/PluieElectrique/xfl2svg/master/xfl2svg/shape/edge.py ('fillStyle0' (left) and 'fillStyle1' (right)); C:\dev\ref\ruffle\render\src\shape_utils.rs:355-363,455-475 (fill_style_1 flush flip=false, fill_style_0 flip=true); XFLConverter.java (JPEXS, URL above: stateFillStyle0 -> fillStyle0 unchanged except morphshape); C:\dev\ref\fla-viewer\AGENTS.md:369-379
- **F14** [confirmed] Edge@cubics is editor data (Animate keeps the original cubic curves); the edges attribute holds the quadratic approximation, which is what Flash publishes. The schema pattern is: '!' int ' ' int '(' [int ',' int] ';' c1x,c1y c2x,c2y ex,ey ([QqPp]? int int)* ')' [int ',' int] ';' with integer twips only. xfl2svg ignores cubics; fla-viewer prefers it. For SWF8 output, use edges and keep cubics verbatim for round-trip. If an Edge has cubics but no edges, approximate the cubic with quadratics ourselves.
  - evidence: C:\dev\ref\xfl-schema\xfl.xsd:2134-2143; C:\dev\ref\fla-viewer\AGENTS.md:320-349; https://raw.githubusercontent.com/PluieElectrique/xfl2svg/master/xfl2svg/shape/edge.py (ignore the cubics attribute)
- **F15** [confirmed] Fills (FillGroup). SolidColor: color default #000000, alpha default 1.0. LinearGradient and RadialGradient: a required <matrix>, at least 2 GradientEntry (color, alpha, ratio as a float in 0..1), spreadMethod = pad | reflect | repeat (default pad), interpolationMethod = rgb | linearRGB (default rgb), and on RadialGradient focalPointRatio in -1..1 (default 0). The gradient matrix maps the gradient square of ±16384/20 = ±819.2 px onto shape space in pixels: a..d have the same numeric values as the SWF Fixed16 fields, and tx/ty are in pixels. BitmapFill: bitmapPath (= a DOMBitmapItem@name) and a required <matrix>.
  - evidence: C:\dev\ref\xfl-schema\xfl.xsd:264-312,386-470; C:\dev\ref\fla-viewer\src\renderer.ts:4067-4069,4105-4106; https://raw.githubusercontent.com/PluieElectrique/xfl2svg/master/xfl2svg/shape/gradient.py (start = a*-16384/20+tx); XFLConverter.java (JPEXS: ratio = getRatioFloat; spreadMethod omitted for pad)
- **F16** [confirmed] bitmapIsClipped default: the schema says true, but JPEXS writes bitmapIsClipped='true' only for clipped SWF fill types and fla-viewer reads an absent attribute as false (repeating). Treat absent as false. Smoothing is a property of the bitmap item (DOMBitmapItem@allowSmoothing, default false). The BitmapFill matrix uses a=d=20 for 1:1 (twips per bitmap pixel). For tx/ty, JPEXS divides by the unit divisor (pixels) while fla-viewer divides them by 20 as well (twips); this conflict is unresolved.
  - evidence: C:\dev\ref\xfl-schema\xfl.xsd:462,3504; C:\dev\ref\fla-viewer\src\fla-parser.ts:1673,1819; C:\dev\ref\fla-viewer\src\renderer.ts:4029-4057; XFLConverter.java (JPEXS: bitmapIsClipped only for CLIPPED_BITMAP/NON_SMOOTHED_CLIPPED_BITMAP; convertMatrix divides only tx/ty)
- **F17** [partly-wrong] Strokes. StrokeStyle@index wraps one of SolidStroke | DashedStroke | DottedStroke | RaggedStroke | StippleStroke | HatchedStroke. Every stroke has a required inner <fill> (FillGroup) and weight (pixels, default 1.0), miterLimit (default 3.0), pixelHinting (default false), sharpCorners (default false) and scaleMode = none | normal | vertical | horizontal. SolidStroke adds caps = none | round | square (default round), joints = miter | round | bevel (default round) and solidStyle ('hairline' marks a one-twip line). JPEXS maps SWF flags as: noHScale only = 'vertical', noVScale only = 'horizontal', both = 'none', neither = 'normal'; it omits caps when round and miterLimit when 3. DashedStroke uses dash1 and dash2 (points, default 6).
  - evidence: C:\dev\ref\xfl-schema\xfl.xsd:472-822 (solidStyle 484; caps 485-496; joints 497-512; scaleMode 763-791; weight 792-801; miterLimit 802-811); XFLConverter.java (JPEXS stroke mapping, URL above)
  - CORRECTION: Mostly confirmed: AbstractStrokeType is at xfl.xsd:741-811 (scaleMode default 'none', weight default 1.0 'in pixels', miterLimit 3.0, pixelHinting and sharpCorners false); caps and joints default to round (:485-512); solidStyle at :484. The JPEXS scale mapping is at XFLConverter.java:415-430. Correction: JPEXS never writes 'round' caps: it writes only caps='none' or 'square' (:462-468), so 'omits caps when round' is right, but joints and caps are also absent for LINESTYLE1 (DefineShape1-3; :435-449). The dash1/dash2 default of 6.0 is confirmed (around xsd:521-532), but the unit 'points' is not stated in the cited lines.
- **F18** [confirmed] DOMShape: optional <matrix> and <transformationPoint>, then <fills>, <strokes> and a required <edges> holding one or more Edge elements, plus attributes isDrawingObject (default false) and isFloating (default false). Edge coordinates are in the shape's local space. DOMGroup holds matrix, transformationPoint and <members> (an ElementList; groups can nest). fla-viewer's assumption is that a member WITH its own matrix stores it in the group's parent space ('absolute'), and only a member without one inherits the group matrix.
  - evidence: C:\dev\ref\xfl-schema\xfl.xsd:1501-1526,2052-2159; C:\dev\ref\fla-viewer\src\fla-parser.ts:1056-1091,1498-1522
- **F19** [confirmed] Matrix: a=1, b=0, c=0, d=1, tx=0, ty=0 when absent. a..d are unitless and tx/ty are pixels (floats). This is the SWF matrix with tx/ty divided by 20: JPEXS writes translateX/unitDivisor and leaves a..d unchanged. transformationPoint/Point has x=0 and y=0 when absent, in the element's local coordinates (pixels).
  - evidence: C:\dev\ref\xfl-schema\xfl.xsd:215-262,1153-1173; XFLConverter.java (JPEXS convertMatrix: tx = translateX / SWF.unitDivisor); C:\dev\ref\fla-viewer\AGENTS.md:127-140
- **F20** [confirmed] DOMSymbolInstance. libraryItemName is required. symbolType defaults to 'movie clip' and real files omit it for movie clips; it is written only for graphic and button ('ik container' exists for IK). name is the instance name; graphics have none. loop = loop | play once | single frame (default loop) and firstFrame (0-based, default 0) apply to graphics. trackAsMenu (default false) applies to buttons. Also: blendMode (normal, layer, multiply, screen, lighten, darken, difference, add, subtract, invert, alpha, erase, overlay, hardlight), cacheAsBitmap (default false), isVisible (CS5.5+, default true), locked, exportAsBitmap, bits32, centerPoint3DX/Y/Z, rotationX/Y/Z, matrix3D (a string of 16 numbers). Child elements: matrix, transformationPoint, Actionscript, filters, color/Color, MatteColor, persistentData. lastFrame is not in the schema; Animate writes it and fla-viewer reads it.
  - evidence: C:\dev\ref\xfl-schema\xfl.xsd:206-213,4386-4638 (symbolType 4505; loop 4525-4548; firstFrame 4549; name 4554; blendMode 4559-4581; isVisible 4582; trackAsMenu 4595; cacheAsBitmap 4600); C:\dev\ref\fla-viewer\AGENTS.md:1038-1039; C:\dev\ref\fla-viewer\src\fla-parser.ts:174-176,1116
- **F21** [partly-wrong] Color transform <color><Color/>: redMultiplier, greenMultiplier, blueMultiplier and alphaMultiplier are floats (default 1.0); redOffset, greenOffset, blueOffset and alphaOffset are integers (default 0, range -255..255). Shorthands: brightness in -1..1, and tintMultiplier in 0..1 together with tintColor #RRGGBB. fla-viewer derives the transform as: brightness b >= 0 gives mult 1-b and offset 255b; b < 0 gives mult 1+b; tint t with colour C gives mult 1-t and offset C*t.
  - evidence: C:\dev\ref\xfl-schema\xfl.xsd:4409-4489; C:\dev\ref\fla-viewer\src\fla-parser.ts:3609-3715
  - CORRECTION: The multiplier and offset types and defaults are confirmed (xfl.xsd:4413-4466; offsets are xs:integer with NO range in the xsd), as are brightness -1..1 and tintMultiplier 0..1. The '-255..255' range comes only from fla-viewer comments (fla-parser.ts:3639-3667). The precedence advice is unverified, and fla-viewer does the opposite: it applies brightness and then tint AFTER the explicit attributes, so the shorthands overwrite them (fla-parser.ts:3673-3714).
- **F22** [confirmed] Filters inside <filters>: DropShadowFilter, BlurFilter, GlowFilter, BevelFilter, GradientGlowFilter, GradientBevelFilter and AdjustColorFilter (brightness, contrast and saturation from -100 to 100, hue from -180 to 180). Real files also contain ColorMatrixFilter (a 'matrix' attribute of 20 comma-separated values) and ConvolutionFilter (matrixX, matrixY, matrix, divisor, bias, preserveAlpha, clamp, color, alpha); neither is in the schema. Shared defaults: blurX = blurY = 5 (px), quality = 1 (1-3), angle = 45 (degrees), distance = 5 (px), strength = 1.0 (a ratio, 1 = 100%), knockout, inner and hideObject = false, isEnabled = true. The default GlowFilter colour is disputed: #000000 in the schema, #FF0000 in fla-viewer.
  - evidence: C:\dev\ref\xfl-schema\xfl.xsd:1175-1484 (Glow color default 1338); C:\dev\ref\fla-viewer\src\fla-parser.ts:3302-3440 (ColorMatrixFilter 3383, ConvolutionFilter 3412, glow '#FF0000' 3337, isEnabled 3466); C:\dev\ref\fla-viewer\AGENTS.md:952-966
- **F23** [confirmed] DOMBitmapItem, in <media>. Attributes: name (the library name, unique), href (the original file name or path, for example 'bitmap.png'), bitmapDataHRef (the file name inside bin/, for example 'M 2 1615711026.dat'), frameRight and frameBottom (required, twips, so pixel width = frameRight/20), frameLeft and frameTop, quality (0-100, required), compressionType and originalCompressionType = photo | lossless (default photo), useImportedJPEGData (default true), allowSmoothing (default false), useDeblocking, isJPEG (default false), sourceExternalFilepath (for example './LIBRARY/myfile.png'), sourceLastImported, externalFileCRC32, externalFileSize, plus the linkage attributes. fla-viewer looks for bin/<bitmapDataHRef> first and falls back to <href> under LIBRARY/, which is how an uncompressed folder may store the image. DOMBitmapInstance carries only libraryItemName, name, matrix, transformationPoint and persistentData.
  - evidence: C:\dev\ref\xfl-schema\xfl.xsd:824-857,3445-3521,1528-1560,167-180; C:\dev\ref\fla-viewer\src\fla-parser.ts:1900-1916,1950-1962
- **F24** [partly-wrong] Lossless bitmap bin/*.dat with magic 03 05, all fields little-endian. Bytes 0-1: magic. 2-3: u16 rowBytes = 4*width. 4-5: u16 width. 6-7: u16 height. 8-23: four u32 frame bounds in twips (0, 20w, 0, 20h). 24: u8 hasAlpha. 25: u8 compressed (1 = chunked zlib, 0 = raw pixels). From byte 26: repeated records of [u16 len][len bytes], with len at most 2048, ended by a u16 0. Concatenating the records gives one zlib stream (header 78 01, Deflater level 1) that inflates to width*height*4 bytes of A,R,G,B pixels premultiplied by alpha. The JPEXS writer stores colour as floor(c*(a+1)/256) and the alpha byte as a+1 for 0 < a < 255, so a reader should undo it with alpha-1. Some files need a 32 KiB all-zero preset dictionary; ignore bytes past the expected pixel count.
  - evidence: https://raw.githubusercontent.com/jindrapetrik/jpexs-decompiler/master/libsrc/ffdec_lib/src/com/jpexs/decompiler/flash/xfl/ImageBinDataGenerator.java (header fields, Deflater(1), 2048-byte chunks, UI16 0 terminator, A,R,G,B order); C:\dev\ref\fla-viewer\AGENTS.md:679-780,811-844; C:\dev\ref\fla-viewer\src\fla-parser.ts:2627-2662
  - CORRECTION: Two errors. (1) The JPEXS writer premultiplies with the ORIGINAL alpha, floor(c*a/256), and only THEN stores a+1 for 0<a<255 (ImageBinDataGenerator.java, the loop after 'some weird premultiplication'); the report's floor(c*(a+1)/256) copies a mistake from fla-viewer AGENTS.md:747-751. The JPEXS reader does a=a-1 and then c = floor(c*256/a) (LosslessImageBinDataReader.java readImage). (2) '03 05' is not a fixed magic: byte 0 is a MAJOR VERSION and byte 1 is a minor version that must be 5. The reader handles major 0 (uncompressed, no alpha: no flags byte and no variant byte, so pixels start at byte 24), 2 (variant byte only) and 3 (flags byte, then variant byte). hasAlpha=0 means alpha must be forced to 255. The chunk size of 2048 is the writer's choice ('2024 is used in CS5'), not a format limit. The A,R,G,B byte order is confirmed.
- **F25** [confirmed] 8-bit palette bitmap .dat with magic 03 03. It shares the 03 05 header layout up to byte 24. Byte 25 is the palette colour count (0 means 256). Bytes 26-27 are a prefix of unknown meaning. Then come count palette entries of 4 bytes each, R,G,B,A with alpha last and NOT premultiplied, followed by the indices, deflate-compressed with the same chunk framing. Index rows are padded to rowBytes. This layout was reverse-engineered from real files only.
  - evidence: C:\dev\ref\fla-viewer\AGENTS.md:782-809
- **F26** [confirmed] JPEG bitmaps in bin/*.dat. The JPEXS writer copies the original JPEG bytes (FF D8 FF ...) and then appends four u32 values (0, 20w, 0, 20h). fla-viewer passes JPEG data straight through, detected by magic bytes. A JPEG decoder has to tolerate the 16-byte trailer after EOI; for SWF output, strip the trailer before wrapping the data in DefineBitsJPEG2/3. PNG (89 50 4E 47) and GIF may also appear as raw files.
  - evidence: ImageBinDataGenerator.java (JPEXS, URL above: JPEG branch writes inputData then 4 UI32); C:\dev\ref\fla-viewer\AGENTS.md:685-695,843; C:\dev\ref\fla-viewer\src\fla-parser.ts:2006-2035
- **F27** [confirmed] Classic text (DOMStaticText, DOMDynamicText, DOMInputText) shares AbstractDOMTextType. Children: matrix, transformationPoint, textRuns/DOMTextRun (characters plus textAttrs/DOMTextAttrs), filters, persistentData. Attributes: left, top, width and height (float, pixels, default 0), autoExpand (default false), scrollable, includeOutlines, fontRenderingMode = device | bitmap | standard | advanced | customThicknessSharpness (default advanced; the schema has no attribute called 'renderingMode'), antiAliasSharpness, antiAliasThickness. Static and Dynamic text add isSelectable (default true) and orientation = horizontal | 'vertical right to left' | 'vertical left to right'. Dynamic and Input text add name, renderAsHTML, border, lineType = 'single line' | multiline | 'multiline no wrap' | password (default 'single line') and variableName. Input text also has maxCharacters.
  - evidence: C:\dev\ref\xfl-schema\xfl.xsd:871-1151,1609-1650
- **F28** [partly-wrong] DOMTextAttrs. face is required (the font name). Defaults: size=12 (points, integer in the schema), bitmapSize = size*20 (twips), fillColor=#000000, alpha=1, alignment = left | right | center | justify (default left), lineSpacing=2 (points), letterSpacing=0 (points), autoKern=true, aliasText=true, characterPosition = normal | superscript | subscript, indent, leftMargin and rightMargin (points, default 0), url, target, rotation (bool), lineHeight (Animate). Paragraph attributes are repeated on every run. Line breaks inside <characters> are CR characters (a run usually ends with '\r'), so treat \r, \n and \r\n as paragraph breaks.
  - evidence: C:\dev\ref\xfl-schema\xfl.xsd:893-997; C:\dev\ref\fla-viewer\src\renderer.ts:2794-2799,2969-2976 ('the common ...\r run shape')
  - CORRECTION: bitmapSize has NO default in the xsd: it is documented as 'Font size in twips (20 times the font size in pt)' (xfl.xsd:902-906), so 'default = size*20' is a derived rule, not a schema default. The other defaults are confirmed: size 12 xs:integer in points (:897-901); lineSpacing 2.0 and letterSpacing 0.0 in points; autoKern and aliasText true; rotation; lineHeight default 0.0 (:992). The CR line-break behaviour is at renderer.ts:2794-2799.
- **F29** [confirmed] RTL. Classic XFL text has no direction or bidi attribute; the only orientation values are vertical writing modes. A direction attribute exists only in TLF markup (DOMTLFText, textLayout.xsd direction = inherit | ltr | rtl). AbstractDOMTextType does allow <persistentData><PD n t v/></persistentData> (types s, i, d, I, D, B), which is a documented carrier for our own per-text metadata such as base direction.
  - evidence: C:\dev\ref\xfl-schema\xfl.xsd:1139-1150,1013,4639-4648,1652-2009; C:\dev\ref\xfl-schema\textLayout.xsd:757-768; I searched xfl.xsd for direction, rtl and bidi and found no matches
- **F30** [confirmed] Library item files. Root <DOMSymbolItem name itemID symbolType (default movie clip) lastModified scaleGridLeft/Right/Top/Bottom linkage*> with child <timeline><DOMTimeline .../></timeline>. LinkageAttributeGroup: linkageExportForAS (default false), linkageIdentifier (AS2 attachMovie id), linkageClassName, linkageBaseClass, linkageExportInFirstFrame (default true), linkageExportForRS, linkageImportForRS, linkageURL. A button symbol's timeline uses frames 0-3 as Up, Over, Down and Hit. fla-viewer reads 9-slice data from scalingGrid='true' plus scalingGridRect='l t r b' in twips, which differs from the schema's scaleGrid* attributes.
  - evidence: C:\dev\ref\xfl-schema\xfl.xsd:4249-4319,3273-3344; C:\dev\ref\fla-viewer\src\fla-parser.ts:526-560,566-604
- **F31** [confirmed] DOMFolderItem: name (a path using '/'), itemID, isExpanded. DOMFontItem: name (library name), font (font family/face name), size, id, sourceLastImported (all required), embedRanges (pipe-separated range ids such as '1|2|3'), embeddedCharacters (literal characters), plus linkage attributes. DOMSoundItem: name, href, soundDataHRef (in bin/), format (for example '44kHz 16bit Stereo', which describes playback, not the stored codec), sampleCount, exportFormat, exportBits, dataLength, exportNative, cacheFormat, cachedSampleCount. Sniff MP3 (FF Ex or 'ID3') and trim the data to dataLength.
  - evidence: C:\dev\ref\xfl-schema\xfl.xsd:3358-3438,3581-3675; C:\dev\ref\fla-viewer\AGENTS.md:861-870
- **F32** [confirmed] PublishSettings.xml has no namespace. Layout: <flash_profiles><flash_profile version name current> containing <PublishFlashProperties enabled>, whose relevant string elements are Version (target player), ActionScriptVersion, CompressMovie, CompressionType (newer files), Quality (JPEG), InvisibleLayer, OmitTraceActions, Protect, DebuggingPermitted, PackagePaths (AS2 classpath), and StreamFormat, StreamCompress, EventFormat and EventCompress for sound. The frame rate is NOT stored here; it is DOMDocument@frameRate. The schema keeps every value as a string because real files contain empty fields.
  - evidence: C:\dev\ref\xfl-schema\PublishSettings.xsd:5-6,9-22,96-145 (Version 97, ActionScriptVersion 99, CompressMovie 106, CompressionType 107, InvisibleLayer 108)
- **F33** [partly-wrong] Animate-era (and some CS4+) items to tolerate. layerType='camera' with a __Camera__ instance; DOMTimeline cameraLayerEnabled and layerDepthEnabled; DOMLayer attachedToCamera, transparent and alphaPercent; DOMFrame frameZDepth. Layer parenting, where parentLayerIndex links a normal layer to another normal layer (Animate 2019; CS6 ignores it). Object motion tweens (CS4+): tweenType='motion object', <motionObjectXML><AnimationCore TimeScale duration Version>, layer animationType='motion object', isMotionObject, visibleAnimationKeyframes, tweenInstanceName. IK: animationType='IK pose', poseLocations, tweenType='IK pose', isIKPose, <IKTree>, <betweenFrameMatrixList>, symbolType='ik container'. 3D: rotationX/Y/Z, matrix3D, centerPoint3DZ, DOMDocument viewAngle3D and vanishingPoint3D*. Also <tweens><Ease target method intensity>, loop='loop reverse' or 'play once reverse', lastFrame, VariablePointWidth/WidthMarker inside SolidStroke, DOMTLFText, DOMRectangleObject/DOMOvalObject primitives (CS4+), DOMComponentInstance and DOMCompiledClip*. No attribute for Animate's 'Advanced Layers' appears in either source.
  - evidence: C:\dev\ref\fla-viewer\AGENTS.md:198-244,873-903,935-951,970-1037; C:\dev\ref\xfl-schema\xfl.xsd:2780,2782-2881,2894-2896,3208-3219,3241-3250,4376-4385,4611-4630,1652,2011-2051,2161-2184; C:\dev\ref\fla-viewer\src\fla-parser.ts:658-662,805-810,859,965-985
  - CORRECTION: The list is useful, but the evidence is mislabelled. layerType='camera', cameraLayerEnabled, layerDepthEnabled, attachedToCamera, transparent, alphaPercent, frameZDepth, lastFrame, 'loop reverse', <Ease> and VariablePointWidth are NOT in xfl.xsd (grep finds none); they come only from fla-viewer (AGENTS.md:198-244, 977, 1008). Primitives are 'CS3+' per fla-viewer AGENTS.md:970, not CS4+; DOMRectangleObject and DOMOvalObject are in the xsd at :2011 and :2032. viewAngle3D and vanishingPoint3DX/Y (defaults 275 and 200) are at xsd:4214-4224.
- **F34** [confirmed] MorphShape (shape tweens) stores points as strings of the form 'x, y', using the same decimal or '#' number format as edges, in twips. Each MorphSegment has startPointA/B, fillIndex1/2 and strokeIndex1/2 (default -1), and holds MorphCurves elements (controlPointA/B, anchorPointA/B, isLine).
  - evidence: C:\dev\ref\xfl-schema\xfl.xsd:2564-2673; C:\dev\ref\fla-viewer\src\fla-parser.ts:3745-3800
- **F35** [confirmed] Matching dependencies to Ruffle avoids duplicate crates. The Ruffle workspace pins quick-xml 0.37.5 (used by ruffle_core), flate2 1.1.9 and zip 9.0.5 (default-features = false; frontend-utils enables 'deflate'). Current crates.io maxima are quick-xml 0.42.0, flate2 1.1.10, zip 9.1.3 (published 2026-10-27), roxmltree 0.21.2 and cfb 0.15.0.
  - evidence: C:\dev\ref\ruffle\Cargo.toml:57,79,90; C:\dev\ref\ruffle\core\Cargo.toml:17,34; C:\dev\ref\ruffle\frontend-utils\Cargo.toml:35; C:\dev\ref\ruffle\Cargo.lock:1740,4423,7774; https://crates.io/api/v1/crates/{quick-xml,flate2,zip,roxmltree,cfb}
- **F36** [partly-wrong] Target types in the swf crate for the edge mapping: ShapeRecord::{StyleChange(Box<StyleChangeData>), StraightEdge{delta}, CurvedEdge{control_delta, anchor_delta}}; StyleChangeData{move_to: Option<Point<Twips>>, fill_style_0, fill_style_1, line_style: Option<u32>, new_styles}; FillStyle::{Color, LinearGradient, RadialGradient, FocalGradient{gradient, focal_point: Fixed8}, Bitmap{id, matrix, is_smoothed, is_repeating}}; GradientSpread {Pad, Reflect, Repeat}; GradientInterpolation {Rgb, LinearRgb}; GradientRecord{ratio: u8, color}; LineCapStyle {Round, None, Square}; LineJoinStyle {Round, Bevel, Miter(Fixed8)}; LineStyleFlag NO_V_SCALE / NO_H_SCALE / PIXEL_HINTING / HAS_FILL; Filter enum with 8 SWF8 filters; BlendMode values 0-14; Twips::new(i32); Point::new; PointDelta::new.
  - evidence: C:\dev\ref\ruffle\swf\src\types.rs:364-394,724-829,976-990,1004-1024; C:\dev\ref\ruffle\swf\src\types\point.rs:50,167; C:\dev\ref\ruffle\swf\src\types\twips.rs:65
  - CORRECTION: The types are confirmed at C:\dev\ref\ruffle\swf\src\types.rs:364-394 and 724-829, point.rs:50,167 and twips.rs:65 (const fn new(i32)); they are re-exported at the crate root (lib.rs:34, types.rs:41-43). Two corrections. (1) BlendMode has no variant 1: Normal=0, then Layer=2 through HardLight=14 (types.rs:377-392). (2) LineStyle fields are pub(crate) (types.rs:820-825), so line styles must be built with LineStyle::new().with_width(Twips).with_fill_style(...).with_join_style(...) and similar builders (:831-955).

## Risks

- All format knowledge is reverse-engineered. The XSD's README warns that the schema 'might be incomplete or inaccurate', and the reference checkouts contain no real CS5/CS6 sample: the one XFL fixture (easing-tokens.fla) is synthetic. We need a corpus of real CS5 and CS6 saves before freezing the parser (C:\dev\ref\xfl-schema\README.md; C:\dev\ref\fla-viewer\src\__tests__\fixtures).
- The sources contradict each other on: the sign of acceleration (xsd vs fla-viewer), bitmapIsClipped default (xsd true vs JPEXS/fla-viewer false), GlowFilter default colour (#000000 vs #FF0000), the units of BitmapFill matrix tx/ty (pixels per JPEXS vs twips per fla-viewer), stroke scaleMode default (xsd 'none' vs IDE 'normal'), motionTweenRotate default (xsd 'none' while keyMode bit 0x0400 is set in default files), and fillStyle left/right wording. A wrong choice gives visibly wrong output.
- Arabic library names. Symbol file names inside the ZIP may be encoded in a legacy code page, so href lookup can fail. Mitigate by scanning every LIBRARY/*.xml and keying symbols by their internal @name, as fla-viewer does.
- Classic text has no direction attribute. Arabic in XFL is stored as logical Unicode with no base direction, so the base direction must be inferred or stored by us (persistentData). Flash Player 8 cannot shape Arabic with embedded fonts, so static text has to be shaped at publish time into DefineFont3/DefineText2 glyph runs or outlines. This touches the publish module, not only the reader.
- Object motion tweens (tweenType='motion object', a CS4+ feature) can occur in CS5/CS6 files. Skipping them freezes the animation at the first pose. Milestone 1 needs at least a warning, and ideally baking from AnimationCore.
- Primitive objects (DOMRectangleObject/DOMOvalObject, CS4+) have no edges. Their outline must be rebuilt from parameters, otherwise they disappear.
- Group member matrix semantics (stored in the parent's space or relative to the group) rest only on fla-viewer's heuristic. Getting this wrong misplaces grouped artwork.
- Rounding 1/256-twip XFL coordinates to whole twips can leave hairline gaps if deltas are taken from unrounded values. Always compute deltas from rounded absolute positions (as in the code sketch).
- fla-viewer has no LICENSE file; only package.json says ISC. Use it as a reference and never copy code. The code sketch was written independently.
- Process note: one crates.io API request in this research sent a User-Agent containing the user's e-mail local part ('barajeelair'). No other requests did, and future requests should use a neutral UA.

## Open questions

- Sign of DOMFrame@acceleration: does an IDE 'ease out 100' save as +100 (fla-viewer) or -100 (xsd)? Test with one real CS6 file. The classic-ease curve itself is probably CreateJS Ease.get(amount): amount<0 gives t*(t*-amount+1+amount), amount>0 gives t*((2-t)*amount+(1-amount)), with amount = acceleration/100; this is unverified for Flash Player output.
- Absent motionTweenRotate: treat as 'none' (xsd) or as 'auto' (the IDE default, and keyMode 22017 = 0x4001|0x1000|0x0400|0x0200 sets the rotate bit)? Likewise, should absent scaleMode mean 'normal'?
- Units of BitmapFill matrix tx/ty: pixels (JPEXS writer) or twips (fla-viewer renderer)? a=d=20 meaning 1:1 is agreed.
- When <Color> carries tintColor/tintMultiplier or brightness, does Flash also write the explicit multiplier and offset attributes? If so, prefer the explicit ones. Unverified.
- Is bitmapIsClipped absent = false (JPEXS and fla-viewer) in every real CS5/CS6 file, despite the xsd default of true?
- Does the cubics attribute ever hold more than one cubic, and does an Edge with cubics always also have edges? The xsd pattern allows one cubic per attribute. What do the q/Q/p/P approximation markers mean?
- Do DOMGroup member matrices really live in the group's parent space?
- Is DOMTextAttrs@size ever fractional in CS5/CS6 files (the xsd says integer)? Is face the PostScript name, and are bold/italic encoded in the face name or as separate attributes (fla-viewer reads bold/italic/underline, which are not in the xsd)?
- Does Flash escape characters that are illegal in file names when it writes LIBRARY/<name>.xml, and how are ZIP entry names encoded (UTF-8 flag or code page) for Arabic item names in CS5/CS6 on Windows?
- Exact contents of PublishSettings Version for CS5/CS6 (for example '10' or 'FlashLite40') and the values of CompressionType in Animate. The xsd keeps them as strings and the checkouts have no sample.
- Where trackAsMenu is stored: on the instance (xsd 4595) or on the button DOMSymbolItem (the JPEXS writer, per the fetched source)?
- Middle-East (WinSoft) editions of Flash may have written extra RTL attributes on classic text. Neither source documents any.
- Is there an XFL attribute for Animate 'Advanced Layers' mode? Neither source names one, so it falls under the generic ignore-unknown rule.

## Verifier: problems in the code sketch

- decode_dat_argb does NOT compile. The non-move closure `un` borrows `a`, then `a -= 1` runs while `un` is still used, giving E0506 'cannot assign to `a` because it is borrowed'. I reproduced this with rustc --edition 2021 (C:\dev\rust\cargo\bin\rustc.exe) on the function copied verbatim. Fix: compute `let a_raw = a;` and unpremultiply with an explicit value, or use a `move` closure.
- Unpremultiply uses the wrong divisor. The sketch divides by the stored alpha (a+1). The JPEXS reader first decrements and then computes floor(c*256/(a_stored-1)) (LosslessImageBinDataReader.java), matching the writer's floor(c*a/256). The sketch's comment, which cites writer 'c*(a+1)/256', is wrong.
- decode_dat_argb accepts only 03 05 and hard-codes byte 24 = hasAlpha and byte 25 = variant. Per the JPEXS reader, byte 0 is a major version (0, 2 or 3) and byte 1 must be 5. Major 0 has no flags or variant byte (data at offset 24), major 2 has a variant byte but no flags byte. When hasAlpha (flags&1) is 0, alpha must be forced to 255; the sketch ignores b[24] entirely.
- No length check after inflate. If the zlib stream is short or truncated, `px[s]` panics. The preset-dictionary case makes ZlibDecoder return Err, so the TODO retry is required before release. Check `px.len() >= row*h` first, and use row stride consistently: `need` takes max(row, w*4) but indexing uses `row`.
- The 'publish as is' note for DefineBitsLossless2 is incomplete. The stored alpha byte is a+1 for semi-transparent pixels and the colours are premultiplied by a/256, so subtract 1 from alpha (and ideally re-premultiply by a/255) before re-zlibbing.
- push_edge maps XFL indices 1:1 to SWF style arrays. That is valid only if FillStyle@index and StrokeStyle@index are contiguous 1..n and emitted in index order into ShapeStyles; build an index remap table. It also ignores the DOMShape <matrix> (Edge coordinates are shape-local), and it emits records for Edges with fillStyle0 = fillStyle1 = strokeStyle = 0, which the report's own policy says to skip.
- sniff(): for a bare relative stub path such as 'x.xfl', path.parent() is Some(""), so the result is Unknown; canonicalize first. A UTF-8 BOM before 'PROXY-CS5' would defeat .trim(), so strip '\u{feff}'. A single File::read may short-read; prefer read_exact on min(len, 8).
- parse_edges is correct on all report examples and on fla-viewer's '!7995 6095/8205 7505' and '!-226.5 229.5[-257.90625 #D0.3A ...' (I compiled and ran it). One gap: a bare '/' handled as Close followed by a lineTo with no new moveTo is accepted silently, and fla-viewer additionally inserts an implicit moveTo to the start after close (edge-decoder.ts:533-537). Push_edge's `start` tracking matches that only partly.

## Verifier: missed points

- A local corpus exists that the report did not inspect: C:\dev\samples. Its source files (f8\smoke.fla, fixtures\shapes-basic\shapes-basic.fla) are Flash 8 binary OLE2 FLAs, and their XFL siblings were machine-converted by FlaComDoc (creatorInfo="FlaComDoc", xflVersion="2.0", conversion-report.txt says 'experimental export'). The report's 'refuse pre-CS5 binary' policy therefore rejects the project's own Flash-8 inputs. Decide explicitly whether milestone 1 accepts only converted XFL, which should never be mistaken for Adobe CS5/CS6 output.
- keyMode in real-world data does not follow the xsd table: the converted samples use keyMode="1536" with motionTweenRotate="auto" on untweened frames (C:\dev\samples\f8\smoke-xfl\DOMDocument.xml). The reader must derive tween semantics from tweenType and the explicit attributes only, and treat keyMode as opaque round-trip data.
- The .dat lossless header is versioned (major 0, 2 or 3 plus minor 5; flags and variant bytes are present only for major 3 and 2; hasAlpha=0 forces opaque) per JPEXS LosslessImageBinDataReader.java. The report documents only the 03 05 layout.
- Scenes: every DOMTimeline in DOMDocument is a scene, but SWF8 has no runtime scenes. Publish must concatenate scenes into one root timeline and resolve AS2 gotoAndPlay("Scene 2", n) and scene-relative frame numbers to absolute frames (Flash does this at compile time), which affects the Rascal compilation step.
- AS2 handler placement: on(press/release/...) is legal on movie-clip instances too, compiled as PlaceObject clip actions with ClipEventFlag PRESS, RELEASE, ROLL_OVER and similar (C:\dev\ref\ruffle\swf\src\types.rs:485-492). On button instances, on() becomes DefineButton2 condition actions. The reader must keep the instance's symbolType together with its script so the publisher can choose between them.
- PublishSettings fields that matter for an SWF8 header were omitted: TopDown (layer load order, PublishSettings.xsd:85) and UseNetwork (the FileAttributes use-network bit; it appears in the sample PublishSettings.xml). Also, AS2 class and #include resolution needs PackagePaths (relative to the FLA folder) plus Actionscript@href (xfl.xsd:4662).
- xflVersion is a free string; Animate 2022+ writes '22.0', '23.0' and '24.0' (xfl.xsd:3961-3963). Version gating must parse it as a number, and the schema's DOMCompiledClipItem lives under <media>, not as a LIBRARY file (fla-viewer loads it from LIBRARY, fla-parser.ts:534-536). Tolerate both locations.
- For swf-crate output, LineStyle has pub(crate) fields and must be built through LineStyle::new().with_width(Twips::new(weight*20)).with_fill_style/with_join_style/with_start_cap/with_end_cap/with_allow_scale_x/y/with_is_pixel_hinted (types.rs:831-955). SWF8 DefineShape4 gradients are limited to 15 records, and solidStyle='hairline' needs an explicit width rule (1 twip per the xsd note at xfl.xsd:484).

## Code sketch (researcher; check against corrections above)
```rust
// Original sketch for jazari-xfl (no code copied from fla-viewer).
// Deps: quick-xml 0.37 / flate2 1.1.9 / zip 9.0.5 (match ruffle), swf (ruffle git rev).

use std::{fs, io::{self, Read}, path::{Path, PathBuf}};

// ---------- 1. container sniffing ----------
pub enum Container { Zip(PathBuf), XflFolder(PathBuf), PreCs5Binary, Unknown }

pub fn sniff(path: &Path) -> io::Result<Container> {
    if path.is_dir() {
        return Ok(if path.join("DOMDocument.xml").is_file() { Container::XflFolder(path.into()) } else { Container::Unknown });
    }
    let mut head = [0u8; 8];
    let n = fs::File::open(path)?.read(&mut head)?;
    if n >= 4 && &head[..4] == b"PK\x03\x04" { return Ok(Container::Zip(path.into())); }
    if n == 8 && head == [0xD0, 0xCF, 0x11, 0xE0, 0xA1, 0xB1, 0x1A, 0xE1] { return Ok(Container::PreCs5Binary); } // refuse with a "re-save in CS5+" message
    if fs::metadata(path)?.len() <= 64 && fs::read_to_string(path).map(|s| s.trim() == "PROXY-CS5").unwrap_or(false) {
        if let Some(dir) = path.parent() { return sniff(dir); }               // .xfl stub -> open its folder
    }
    Ok(Container::Unknown)
}

// ---------- 2. edge-format numbers (twips) ----------
/// "1000", "-12.5"  -> decimal twips
/// "#3E8.80"        -> 0x0003E880 as i32 / 256 = 1000.5
/// "#FFFF38.40"     -> 0xFFFF3840 as i32 / 256 = -199.75   (fraction is ADDED: two's-complement 24.8)
pub fn edge_number(tok: &str) -> Option<f64> {
    match tok.strip_prefix('#') {
        Some(hex) => {
            let (int_s, frac_s) = hex.split_once('.').unwrap_or((hex, ""));
            if int_s.len() > 6 || frac_s.len() > 2 { return None; }
            let int_v = if int_s.is_empty() { 0 } else { u32::from_str_radix(int_s, 16).ok()? };
            let frac_v = if frac_s.is_empty() { 0 }
                         else { u32::from_str_radix(frac_s, 16).ok()? << (4 * (2 - frac_s.len() as u32)) };
            Some(((int_v << 8) | frac_v) as i32 as f64 / 256.0)
        }
        None => tok.parse::<f64>().ok(),
    }
}

#[derive(Clone, Copy, Debug)]
pub enum EdgeCmd { MoveTo([f64; 2]), LineTo([f64; 2]), QuadTo { ctrl: [f64; 2], to: [f64; 2] }, Close }

/// Grammar: moveto (moveto|lineto|quadto)*; '!'=move, '|' '/'=line, '[' ']'=quad, 'S'digit=ignored hint.
pub fn parse_edges(s: &str) -> Result<Vec<EdgeCmd>, String> {
    let b = s.as_bytes();
    let mut i = 0usize;
    let mut out = Vec::new();
    let skip_ws = |i: &mut usize| while *i < b.len() && b[*i].is_ascii_whitespace() { *i += 1 };
    let num = |i: &mut usize| -> Option<f64> {
        skip_ws(i);
        let st = *i;
        while *i < b.len() && matches!(b[*i], b'#' | b'.' | b'-' | b'0'..=b'9' | b'A'..=b'F' | b'a'..=b'f') { *i += 1; }
        if st == *i { None } else { edge_number(std::str::from_utf8(&b[st..*i]).ok()?) }
    };
    let pt = |i: &mut usize| -> Option<[f64; 2]> { Some([num(i)?, num(i)?]) };
    loop {
        skip_ws(&mut i);
        let Some(&c) = b.get(i) else { break };
        i += 1;
        match c {
            b'!' => out.push(EdgeCmd::MoveTo(pt(&mut i).ok_or("bad moveTo")?)),
            b'|' => out.push(EdgeCmd::LineTo(pt(&mut i).ok_or("bad lineTo")?)),
            b'/' => { let save = i; match pt(&mut i) { Some(p) => out.push(EdgeCmd::LineTo(p)),
                                                      None => { i = save; out.push(EdgeCmd::Close) } } } // bare '/' tolerated
            b'[' | b']' => { let c = pt(&mut i).ok_or("bad quad")?; let t = pt(&mut i).ok_or("bad quad")?;
                             out.push(EdgeCmd::QuadTo { ctrl: c, to: t }) }
            b'S' => while i < b.len() && b[i].is_ascii_digit() { i += 1 },   // selection hint, editor only
            other => return Err(format!("unexpected '{}' at byte {}", other as char, i - 1)),
        }
    }
    Ok(out)
}

// ---------- 3. one <Edge> -> SWF DefineShape4 records (Ruffle/Flash do the stitching) ----------
use swf::{Point, PointDelta, ShapeRecord, StyleChangeData, Twips};
fn tw(v: f64) -> i32 { v.round() as i32 }   // XFL has 1/256-twip precision; SWF records are whole twips

/// f0/f1/ls = XFL fillStyle0/fillStyle1/strokeStyle: identical 1-based indices in SWF (0 = none).
pub fn push_edge(rec: &mut Vec<ShapeRecord>, pen: &mut (i32, i32), start: &mut (i32, i32),
                 f0: u32, f1: u32, ls: u32, cmds: &[EdgeCmd]) {
    let mut styles = true;                       // set styles on the first StyleChange of this <Edge>
    for cmd in cmds {
        match *cmd {
            EdgeCmd::MoveTo(p) => {
                let to = (tw(p[0]), tw(p[1]));
                rec.push(ShapeRecord::StyleChange(Box::new(StyleChangeData {
                    move_to: Some(Point::new(Twips::new(to.0), Twips::new(to.1))),
                    fill_style_0: styles.then_some(f0),
                    fill_style_1: styles.then_some(f1),
                    line_style: styles.then_some(ls),
                    new_styles: None,
                })));
                styles = false; *pen = to; *start = to;
            }
            EdgeCmd::LineTo(p) => line(rec, pen, (tw(p[0]), tw(p[1]))),
            EdgeCmd::Close => { let s = *start; line(rec, pen, s) }
            EdgeCmd::QuadTo { ctrl, to } => {
                let (c, a) = ((tw(ctrl[0]), tw(ctrl[1])), (tw(to[0]), tw(to[1])));
                rec.push(ShapeRecord::CurvedEdge {
                    control_delta: PointDelta::new(Twips::new(c.0 - pen.0), Twips::new(c.1 - pen.1)),
                    anchor_delta:  PointDelta::new(Twips::new(a.0 - c.0),   Twips::new(a.1 - c.1)),
                });
                *pen = a;
            }
        }
    }
}
fn line(rec: &mut Vec<ShapeRecord>, pen: &mut (i32, i32), to: (i32, i32)) {
    if to != *pen {
        rec.push(ShapeRecord::StraightEdge { delta: PointDelta::new(Twips::new(to.0 - pen.0), Twips::new(to.1 - pen.1)) });
        *pen = to;
    }
}

// ---------- 4. bin/*.dat lossless (03 05) -> straight RGBA8 ----------
pub fn decode_dat_argb(b: &[u8]) -> Result<(u32, u32, Vec<u8>), String> {
    if b.len() < 26 || b[0] != 0x03 || b[1] != 0x05 { return Err("not a 32-bit FLA bitmap".into()); }
    let u16le = |o: usize| u16::from_le_bytes([b[o], b[o + 1]]) as usize;
    let (row, w, h) = (u16le(2), u16le(4), u16le(6));
    let need = row.max(w * 4) * h;
    let px: Vec<u8> = if b[25] == 1 {
        let mut z = Vec::new(); let mut s = &b[26..];
        while s.len() >= 2 { let n = u16::from_le_bytes([s[0], s[1]]) as usize; s = &s[2..];
                             if n == 0 { break } let n = n.min(s.len()); z.extend_from_slice(&s[..n]); s = &s[n..]; }
        let mut out = Vec::with_capacity(need);
        flate2::read::ZlibDecoder::new(&z[..]).take(need as u64).read_to_end(&mut out).map_err(|e| e.to_string())?;
        // TODO: on failure retry raw-deflate with a 32 KiB zero preset dictionary (seen in some files)
        out
    } else { b.get(26..26 + need).ok_or("short raw plane")?.to_vec() };
    let mut rgba = vec![0u8; w * h * 4];
    for y in 0..h { for x in 0..w {
        let s = y * row + x * 4; let d = (y * w + x) * 4;
        let (mut a, r, g, bl) = (px[s] as u32, px[s + 1] as u32, px[s + 2] as u32, px[s + 3] as u32);
        let un = |c: u32| if a > 0 && a < 255 { (c * 256 / a.max(1)).min(255) } else { c };
        if a > 1 && a < 255 { a -= 1; }       // writer stored a+1 for 0<a<255 (JPEXS)
        rgba[d..d + 4].copy_from_slice(&[un(r) as u8, un(g) as u8, un(bl) as u8, a as u8]);
    }}
    Ok((w as u32, h as u32, rgba))
}
// NB: for SWF publish the premultiplied A,R,G,B plane can be re-zlibbed into DefineBitsLossless2 (format 5) directly (±1 LSB alpha).

// ---------- 5. parse policy ----------
// * Match elements by local name; ignore the namespace. Accept attributes and children in any order.
// * Unknown element -> skip its subtree and record a warning. Unknown attribute -> keep it in a side map for round-trip.
// * Unknown enum string -> Unknown(String), never a hard error. Missing keyMode/color/etc. -> schema default.
// * Booleans are "true"/"false". Colours are "#RRGGBB" (accept either case). Floats use '.' only.
```
