// Fixture builder: gradients  (run with tools/oracle/build-all.ps1)
// Linear, radial and focal-point radial gradient fills (Flash 8), alpha stops,
// linearRGB / spread modes, a 15-stop gradient, and gradient strokes.
// Gradient strokes: draw a solid stroke, select it, read a *valid* Stroke with
// getCustomStroke("selection"), set its shapeFill to a valid gradient Fill and
// call setCustomStroke (a fresh getCustomStroke() stub ignores shapeFill).
//
// Flash 8 quirk: setCustomFill() cannot create a gradient from a fresh Fill
// object (getCustomFill() returns a stub {style:"noFill"} and Flash stores
// uninitialised memory as the gradient).  Workaround: import a tiny SWF that
// already holds a linear and a radial gradient (tests\fixtures\_src\
// grad-src.swf, written by tools\oracle\make-gradient-swf.py), read those
// shapes' Fill objects with getCustomFill("selection"), and modify them
// (colorArray, posArray, matrix, focalPoint, overflow, linearRGB, style).
// The imported source layers are deleted before saving.
var NAME = "gradients";
var OUT = JZ_OUT;   // tests/fixtures/NAME/ (from the prelude build-all.ps1 prepends)
var SRC = JZ_SRC + "grad-src.swf";
var LOG = JZ_LOG;   // target/oracle/logs/NAME.log
var FAILS = [];
FLfile.write(LOG, "");
function log(m) { FLfile.write(LOG, m + "\n", "append"); }
function step(label, fn) {
  try { var r = fn(); log("OK   " + label + (r === undefined ? "" : " -> " + r)); return true; }
  catch (e) { FAILS.push(label + ": " + e); log("FAIL " + label + ": " + e); return false; }
}
function dump(o) {
  if (o === null || o === undefined) return String(o);
  var s = "{";
  for (var k in o) {
    var v = o[k];
    if (v && typeof v == "object" && v.length !== undefined) v = "[" + v.join(",") + "]";
    else if (v && typeof v == "object") v = dump(v);
    s += k + ":" + v + " ";
  }
  return s + "}";
}
// ---- Stroke template.  Flash 8 quirk: getCustomStroke() returns a stub and
// every field left unset is stored as 0/none (miter limit 0, scale "none");
// assigning miterLimit / scaleType / strokeHinting on that stub makes
// exportSWF crash Flash.  A Stroke read back from a selected stroke is
// complete and safe to modify (colour must also go into its shapeFill), so a
// throwaway line on a temporary layer supplies one before any content exists.
var STROKE = null;
function initStroke() {
  var t = doc.getTimeline();
  doc.selectNone(); t.setSelectedLayers(0, true); t.currentLayer = 0;
  t.addNewLayer("_stroke_tmp");
  var s0 = doc.getCustomStroke(); s0.style = "solid"; s0.color = "#000000"; s0.thickness = 1; s0.capType = "round"; s0.joinType = "round";
  doc.setCustomStroke(s0);
  doc.addNewLine({x: 10, y: 10}, {x: 60, y: 10});
  doc.selectNone();
  doc.mouseClick({x: 35, y: 10}, false, true);
  STROKE = doc.getCustomStroke("selection");
  doc.selectNone();
  for (var i = 0; i < t.layers.length; i++) if (t.layers[i].name == "_stroke_tmp") break;
  t.setSelectedLayers(i, true); t.currentLayer = i;
  t.deleteLayer(i);
  doc.selectNone(); t.setSelectedLayers(0, true); t.currentLayer = 0;
  if (!STROKE || STROKE.thickness != 1) throw new Error("no valid Stroke template");
  return "template " + STROKE.style + " " + STROKE.thickness + "px; layers now " + t.layers.length;
}
function solidStroke(color, w, cap, join, miter) {
  var S = STROKE;
  S.style = "solid"; S.thickness = w; S.capType = cap || "round"; S.joinType = join || "round";
  S.miterLimit = miter || 3; S.scaleType = "normal"; S.strokeHinting = false;
  S.color = color;
  var sf = S.shapeFill; sf.style = "solid"; sf.color = color; S.shapeFill = sf;
  doc.setCustomStroke(S);
}
function noStroke() { var s = doc.getCustomStroke(); s.style = "noStroke"; doc.setCustomStroke(s); }
// A valid gradient Fill object read from the imported source shapes.
var SRCPT = {linear: {x: 70, y: 50}, radial: {x: 190, y: 50}};
// Clicking a shape makes its layer the current layer, so restore it afterwards.
function srcFill(kind) {
  var tl = doc.getTimeline(), cur = tl.currentLayer;
  doc.selectNone();
  doc.mouseClick(SRCPT[kind], false, true);
  var f = doc.getCustomFill("selection");
  doc.selectNone();
  tl.currentLayer = cur;
  if (!f || f.style.indexOf("Gradient") < 0) throw new Error("no valid " + kind + " source fill: " + dump(f));
  return f;
}
// Gradient box for a bounding rect (JSFL fill matrices use the 1638.4 px gradient square).
function box(l, t, r, b, rot) {
  var w = (r - l) / 1638.4, h = (b - t) / 1638.4;
  var c = Math.cos(rot || 0), s = Math.sin(rot || 0);
  return {a: w * c, b: w * s, c: -h * s, d: h * c, tx: (l + r) / 2, ty: (t + b) / 2};
}
function gradient(kind, style, colors, pos, rect, extra) {
  var f = srcFill(kind);
  f.style = style; f.colorArray = colors; f.posArray = pos;
  f.matrix = box(rect.left, rect.top, rect.right, rect.bottom, extra && extra.rotation);
  f.overflow = (extra && extra.overflow) || "Extend";
  f.linearRGB = !!(extra && extra.linearRGB);
  f.focalPoint = (extra && extra.focalPoint) || 0;
  return f;
}
function readBack(pt) {
  doc.selectNone();
  doc.mouseClick(pt, false, true);
  var s = "sel=" + doc.selection.length + " fill=" + dump(doc.getCustomFill("selection"));
  doc.selectNone();
  return s;
}

