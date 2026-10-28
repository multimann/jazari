// Fixture builder: masks  (run with tools/oracle/build-all.ps1 -Names masks)
// Mask layers as Flash 8 publishes them: a raw-shape mask over one masked
// layer; a mask over two masked layers; a mask whose content is a movie clip
// instance; a mask layer holding two elements (a raw shape and an instance);
// a motion-tweened mask (a movie clip); a masked layer holding a motion tween;
// a movie clip that contains its own mask over two masked layers; and a mask
// layer holding two movie clip instances.
// Stage 550x400 in 4 columns x 2 rows of 125x170 cells, 20 frames.
var NAME = "masks";
var OUT = JZ_OUT;   // tests/fixtures/NAME/ (from the prelude build-all.ps1 prepends)
var LOG = JZ_LOG;   // target/oracle/logs/NAME.log
var FRAMES = 20;
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
function rect(c, l, t, r, b, rad) { solidFill(c); doc.addNewRectangle({left: l, top: t, right: r, bottom: b}, rad || 0, false, true); }
function oval(c, l, t, r, b) { solidFill(c); doc.addNewOval({left: l, top: t, right: r, bottom: b}, false, true); }
// n stripes across the box, alternating through the colours.
function stripes(l, t, w, h, n, cols, vertical) {
  for (var i = 0; i < n; i++) {
    if (vertical) rect(cols[i % cols.length], l + i * w / n, t, l + (i + 1) * w / n, t + h);
    else rect(cols[i % cols.length], l, t + i * h / n, l + w, t + (i + 1) * h / n);
  }
}
// ---- tween helpers (see tweens.jsfl)
function sel(layer, frame) {
  var t = doc.getTimeline(), L = useLayer(layer);
  t.currentFrame = frame;
  var el = t.layers[L].frames[frame].elements[0];
  doc.selectNone(); doc.selection = [el];
  return el;
}
function key(layer, frame) { var t = doc.getTimeline(); useLayer(layer); t.insertKeyframe(frame); }
function tween(layer, start, end) {
  var t = doc.getTimeline(), L = useLayer(layer);
  t.createMotionTween(start, end);
  var f = t.layers[L].frames[start];
  return "frames " + start + "-" + end + " tweenType=" + f.tweenType;
}
// Make the named layer a mask and the listed layers (directly below it, top
// down) masked by it. Flash 8 attaches a "masked" layer to the mask above it.
function makeMask(maskName, maskedNames) {
  var t = doc.getTimeline(), s = "";
  var M = layerIndex(t, maskName);
  t.layers[M].layerType = "mask";
  for (var i = 0; i < maskedNames.length; i++) {
    var L = layerIndex(t, maskedNames[i]);
    t.layers[L].layerType = "masked";
    var p = null;
    try { p = t.layers[L].parentLayer; } catch (e) { p = "ERR " + e; }
    if (p !== undefined && p !== null && typeof p == "object") {
      if (p.name != maskName) { try { t.layers[L].parentLayer = t.layers[M]; } catch (e) { s += " (parentLayer set failed: " + e + ")"; } }
      p = t.layers[L].parentLayer.name;
    }
    s += " " + maskedNames[i] + ":" + t.layers[L].layerType + "/parent=" + p;
  }
  return maskName + ":" + t.layers[M].layerType + s;
}
function extendAll(n) {
  var t = doc.getTimeline(), s = [];
  for (var i = 0; i < t.layers.length; i++) {
    var c = t.layers[i].frameCount;
    if (c < n) { useLayer(t.layers[i].name); t.insertFrames(n - c, false, c); }
    s.push(t.layers[i].name + " " + c + "->" + t.layers[i].frameCount);
  }
  return s.join(", ");
}
function layerTable() {
  var t = doc.getTimeline(), s = "";
  for (var i = 0; i < t.layers.length; i++) {
    var ly = t.layers[i], p = "";
    try { p = ly.parentLayer ? " parent=" + ly.parentLayer.name : ""; } catch (e) { p = " parent=ERR"; }
    var els = ly.frames[0].elements, n = [];
    for (var k = 0; k < els.length; k++) n.push(els[k].elementType + (els[k].name ? ":" + els[k].name : ""));
    s += "\n     " + i + " " + ly.name + " type=" + ly.layerType + p + " frames=" + ly.frameCount + " [" + n.join(", ") + "]";
  }
  return s;
}

