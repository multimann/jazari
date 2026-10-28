// Fixture builder: shapes-basic  (run with tools/oracle/build-all.ps1)
// Raw shapes on the main timeline: solid fills, strokes of several widths /
// caps / joins, ovals, shapes with holes, overlapping shapes on several layers.
var NAME = "shapes-basic";
var OUT = JZ_OUT;   // tests/fixtures/NAME/ (from the prelude build-all.ps1 prepends)
var LOG = JZ_LOG;   // target/oracle/logs/NAME.log
var FAILS = [];
FLfile.write(LOG, "");
function log(m) { FLfile.write(LOG, m + "\n", "append"); }
function step(label, fn) {
  try { var r = fn(); log("OK   " + label + (r === undefined ? "" : " -> " + r)); return true; }
  catch (e) { FAILS.push(label + ": " + e); log("FAIL " + label + ": " + e); return false; }
}
function solidFill(doc, color) { var f = doc.getCustomFill(); f.style = "solid"; f.color = color; doc.setCustomFill(f); }
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
function shapeInfo(el) {
  var s = el.elementType;
  try { s += " edges=" + el.edges.length + " contours=" + el.contours.length; } catch (e) { s += " (no edge info: " + e + ")"; }
  return s;
}
function layerInfo(tl, i) {
  var els = tl.layers[i].frames[0].elements, s = tl.layers[i].name + ": " + els.length + " element(s)";
  for (var k = 0; k < els.length; k++) s += " [" + shapeInfo(els[k]) + "]";
  return s;
}