var doc = null, srcLayers = [];
try {
  FLfile.createFolder(OUT);
  doc = fl.createDocument();
  var tl = doc.getTimeline();
  step("stage 550x400 @ 12fps, light grey", function () {
    doc.width = 550; doc.height = 400; doc.frameRate = 12; doc.backgroundColor = "#E0E0E0";
    fl.objectDrawingMode = false;
    return doc.width + "x" + doc.height + " bg=" + doc.backgroundColor;
  });
  step("stroke template", initStroke);
  step("import gradient source SWF", function () {
    tl.layers[0].name = "src";
    doc.importFile(SRC, false);
    log("     imported; layers=" + tl.layers.length);
    // the import arrives as groups; break them apart once (as in probe-gradient2.jsfl)
    doc.selectAll();
    log("     selected " + doc.selection.length);
    doc.breakApart();
    log("     broken apart");
    doc.selectNone();
    var s = "layers=" + tl.layers.length;
    for (var i = 0; i < tl.layers.length; i++) { srcLayers.push(tl.layers[i].name); s += " " + tl.layers[i].name + ":" + tl.layers[i].frames[0].elements.length; }
    return s + " | linear=" + dump(srcFill("linear")) + " | radial=" + dump(srcFill("radial"));
  });

  // ---- layer "backdrop": black bars behind the alpha gradients
  step("layer backdrop: black bars", function () {
    tl.currentLayer = 0;
    tl.addNewLayer("backdrop");
    noStroke();
    var f = doc.getCustomFill(); f.style = "solid"; f.color = "#000000"; doc.setCustomFill(f);
    doc.addNewRectangle({left: 0, top: 120, right: 550, bottom: 140}, 0, false, true);
    doc.addNewRectangle({left: 225, top: 160, right: 245, bottom: 310}, 0, false, true);
    return tl.layers[tl.currentLayer].frames[0].elements.length + " element(s)";
  });

  tl.addNewLayer("fills");
  // ---- row 1 (y 90..170): linear gradients
  var R1 = {left: 20, top: 90, right: 170, bottom: 170};
  step("linear 3 stops red/yellow/blue, horizontal", function () {
    doc.setCustomFill(gradient("linear", "linearGradient", ["#FF0000", "#FFFF00", "#0000FF"], [0, 128, 255], R1));
    doc.addNewRectangle(R1, 0, false, true);
    return readBack({x: 95, y: 100});
  });
  var R2 = {left: 190, top: 90, right: 340, bottom: 170};
  step("linear alpha stops blue a0 -> blue a255 -> green a128, rotated 90deg (vertical)", function () {
    doc.setCustomFill(gradient("linear", "linearGradient", ["#0000FF00", "#0000FFFF", "#00FF0080"], [0, 128, 255],
                               {left: 225, top: 55, right: 305, bottom: 205}, {rotation: Math.PI / 2}));
    doc.addNewRectangle(R2, 0, false, true);
    return readBack({x: 265, y: 100});
  });
  var R3 = {left: 360, top: 90, right: 530, bottom: 170};
  step("linear white/purple, linearRGB, overflow Reflect, 34px box", function () {
    doc.setCustomFill(gradient("linear", "linearGradient", ["#FFFFFF", "#9900CC"], [0, 255],
                               {left: 428, top: 90, right: 462, bottom: 170}, {overflow: "Reflect", linearRGB: true}));
    doc.addNewRectangle(R3, 0, false, true);
    return readBack({x: 445, y: 100});
  });

  // ---- row 2 (y 180..310): radial and focal radial
  var O1 = {left: 20, top: 180, right: 150, bottom: 310};
  step("radial white -> black", function () {
    doc.setCustomFill(gradient("radial", "radialGradient", ["#FFFFFF", "#000000"], [0, 255], O1));
    doc.addNewOval(O1, false, true);
    return readBack({x: 85, y: 245});
  });
  var O2 = {left: 170, top: 180, right: 300, bottom: 310};
  step("radial alpha red a255 -> a153 -> a0 over a black bar", function () {
    doc.setCustomFill(gradient("radial", "radialGradient", ["#FF0000FF", "#FF000099", "#FF000000"], [0, 160, 255], O2));
    doc.addNewOval(O2, false, true);
    return readBack({x: 200, y: 245});
  });
  var O3 = {left: 320, top: 180, right: 450, bottom: 310};
  step("focal radial focalPoint=+180 yellow/orange/dark red", function () {
    doc.setCustomFill(gradient("radial", "radialGradient", ["#FFFF66", "#FF9900", "#660000"], [0, 110, 255], O3, {focalPoint: 180}));
    doc.addNewOval(O3, false, true);
    return readBack({x: 385, y: 245});
  });
  var O4 = {left: 465, top: 180, right: 540, bottom: 310};
  step("focal radial focalPoint=-120, overflow Repeat, 40px box, on a rect", function () {
    doc.setCustomFill(gradient("radial", "radialGradient", ["#00FFFF", "#003366"], [0, 255],
                               {left: 482, top: 225, right: 522, bottom: 265}, {focalPoint: -120, overflow: "Repeat"}));
    doc.addNewRectangle(O4, 0, false, true);
    return readBack({x: 500, y: 190});
  });

  // ---- row 3 (y 320..395): 15-stop gradient (Flash 8 maximum)
  var R4 = {left: 440, top: 320, right: 540, bottom: 395};
  step("linear 15 stops", function () {
    var c = ["#FF0000", "#FF7F00", "#FFFF00", "#7FFF00", "#00FF00", "#00FF7F", "#00FFFF", "#007FFF",
             "#0000FF", "#7F00FF", "#FF00FF", "#FF007F", "#FFFFFF", "#7F7F7F", "#000000"];
    var p = [];
    for (var i = 0; i < 15; i++) p.push(Math.round(i * 255 / 14));
    doc.setCustomFill(gradient("linear", "linearGradient", c, p, R4));
    doc.addNewRectangle(R4, 0, false, true);
    return readBack({x: 490, y: 357});
  });

  // ---- layer "strokes": gradient strokes
  tl.addNewLayer("strokes");
  step("gradient stroke via stroke.shapeFill on a selected stroke (rect outline 14px)", function () {
    solidStroke("#808080", 14, "round", "round");
    doc.addNewRectangle({left: 25, top: 330, right: 215, bottom: 385}, 0, true, false);
    var g = gradient("linear", "linearGradient", ["#FF0000", "#00FF00", "#0000FF"], [0, 128, 255],
                     {left: 18, top: 323, right: 222, bottom: 392});
    doc.selectNone();
    doc.mouseDblClk({x: 25, y: 357}, false, false, true);
    var n = doc.selection.length;
    var sel = doc.getCustomStroke("selection");
    var before = dump(sel);
    sel.shapeFill = g;
    doc.setCustomStroke(sel);
    doc.selectNone();
    doc.mouseDblClk({x: 25, y: 357}, false, false, true);
    var after = dump(doc.getCustomStroke("selection"));
    doc.selectNone();
    return "selected " + n + "; stroke before=" + before + " after=" + after;
  });
  step("radial gradient stroke 10px on an oval outline (same technique, own layer)", function () {
    tl.addNewLayer("stroke-radial");
    solidStroke("#808080", 10, "round", "round");
    doc.addNewOval({left: 260, top: 320, right: 400, bottom: 395}, true, false);
    var g = gradient("radial", "radialGradient", ["#FFFF00", "#FF00FF"], [0, 255], {left: 255, top: 315, right: 405, bottom: 400});
    doc.selectNone();
    var els = tl.layers[tl.currentLayer].frames[0].elements;
    doc.selection = [els[0]];
    var sel = doc.getCustomStroke("selection");
    sel.shapeFill = g;
    doc.setCustomStroke(sel);
    doc.selectNone();
    doc.selection = [els[0]];
    var after = dump(doc.getCustomStroke("selection"));
    doc.selectNone();
    return "after=" + after;
  });

  step("delete gradient source layers", function () {
    var removed = 0;
    for (var i = tl.layers.length - 1; i >= 0; i--) {
      for (var j = 0; j < srcLayers.length; j++) {
        if (tl.layers[i].name == srcLayers[j]) { tl.deleteLayer(i); removed++; break; }
      }
    }
    return "removed " + removed + "; library items=" + doc.library.items.length;
  });
  step("layer actions: frame 1 trace", function () {
    tl.currentLayer = 0;
    tl.addNewLayer("actions");
    tl.layers[tl.currentLayer].frames[0].actionScript = 'trace("gradients: frame 1");';
  });
  step("summary", function () {
    var s = "layers=" + tl.layers.length + " frames=" + tl.frameCount;
    for (var i = 0; i < tl.layers.length; i++) s += " | " + tl.layers[i].name + ":" + tl.layers[i].frames[0].elements.length;
    return s;
  });
  step("save FLA", function () { return fl.saveDocument(doc, OUT + NAME + ".fla"); });
  // Publish from the saved file, not from the edited in-memory document:
  // Flash 8 keeps unused fill/line styles of edited shapes in memory (deleted
  // regions, replaced strokes) and would write them into the SWF; they are not
  // saved in the FLA, so the reopened document is what JPEXS and Al-Jazari see.
  step("reopen saved FLA, export SWF from it", function () {
    fl.closeDocument(doc, false);
    doc = fl.openDocument(OUT + NAME + ".fla");
    doc.exportSWF(OUT + "flash8.swf", true);
    return doc.name + " -> " + FLfile.exists(OUT + "flash8.swf");
  });
} catch (e) { FAILS.push("top: " + e); log("ERROR: " + e); }
log("FAILED STEPS: " + FAILS.length);
for (var i = 0; i < FAILS.length; i++) log("  - " + FAILS[i]);
try { if (doc) fl.closeDocument(doc, false); } catch (e) { log("close: " + e); }
log("DONE");
fl.quit(false);
