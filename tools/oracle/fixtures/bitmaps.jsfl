// Fixture builder: bitmaps  (run with tools/oracle/build-all.ps1)
// Imports a PNG with transparency and a JPEG (both generated with Pillow into
// tests\fixtures\_src by make-images.py), places them plain and scaled+rotated, breaks
// a JPEG instance apart into a bitmap-filled shape, and makes a rotated
// bitmap-filled oval (break apart + cut with an oval stroke).
var NAME = "bitmaps";
var OUT = JZ_OUT;   // tests/fixtures/NAME/ (from the prelude build-all.ps1 prepends)
var SRC = JZ_SRC;   // tests/fixtures/_src/
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
    var v; try { v = o[k]; } catch (e) { v = "?"; }
    if (v && typeof v == "object" && v.length !== undefined) v = "[" + v.join(",") + "]";
    else if (v && typeof v == "object") v = dump(v);
    else if (typeof v == "number") v = Math.round(v * 10000) / 10000;
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
function layerIndex(t, name) {
  for (var i = 0; i < t.layers.length; i++) if (t.layers[i].name == name) return i;
  throw new Error("no layer " + name);
}
function useLayer(name) {
  var t = doc.getTimeline(), i = layerIndex(t, name);
  doc.selectNone(); t.setSelectedLayers(i, true); t.currentLayer = i;
  return i;
}
function newLayer(name) {
  var t = doc.getTimeline();
  doc.selectNone(); t.setSelectedLayers(0, true); t.currentLayer = 0;
  t.addNewLayer(name);
  return useLayer(name);
}
function findItem(name) {
  var it = doc.library.items;
  for (var i = 0; i < it.length; i++) if (it[i].name == name) return it[i];
  throw new Error("no library item " + name);
}
function place(layer, item, x, y) {
  var t = doc.getTimeline(), L = useLayer(layer), F = t.currentFrame;
  var before = t.layers[L].frames[F].elements.length, r, els, tries = 0;
  do {
    tries++; useLayer(layer);
    r = tries < 4 ? doc.addItem({x: x, y: y}, findItem(item)) : doc.library.addItemToDocument({x: x, y: y}, item);
    els = t.layers[L].frames[F].elements;
  } while (els.length == before && tries < 4);
  if (tries > 1) log("     (" + item + " needed " + tries + " placement attempt(s))");
  if (els.length == before) throw new Error("could not place " + item + " on " + layer);
  var el = els[els.length - 1];
  doc.selectNone(); doc.selection = [el];
  return el;
}
function itemInfo(it) {
  var ks = ["name", "itemType", "allowSmoothing", "compressionType", "quality", "useImportedJPEGQuality", "useDeblocking", "originalCompressionType", "sourceFilePath", "linkageExportForAS"];
  var s = "";
  for (var i = 0; i < ks.length; i++) { var v; try { v = it[ks[i]]; } catch (e) { v = "ERR"; } if (v !== undefined) s += ks[i] + "=" + v + " "; }
  return s;
}
function elInfo(el) { return el.elementType + "/" + (el.instanceType || "") + " " + (el.libraryItem ? el.libraryItem.name : "") + " matrix=" + dump(el.matrix); }

var doc = null, tl = null, PNG = "alpha-disc.png", JPG = "bars.jpg";
try {
  FLfile.createFolder(OUT);
  doc = fl.createDocument();
  tl = doc.getTimeline();
  step("stage 550x400 @ 12fps, light blue background (shows PNG alpha)", function () {
    doc.width = 550; doc.height = 400; doc.frameRate = 12; doc.backgroundColor = "#CCE0FF";
    fl.objectDrawingMode = false;
    tl.layers[0].name = "bitmaps";
    return doc.width + "x" + doc.height;
  });
  step("stroke template", initStroke);
  step("import PNG (alpha) and JPEG into the library", function () {
    doc.importFile(SRC + PNG, true);
    doc.importFile(SRC + JPG, true);
    var s = "library=" + doc.library.items.length;
    for (var i = 0; i < doc.library.items.length; i++) s += " | " + itemInfo(doc.library.items[i]);
    return s;
  });
  step("PNG plain, centre (90,90)", function () { return elInfo(place("bitmaps", PNG, 90, 90)); });
  step("PNG scaled 1.25 and rotated 30deg, centre (130,290)", function () {
    var el = place("bitmaps", PNG, 130, 290);
    doc.scaleSelection(1.25, 1.25);
    doc.rotateSelection(30);
    return elInfo(el);
  });
  step("JPEG plain, centre (300,80)", function () { return elInfo(place("bitmaps", JPG, 300, 80)); });
  step("JPEG scaled 0.75 and rotated -15deg, centre (300,250)", function () {
    var el = place("bitmaps", JPG, 300, 250);
    doc.scaleSelection(0.75, 0.75);
    doc.rotateSelection(-15);
    return elInfo(el);
  });
  var bmpFill = null;
  step("bitmap fill: JPEG instance at centre (465,80) broken apart into a shape", function () {
    newLayer("bitmapfill");
    place("bitmapfill", JPG, 465, 80);
    doc.breakApart();
    doc.selectNone();
    var L = useLayer("bitmapfill");
    var els = tl.layers[L].frames[0].elements;
    doc.mouseClick({x: 465, y: 80}, false, true);
    bmpFill = doc.getCustomFill("selection");
    doc.selectNone();
    return els.length + " element(s): " + els[0].elementType + " fill=" + dump(bmpFill);
  });
  // Flash 8's Fill object has no bitmap style (getCustomFill("selection") on a
  // bitmap-filled shape reports style "solid"), so a bitmap fill cannot be
  // assigned to a new shape.  Instead: break a JPEG apart, cut it with an oval
  // stroke (merge drawing), delete the outside region, then rotate the result.
  step("bitmap-filled oval: break apart JPEG at (470,285), cut by oval stroke, delete outside, rotate 20deg", function () {
    var L = newLayer("bitmapfill-oval");
    place("bitmapfill-oval", JPG, 470, 285);
    doc.breakApart();
    doc.selectNone();
    solidStroke("#000000", 2, "round", "round");
    doc.addNewOval({left: 400, top: 235, right: 540, bottom: 335}, true, false);
    doc.selectNone();
    doc.mouseClick({x: 393, y: 229}, false, true);
    var n = doc.selection.length;
    doc.deleteSelection();
    L = useLayer("bitmapfill-oval");
    var el = tl.layers[L].frames[0].elements[0];
    doc.selection = [el];
    doc.rotateSelection(20);
    doc.selectNone();
    el = tl.layers[L].frames[0].elements[0];
    return "outside region selected=" + n + "; " + tl.layers[L].frames[0].elements.length + " element(s): " + el.elementType +
           " edges=" + el.edges.length + " bounds=" + Math.round(el.left) + "," + Math.round(el.top) + " " + Math.round(el.width) + "x" + Math.round(el.height);
  });
  step("layer actions: frame 1 trace", function () {
    var L = newLayer("actions");
    tl.layers[L].frames[0].actionScript = 'trace("bitmaps: frame 1");';
  });
  step("summary", function () {
    var s = "layers=" + tl.layers.length + " frames=" + tl.frameCount + " library=" + doc.library.items.length;
    for (var i = 0; i < tl.layers.length; i++) {
      var els = tl.layers[i].frames[0].elements, n = [];
      for (var k = 0; k < els.length; k++) n.push(els[k].elementType + (els[k].instanceType ? "/" + els[k].instanceType : ""));
      s += "\n     " + tl.layers[i].name + ": " + n.join(", ");
    }
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