var doc = null;
try {
  FLfile.createFolder(OUT);
  doc = fl.createDocument();
  var tl = doc.getTimeline();
  step("stage 550x400 @ 12fps, white", function () {
    doc.width = 550; doc.height = 400; doc.frameRate = 12; doc.backgroundColor = "#FFFFFF";
    return doc.width + "x" + doc.height;
  });
  step("objectDrawingMode off (merge drawing)", function () { fl.objectDrawingMode = false; return fl.objectDrawingMode; });
  step("stroke template", initStroke);

  // ---- layer "back" (bottom): red rectangle, start of the overlap stack
  step("layer back: red rect", function () {
    tl.layers[0].name = "back";
    solidFill(doc, "#FF0000");
    doc.addNewRectangle({left: 260, top: 300, right: 400, bottom: 390}, 0, false, true);
    return layerInfo(tl, tl.currentLayer);
  });
  // ---- layer "mid": 60%-alpha green oval overlapping the red rect
  step("layer mid: green oval alpha 60%", function () {
    tl.addNewLayer("mid");
    solidFill(doc, "#00CC0099");
    doc.addNewOval({left: 320, top: 290, right: 440, bottom: 390}, false, true);
    return layerInfo(tl, tl.currentLayer);
  });
  // ---- layer "front": blue rounded rect with a 4px black stroke
  step("layer front: rounded rect + stroke", function () {
    tl.addNewLayer("front");
    solidFill(doc, "#0000FF");
    solidStroke("#000000", 4, "round", "round");
    doc.addNewRectangle({left: 380, top: 310, right: 520, bottom: 380}, 15, false, false);
    return layerInfo(tl, tl.currentLayer);
  });

  // ---- layer "holes": donut (no stroke) and a stroked square frame
  step("layer holes: donut", function () {
    tl.addNewLayer("holes");
    solidFill(doc, "#008080");
    doc.addNewOval({left: 300, top: 180, right: 400, bottom: 280}, false, true);
    solidFill(doc, "#FF00FF");
    doc.addNewOval({left: 325, top: 205, right: 375, bottom: 255}, false, true);
    doc.selectNone();
    doc.mouseClick({x: 350, y: 230}, false, true);
    var n = doc.selection.length;
    doc.deleteSelection();
    return "selected " + n + " before delete; " + layerInfo(tl, tl.currentLayer);
  });
  step("layer holes: square frame with strokes", function () {
    solidFill(doc, "#996633");
    solidStroke("#000000", 2, "round", "miter");
    doc.addNewRectangle({left: 420, top: 180, right: 520, bottom: 280}, 0, false, false);
    solidFill(doc, "#FFFFFF");
    doc.addNewRectangle({left: 445, top: 205, right: 495, bottom: 255}, 0, false, false);
    doc.selectNone();
    doc.mouseClick({x: 470, y: 230}, false, true);
    var n = doc.selection.length;
    doc.deleteSelection();
    doc.selectNone();
    return "selected " + n + " before delete; " + layerInfo(tl, tl.currentLayer);
  });

  // ---- layer "solid": plain filled shapes, ovals (curves), alpha fill
  step("layer solid: rect / stroked rect / stroked oval / alpha circle", function () {
    tl.addNewLayer("solid");
    solidFill(doc, "#3366CC");
    doc.addNewRectangle({left: 300, top: 20, right: 380, bottom: 80}, 0, false, true);
    solidFill(doc, "#FFCC00");
    solidStroke("#000000", 2, "round", "miter");
    doc.addNewRectangle({left: 400, top: 20, right: 480, bottom: 80}, 0, false, false);
    solidFill(doc, "#CC3366");
    solidStroke("#000000", 3, "round", "round");
    doc.addNewOval({left: 300, top: 100, right: 400, bottom: 160}, false, false);
    solidFill(doc, "#33AA3380");
    doc.addNewOval({left: 410, top: 100, right: 470, bottom: 160}, false, true);
    return layerInfo(tl, tl.currentLayer);
  });

  // ---- layer "joins": rectangle outlines and V polylines, 12px / 10px,
  // columns: miter (limit 3), round, bevel, miter with limit 1.5 (the V tip,
  // whose miter ratio is ~2.6, is cut back to a bevel; the 90-degree rect
  // corners, ratio 1.41, stay mitred)
  step("layer joins: rect outlines + V polylines", function () {
    tl.addNewLayer("joins");
    var joins = ["miter", "round", "bevel", "miter"], limits = [3, 3, 3, 1.5];
    for (var j = 0; j < 4; j++) {
      var x = 20 + j * 65;
      solidStroke("#336699", 12, "none", joins[j], limits[j]);
      doc.addNewRectangle({left: x, top: 150, right: x + 50, bottom: 200}, 0, true, false);
      solidStroke("#993366", 10, "none", joins[j], limits[j]);
      doc.addNewLine({x: x + 5, y: 300}, {x: x + 27, y: 240});
      doc.addNewLine({x: x + 27, y: 240}, {x: x + 49, y: 300});
    }
    return layerInfo(tl, tl.currentLayer);
  });

  // ---- layer "lines": rows = cap none / round / square, columns = 1, 4, 10, 20 px
  step("layer lines: caps x widths", function () {
    tl.addNewLayer("lines");
    var caps = ["none", "round", "square"], widths = [1, 4, 10, 20];
    for (var r = 0; r < 3; r++) {
      for (var c = 0; c < 4; c++) {
        var x = 20 + c * 70, y = 30 + r * 40;
        solidStroke("#000000", widths[c], caps[r], "round");
        doc.addNewLine({x: x, y: y}, {x: x + 45, y: y});
      }
    }
    return layerInfo(tl, tl.currentLayer);
  });

  // ---- layer "actions"
  step("layer actions: frame 1 trace", function () {
    tl.addNewLayer("actions");
    tl.layers[tl.currentLayer].frames[0].actionScript = 'trace("shapes-basic: frame 1");';
    return tl.layers[tl.currentLayer].frames[0].actionScript;
  });

  step("summary", function () {
    var s = "layers=" + tl.layers.length + " frames=" + tl.frameCount + " library=" + doc.library.items.length;
    for (var i = 0; i < tl.layers.length; i++) s += "\n     " + layerInfo(tl, i);
    return s;
  });
  step("export publish profile", function () { doc.exportPublishProfile(JZ_WORK + NAME + ".profile.xml"); });
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
