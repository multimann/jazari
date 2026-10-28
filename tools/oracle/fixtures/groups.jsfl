// Fixture builder: groups  (run with tools/oracle/build-all.ps1 -Names groups)
// Raw shapes grouped; groups moved, scaled, rotated and skewed after grouping;
// three levels of nested groups (the innermost holding a named movie clip);
// a group holding a scaled and rotated movie clip instance; two groups, a
// rotated drawing object and a raw shape stacked on one layer; a group (with an
// instance) inside a movie clip. Every group gets its own colours so that the SWF fill styles can be
// matched to the XFL shapes. Purpose: show how the XFL that JPEXS writes stores
// group member matrices, against the SWF that Flash 8 publishes.
var NAME = "groups";
var OUT = JZ_OUT;   // tests/fixtures/NAME/ (from the prelude build-all.ps1 prepends)
var LOG = JZ_LOG;   // target/oracle/logs/NAME.log
var FAILS = [];
FLfile.write(LOG, "");
function log(m) { FLfile.write(LOG, m + "\n", "append"); }
function step(label, fn) {
  try { var r = fn(); log("OK   " + label + (r === undefined ? "" : " -> " + r)); return true; }
  catch (e) { FAILS.push(label + ": " + e); log("FAIL " + label + ": " + e); return false; }
}
function r4(v) { return Math.round(v * 10000) / 10000; }
function mat(m) {
  if (!m) return String(m);
  return "(a=" + r4(m.a) + " b=" + r4(m.b) + " c=" + r4(m.c) + " d=" + r4(m.d) + " tx=" + r4(m.tx) + " ty=" + r4(m.ty) + ")";
}
function solidFill(color) { var f = doc.getCustomFill(); f.style = "solid"; f.color = color; doc.setCustomFill(f); }
function noStroke() { var s = doc.getCustomStroke(); s.style = "noStroke"; doc.setCustomStroke(s); }
// ---- layer helpers (see symbols.jsfl; MANIFEST J11, J12)
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
// Lock every layer but the named one, so that selectAll() takes only its content.
function solo(name) {
  var t = doc.getTimeline(), L = useLayer(name);
  for (var i = 0; i < t.layers.length; i++) t.layers[i].locked = (i != L);
  return L;
}
function unlockAll() {
  var t = doc.getTimeline();
  for (var i = 0; i < t.layers.length; i++) t.layers[i].locked = false;
}
function findItem(name) {
  var it = doc.library.items;
  for (var i = 0; i < it.length; i++) if (it[i].name == name) return it[i];
  throw new Error("no library item " + name);
}
function place(layer, item, x, y, name) {
  var t = doc.getTimeline(), L = useLayer(layer), F = t.currentFrame;
  var before = t.layers[L].frames[F].elements.length, r, els, tries = 0;
  do {
    tries++; useLayer(layer);
    r = tries < 4 ? doc.addItem({x: x, y: y}, findItem(item)) : doc.library.addItemToDocument({x: x, y: y}, item);
    els = t.layers[L].frames[F].elements;
  } while (els.length == before && tries < 4);
  if (tries > 1) log("     (" + item + " needed " + tries + " placement attempt(s))");
  if (els.length == before) throw new Error("could not place " + item + " on " + layer);
  // The new instance is the unnamed one (raw shapes may share the layer).
  var el = null;
  for (var i = els.length - 1; i >= 0 && !el; i--) if (els[i].elementType == "instance" && els[i].name == "") el = els[i];
  if (!el) throw new Error("placed " + item + " but found no unnamed instance on " + layer);
  if (name) el.name = name;
  doc.selectNone(); doc.selection = [el];
  return el;
}
// The single group on the current frame of a layer (the last one found).
function groupOn(L) {
  var t = doc.getTimeline(), els = t.layers[L].frames[t.currentFrame].elements, g = null;
  for (var i = 0; i < els.length; i++) if (els[i].elementType == "shape" && els[i].isGroup) g = els[i];
  if (!g) throw new Error("no group on layer " + t.layers[L].name);
  return g;
}
// Group everything on the (soloed) layer and return the new group.
function groupAll(name) {
  var L = solo(name);
  doc.selectAll();
  var n = doc.selection.length;
  doc.group();
  var g = groupOn(L);
  doc.selectNone(); doc.selection = [g];
  return {L: L, g: g, n: n};
}
function describe(el, ind) {
  var s = ind + el.elementType + (el.isGroup ? "(group)" : "") + (el.name ? " name=" + el.name : "") +
          (el.libraryItem ? " item=" + el.libraryItem.name : "") + " matrix=" + mat(el.matrix);
  try { s += " box=(" + r4(el.left) + "," + r4(el.top) + " " + r4(el.width) + "x" + r4(el.height) + ")"; } catch (e) { }
  var m = null;
  try { m = el.members; } catch (e) { s += " members:ERR " + e; }
  if (m) for (var i = 0; i < m.length; i++) s += "\n" + describe(m[i], ind + "    ");
  return s;
}
function layerReport(L) {
  var t = doc.getTimeline(), els = t.layers[L].frames[0].elements, s = t.layers[L].name + ": " + els.length + " element(s)";
  for (var i = 0; i < els.length; i++) s += "\n" + describe(els[i], "       ");
  return s;
}
function rect(c, l, t, r, b) { solidFill(c); doc.addNewRectangle({left: l, top: t, right: r, bottom: b}, 0, false, true); }
function oval(c, l, t, r, b) { solidFill(c); doc.addNewOval({left: l, top: t, right: r, bottom: b}, false, true); }