var STRIPES = ["#E6194B", "#3CB44B", "#FFE119", "#4363D8", "#F58231"];
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
  step("symbol Box (movie clip): 40x40, red/green/blue/black quadrants", function () {
    lib.addNewItem("movie clip", "Box");
    lib.editItem("Box");
    noStroke();
    rect("#FF0000", -20, -20, 0, 0); rect("#00CC00", 0, -20, 20, 0);
    rect("#0000FF", -20, 0, 0, 20); rect("#000000", 0, 0, 20, 20);
    doc.exitEditMode();
  });
  step("symbol Disc (movie clip): grey disc r=45", function () {
    lib.addNewItem("movie clip", "Disc");
    lib.editItem("Disc");
    noStroke();
    oval("#666666", -45, -45, 45, 45);
    doc.exitEditMode();
  });
  step("symbol MaskedClip (movie clip): mask oval over bars + Box inner_box_mc", function () {
    lib.addNewItem("movie clip", "MaskedClip");
    lib.editItem("MaskedClip");
    var t = doc.getTimeline();
    t.layers[0].name = "bars";
    noStroke();
    stripes(-60, -80, 120, 160, 8, ["#911EB4", "#46F0F0"], false);
    newLayer("box");
    place("box", "Box", 25, 30, "inner_box_mc");
    doc.scaleSelection(1.5, 1.5);
    newLayer("mask");
    oval("#000000", -50, -70, 50, 70);
    var s = makeMask("mask", ["box", "bars"]) + layerTable();
    doc.exitEditMode();
    return s;
  });
  tl = doc.getTimeline();

  // ---------------------------------------------------------------- A: one masked layer (col 0, row 0)
  step("A: a_art vertical stripes, a_mask raw oval", function () {
    tl.layers[0].name = "a_art";
    useLayer("a_art");
    stripes(10, 20, 125, 170, 5, STRIPES, true);
    newLayer("a_mask");
    oval("#000000", 20, 40, 125, 170);
  });
  // ---------------------------------------------------------------- B: two masked layers (col 1, row 0)
  step("B: b_art1 horizontal stripes, b_art2 Box b_box_mc x1.5, b_mask rounded rect", function () {
    newLayer("b_art1");
    stripes(145, 20, 125, 170, 6, STRIPES, false);
    newLayer("b_art2");
    place("b_art2", "Box", 245, 160, "b_box_mc");
    doc.scaleSelection(1.5, 1.5);
    newLayer("b_mask");
    rect("#000000", 160, 50, 255, 160, 20);
  });
  // ---------------------------------------------------------------- C: movie clip as mask (col 2, row 0)
  step("C: c_art grid, c_mask Disc maskclip_mc", function () {
    newLayer("c_art");
    for (var r = 0; r < 4; r++) for (var c = 0; c < 3; c++)
      rect(STRIPES[(r + c) % STRIPES.length], 280 + c * 125 / 3, 20 + r * 170 / 4, 280 + (c + 1) * 125 / 3, 20 + (r + 1) * 170 / 4);
    newLayer("c_mask");
    place("c_mask", "Disc", 342, 105, "maskclip_mc");
  });
  // ---------------------------------------------------------------- D: two elements on one mask layer (col 3, row 0)
  step("D: d_art stripes, d_mask raw rect + Disc maskdisc_mc x0.6", function () {
    newLayer("d_art");
    stripes(415, 20, 125, 170, 5, STRIPES, true);
    newLayer("d_mask");
    rect("#000000", 425, 35, 475, 95);
    place("d_mask", "Disc", 495, 140, "maskdisc_mc");
    doc.scaleSelection(0.6, 0.6);
  });
  // ---------------------------------------------------------------- E: motion-tweened mask (col 0, row 1)
  step("E: e_art grid, e_mask Disc tweenmask_mc x0.5 tweened (40,295) -> (105,295)", function () {
    newLayer("e_art");
    for (var r = 0; r < 4; r++) for (var c = 0; c < 3; c++)
      rect(STRIPES[(r + 2 * c) % STRIPES.length], 10 + c * 125 / 3, 210 + r * 170 / 4, 10 + (c + 1) * 125 / 3, 210 + (r + 1) * 170 / 4);
    newLayer("e_mask");
    place("e_mask", "Disc", 40, 295, "tweenmask_mc");
    doc.scaleSelection(0.5, 0.5);
    key("e_mask", FRAMES - 1); sel("e_mask", FRAMES - 1); doc.moveSelectionBy({x: 65, y: 0});
    return tween("e_mask", 0, FRAMES - 2);
  });
  // ---------------------------------------------------------------- F: masked layer holding a tween (col 1, row 1)
  step("F: f_art Box tweenbox_mc tweened (165,230) -> (250,360) x2, f_mask raw rect", function () {
    newLayer("f_art");
    place("f_art", "Box", 165, 230, "tweenbox_mc");
    key("f_art", FRAMES - 1); sel("f_art", FRAMES - 1); doc.moveSelectionBy({x: 85, y: 130}); doc.scaleSelection(2, 2);
    var s = tween("f_art", 0, FRAMES - 2);
    newLayer("f_mask");
    rect("#000000", 160, 250, 255, 350);
    return s;
  });
  // ---------------------------------------------------------------- G: masks inside a movie clip (col 2, row 1)
  step("G: g_clip MaskedClip maskedclip_mc at (342,295)", function () {
    newLayer("g_clip");
    var el = place("g_clip", "MaskedClip", 342, 295, "maskedclip_mc");
    return mat(el.matrix);
  });
  // ---------------------------------------------------------------- H: two instances on one mask layer (col 3, row 1)
  step("H: h_art stripes, h_mask Disc h_disc1_mc (placed first) + Disc h_disc2_mc (placed second), both x0.5", function () {
    newLayer("h_art");
    stripes(415, 210, 125, 170, 5, STRIPES, false);
    newLayer("h_mask");
    place("h_mask", "Disc", 450, 265, "h_disc1_mc");
    doc.scaleSelection(0.5, 0.5);
    place("h_mask", "Disc", 505, 330, "h_disc2_mc");
    doc.scaleSelection(0.5, 0.5);
  });
  // ---------------------------------------------------------------- scripts
  step("layer actions: frame 1 trace, frame 20 trace + stop()", function () {
    var L = newLayer("actions");
    tl.layers[L].frames[0].actionScript =
      'trace("masks: frame 1");\n' +
      'trace("maskclip_mc: " + typeof maskclip_mc + ", _x = " + maskclip_mc._x);\n' +
      'trace("maskdisc_mc._xscale = " + maskdisc_mc._xscale);\n' +
      'trace("tweenmask_mc._x = " + tweenmask_mc._x + ", tweenbox_mc._x = " + tweenbox_mc._x);\n' +
      'trace("maskedclip_mc.inner_box_mc: " + maskedclip_mc.inner_box_mc._name);\n' +
      'trace("h_disc1_mc: " + typeof h_disc1_mc + ", h_disc2_mc: " + typeof h_disc2_mc);\n';
    tl.insertBlankKeyframe(FRAMES - 1);
    tl.layers[L].frames[FRAMES - 1].actionScript =
      'trace("masks: frame " + _currentframe + ", tweenmask_mc._x = " + tweenmask_mc._x + ", tweenbox_mc._x = " + tweenbox_mc._x);\n' +
      'stop();\n';
    return "actions frameCount=" + tl.layers[L].frameCount;
  });
  step("extend every layer to " + FRAMES + " frames", function () { return extendAll(FRAMES) + "; timeline frameCount=" + tl.frameCount; });
  // ---------------------------------------------------------------- mask links last (content first)
  step("mask types: a_mask > a_art", function () { return makeMask("a_mask", ["a_art"]); });
  step("mask types: b_mask > b_art2, b_art1", function () { return makeMask("b_mask", ["b_art2", "b_art1"]); });
  step("mask types: c_mask > c_art", function () { return makeMask("c_mask", ["c_art"]); });
  step("mask types: d_mask > d_art", function () { return makeMask("d_mask", ["d_art"]); });
  step("mask types: e_mask > e_art", function () { return makeMask("e_mask", ["e_art"]); });
  step("mask types: f_mask > f_art", function () { return makeMask("f_mask", ["f_art"]); });
  step("mask types: h_mask > h_art", function () { return makeMask("h_mask", ["h_art"]); });
  step("summary", function () { return "layers=" + tl.layers.length + " frames=" + tl.frameCount + " library=" + lib.items.length + layerTable(); });
  step("save FLA", function () { return fl.saveDocument(doc, OUT + NAME + ".fla"); });
  // Publish from the saved file (MANIFEST J15).
  step("reopen saved FLA, export SWF from it", function () {
    fl.closeDocument(doc, false);
    doc = fl.openDocument(OUT + NAME + ".fla");
    doc.exportSWF(OUT + "flash8.swf", true);
    return doc.name + " -> " + FLfile.exists(OUT + "flash8.swf");
  });
  step("reopened document: layer types", function () { return layerTable(); });
} catch (e) { FAILS.push("top: " + e); log("ERROR: " + e); }
log("FAILED STEPS: " + FAILS.length);
for (var i = 0; i < FAILS.length; i++) log("  - " + FAILS[i]);
try { if (doc) fl.closeDocument(doc, false); } catch (e) { log("close: " + e); }
log("DONE");
fl.quit(false);