var doc = null, tl = null;
try {
  FLfile.createFolder(OUT);
  doc = fl.createDocument();
  var lib = doc.library;
  step("stage 550x400 @ 12fps", function () {
    doc.width = 550; doc.height = 400; doc.frameRate = 12; doc.backgroundColor = "#FFFFFF";
    fl.objectDrawingMode = false;
    noStroke();
    return doc.width + "x" + doc.height;
  });

  // ---------------------------------------------------------------- library
  step("symbol Marker (movie clip): blue body (-20,-10)-(12,10), red tip (12,-5)-(20,5)", function () {
    lib.addNewItem("movie clip", "Marker");
    lib.editItem("Marker");
    noStroke();
    rect("#3366CC", -20, -10, 12, 10);
    rect("#FF0000", 12, -5, 20, 5);
    doc.exitEditMode();
  });
  step("symbol GroupClip (movie clip): group of rect + oval + Marker inclip_mc, rotated 30", function () {
    lib.addNewItem("movie clip", "GroupClip");
    lib.editItem("GroupClip");
    var t = doc.getTimeline();
    t.layers[0].name = "art";
    noStroke();
    rect("#9966CC", -40, -20, 20, 20);
    oval("#FFCC00", 25, -15, 45, 5);
    place("art", "Marker", 0, 30, "inclip_mc");
    var r = groupAll("art");
    doc.rotateSelection(30);
    var s = "grouped " + r.n + " -> " + layerReport(r.L);
    doc.exitEditMode();
    return s;
  });
  tl = doc.getTimeline();

  // ---------------------------------------------------------------- one group per layer, bottom up
  step("layer plain: rect #FF0000 + oval #00AA00, grouped, not transformed", function () {
    tl.layers[0].name = "plain";
    useLayer("plain");
    rect("#FF0000", 20, 40, 70, 80);
    oval("#00AA00", 75, 45, 115, 85);
    var r = groupAll("plain");
    return "grouped " + r.n + " -> " + layerReport(r.L);
  });
  step("layer moved: rect #0066FF + oval #FF9900, grouped, moved by (10,20)", function () {
    newLayer("moved");
    rect("#0066FF", 150, 30, 200, 70);
    oval("#FF9900", 205, 35, 245, 75);
    var r = groupAll("moved");
    doc.moveSelectionBy({x: 10, y: 20});
    return "grouped " + r.n + " -> " + layerReport(r.L);
  });
  step("layer scaled: rect #CC0066 + oval #66CC00, grouped, scaled (1.5, 0.75)", function () {
    newLayer("scaled");
    rect("#CC0066", 290, 40, 330, 80);
    oval("#66CC00", 335, 45, 375, 85);
    var r = groupAll("scaled");
    doc.scaleSelection(1.5, 0.75);
    return "grouped " + r.n + " -> " + layerReport(r.L);
  });
  step("layer rotated: rect #009999 + oval #996600, grouped, rotated 30", function () {
    newLayer("rotated");
    rect("#009999", 430, 40, 480, 80);
    oval("#996600", 485, 45, 525, 85);
    var r = groupAll("rotated");
    doc.rotateSelection(30);
    return "grouped " + r.n + " -> " + layerReport(r.L);
  });
  step("layer skewed: rect #6633CC + oval #CCCC00, grouped, skewed (20, -10)", function () {
    newLayer("skewed");
    rect("#6633CC", 20, 160, 70, 200);
    oval("#CCCC00", 75, 165, 115, 205);
    var r = groupAll("skewed");
    doc.skewSelection(20, -10);
    return "grouped " + r.n + " -> " + layerReport(r.L);
  });
  step("layer nested: N1 = rect #FF3333 + Marker nested_mc, rotated 45", function () {
    newLayer("nested");
    rect("#FF3333", 170, 150, 210, 170);
    place("nested", "Marker", 230, 160, "nested_mc");
    var r = groupAll("nested");
    doc.rotateSelection(45);
    return "grouped " + r.n + " -> " + layerReport(r.L);
  });
  step("layer nested: N2 = N1 + oval #3333FF, scaled (1.25, 0.8)", function () {
    useLayer("nested");
    unlockAll();
    useLayer("nested");
    oval("#3333FF", 250, 140, 290, 180);
    var r = groupAll("nested");
    doc.scaleSelection(1.25, 0.8);
    return "grouped " + r.n + " -> " + layerReport(r.L);
  });
  step("layer nested: N3 = N2 + rect #33CC33, moved (5,5) and rotated -15", function () {
    unlockAll();
    useLayer("nested");
    rect("#33CC33", 160, 200, 300, 215);
    var r = groupAll("nested");
    doc.moveSelectionBy({x: 5, y: 5});
    doc.rotateSelection(-15);
    return "grouped " + r.n + " -> " + layerReport(r.L);
  });
  step("layer symbol: Marker ingroup_mc scaled 1.5 and rotated 90, + rect #999999, grouped, rotated 30, moved (0,10)", function () {
    unlockAll();
    newLayer("symbol");
    rect("#999999", 400, 150, 420, 210);
    var el = place("symbol", "Marker", 470, 180, "ingroup_mc");
    doc.scaleSelection(1.5, 1.5);
    doc.rotateSelection(90);
    var before = "instance before grouping " + mat(el.matrix);
    var r = groupAll("symbol");
    doc.rotateSelection(30);
    doc.moveSelectionBy({x: 0, y: 10});
    return before + "; grouped " + r.n + " -> " + layerReport(r.L);
  });
  step("layer stack: group Ga (rect #FF00FF), group Gb (oval #00FFFF, moved (10,0)), drawing object (oval #FFAA00, rotated 20), raw rect #DDDDDD drawn last", function () {
    unlockAll();
    newLayer("stack");
    rect("#FF00FF", 40, 290, 140, 370);
    var a = groupAll("stack");
    unlockAll(); useLayer("stack");
    oval("#00FFFF", 100, 300, 200, 380);
    // group only the new raw oval: lock nothing else, select it by clicking
    solo("stack");
    doc.selectNone();
    doc.mouseClick({x: 150, y: 375}, false, true);
    var picked = doc.selection.length;
    doc.group();
    doc.moveSelectionBy({x: 10, y: 0});
    // A Flash 8 drawing object (object drawing mode), rotated 20 after drawing.
    unlockAll(); useLayer("stack");
    fl.objectDrawingMode = true;
    oval("#FFAA00", 150, 280, 250, 330);
    fl.objectDrawingMode = false;
    var els = tl.layers[layerIndex(tl, "stack")].frames[0].elements, dobj = els[els.length - 1];
    solo("stack");
    doc.selectNone(); doc.selection = [dobj];
    doc.rotateSelection(20);
    var dinfo = "drawing object isDrawingObject=" + dobj.isDrawingObject + " " + mat(dobj.matrix);
    unlockAll(); useLayer("stack");
    rect("#DDDDDD", 20, 270, 260, 390);
    return "picked " + picked + "; " + dinfo + " -> " + layerReport(layerIndex(tl, "stack"));
  });
  step("layer clip: GroupClip clip_mc at (340,330), scaled 0.8, rotated 10", function () {
    unlockAll();
    newLayer("clip");
    var el = place("clip", "GroupClip", 340, 330, "clip_mc");
    doc.scaleSelection(0.8, 0.8);
    doc.rotateSelection(10);
    return mat(el.matrix);
  });
  step("layer actions: frame 1 script", function () {
    unlockAll();
    newLayer("actions");
    tl.layers[layerIndex(tl, "actions")].frames[0].actionScript =
      'function r2(n) {\n\treturn Math.round(n * 100) / 100;\n}\n' +
      'function info(mc) {\n\treturn mc._name + ": x=" + r2(mc._x) + " y=" + r2(mc._y) + " xscale=" + r2(mc._xscale) +\n' +
      '\t\t" yscale=" + r2(mc._yscale) + " rotation=" + r2(mc._rotation);\n}\n' +
      'trace("groups: frame 1");\n' +
      'trace(info(ingroup_mc));\n' +
      'trace(info(nested_mc));\n' +
      'trace(info(clip_mc));\n' +
      'trace(info(clip_mc.inclip_mc));\n';
    return "ok";
  });
  step("summary", function () {
    unlockAll();
    var s = "layers=" + tl.layers.length + " frames=" + tl.frameCount + " library=" + lib.items.length;
    for (var i = 0; i < tl.layers.length; i++) s += "\n     " + layerReport(i);
    return s;
  });
  step("save FLA", function () { return fl.saveDocument(doc, OUT + NAME + ".fla"); });
  // Publish from the saved file (MANIFEST J15).
  step("reopen saved FLA, export SWF from it", function () {
    fl.closeDocument(doc, false);
    doc = fl.openDocument(OUT + NAME + ".fla");
    doc.exportSWF(OUT + "flash8.swf", true);
    return doc.name + " -> " + FLfile.exists(OUT + "flash8.swf");
  });
  step("reopened document: group matrices as Flash reports them", function () {
    var t = doc.getTimeline(), s = "";
    for (var i = 0; i < t.layers.length; i++) s += "\n     " + (function (L) {
      var els = t.layers[L].frames[0].elements, r = t.layers[L].name + ": " + els.length + " element(s)";
      for (var k = 0; k < els.length; k++) r += "\n" + describe(els[k], "       ");
      return r;
    })(i);
    return s;
  });
} catch (e) { FAILS.push("top: " + e); log("ERROR: " + e); }
log("FAILED STEPS: " + FAILS.length);
for (var i = 0; i < FAILS.length; i++) log("  - " + FAILS[i]);
try { if (doc) fl.closeDocument(doc, false); } catch (e) { log("close: " + e); }
log("DONE");
fl.quit(false);
