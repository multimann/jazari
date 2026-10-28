// Tween probe: graphics (run with tests/fixtures/tween-probes/builders/build.ps1)
// Which frame of a graphic symbol shows inside a motion tween and after
// keyframes. A 10-frame graphic Counter (bar 8*(n+1) px wide, one colour per
// frame) per layer, tweened from frame 0 to frame 15 (moved 100 px), with
// loop / play once / single frame, sync on and off, and a different first
// frame or loop mode at the end keyframe; plus static layers with a second
// keyframe.
var NAME = "graphics";
var OUT = JZ_OUT;
var LOG = JZ_LOG;
var FRAMES = 41;
var LAST = FRAMES - 1;
var FAILS = [];
FLfile.write(LOG, "");
function log(m) { FLfile.write(LOG, m + "\n", "append"); }
function step(label, fn) {
  try { var r = fn(); log("OK   " + label + (r === undefined ? "" : " -> " + r)); return true; }
  catch (e) { FAILS.push(label + ": " + e); log("FAIL " + label + ": " + e); return false; }
}
function solidFill(color) { var f = doc.getCustomFill(); f.style = "solid"; f.color = color; doc.setCustomFill(f); }
function noStroke() { var s = doc.getCustomStroke(); s.style = "noStroke"; doc.setCustomStroke(s); }
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
  var before = t.layers[L].frames[F].elements.length, els, tries = 0;
  do {
    tries++; useLayer(layer);
    if (tries < 4) doc.addItem({x: x, y: y}, findItem(item)); else doc.library.addItemToDocument({x: x, y: y}, item);
    els = t.layers[L].frames[F].elements;
  } while (els.length == before && tries < 4);
  if (els.length == before) throw new Error("could not place " + item + " on " + layer);
  return els[els.length - 1];
}
function key(layer, frame) { useLayer(layer); tl.insertKeyframe(frame); }
function elAt(layer, frame) {
  var L = useLayer(layer);
  tl.currentFrame = frame;
  return tl.layers[L].frames[frame].elements[0];
}
function selAt(layer, frame) {
  var L = useLayer(layer);
  tl.currentFrame = frame;
  var el = tl.layers[L].frames[frame].elements[0];
  doc.selectNone(); doc.selection = [el];
  return el;
}
var KEY = 15;
function info(el) { return el.symbolType + " loop=" + el.loop + " firstFrame=" + el.firstFrame; }
// A layer with a Counter at (x, y): loop mode and first frame at frame 0,
// then a keyframe at KEY moved 100 px right; end(el) may change it; a motion
// tween from 0 to KEY unless props is null.
function probe(name, x, y, loop, first, end, props) {
  return step("layer " + name, function () {
    newLayer(name);
    var el = place(name, "Counter", x, y);
    el.loop = loop; el.firstFrame = first;
    el = selAt(name, 0);
    var s = info(el);
    key(name, KEY);
    selAt(name, KEY);
    doc.moveSelectionBy({x: 100, y: 0});
    el = selAt(name, KEY);
    if (end) end(el);
    s += " -> " + info(selAt(name, KEY));
    if (props) {
      var L = useLayer(name);
      tl.createMotionTween(0, KEY - 1);
      var f = tl.layers[L].frames[0];
      f.motionTweenRotate = "auto"; f.motionTweenRotateTimes = 0; f.motionTweenScale = true; f.tweenEasing = 0;
      for (var k in props) f[k] = props[k];
      s += " tween=" + f.tweenType + " sync=" + f.motionTweenSync;
      s += " | after: end " + info(selAt(name, KEY));
    }
    return s;
  });
}
function afterReopen() {
  var t = doc.getTimeline(), s = "";
  for (var i = 0; i < t.layers.length; i++) {
    var fr = t.layers[i].frames;
    if (fr.length > KEY && fr[KEY].elements.length) s += " " + t.layers[i].name + "@" + KEY + ":" + info(fr[KEY].elements[0]);
  }
  return s;
}

var doc = null, tl = null;
try {
  FLfile.createFolder(OUT);
  doc = fl.createDocument();
  var lib = doc.library;
  step("stage 550x400 @ 12fps", function () {
    doc.width = 550; doc.height = 400; doc.frameRate = 12; doc.backgroundColor = "#FFFFFF";
    fl.objectDrawingMode = false;
    return doc.width + "x" + doc.height;
  });
  step("symbol Counter (graphic): 10 keyframes, bar width 8*(n+1) px, colour per frame", function () {
    lib.addNewItem("graphic", "Counter");
    lib.editItem("Counter");
    var t = doc.getTimeline();
    var cols = ["#E6194B", "#3CB44B", "#FFE119", "#4363D8", "#F58231", "#911EB4", "#46F0F0", "#F032E6", "#BCF60C", "#000000"];
    noStroke();
    for (var i = 0; i < 10; i++) {
      if (i > 0) t.insertBlankKeyframe(i);
      t.currentFrame = i;
      solidFill(cols[i]);
      doc.addNewRectangle({left: -40, top: -8, right: -40 + 8 * (i + 1), bottom: 8}, 0, false, true);
    }
    var s = "frameCount=" + t.frameCount;
    doc.exitEditMode();
    return s;
  });
  tl = doc.getTimeline();
  tl.layers[0].name = "base";
  var sync = {motionTweenSync: true}, nosync = {motionTweenSync: false};
  probe("g_loop", 60, 30, "loop", 0, null, nosync);
  probe("g_loop_sync", 60, 60, "loop", 0, null, sync);
  probe("g_loop_ff3", 60, 90, "loop", 3, null, nosync);
  probe("g_once", 60, 120, "play once", 3, null, nosync);
  probe("g_once_sync", 60, 150, "play once", 3, null, sync);
  probe("g_single", 60, 180, "single frame", 6, function (el) { el.firstFrame = 2; }, nosync);
  probe("g_loop_end5", 60, 210, "loop", 0, function (el) { el.firstFrame = 5; }, nosync);
  probe("g_loop_end5_sync", 60, 240, "loop", 0, function (el) { el.firstFrame = 5; }, sync);
  probe("g_loop_to_once", 60, 270, "loop", 2, function (el) { el.loop = "play once"; el.firstFrame = 8; }, nosync);
  probe("g_static", 60, 300, "loop", 0, null, null);
  probe("g_static_ff4", 60, 330, "loop", 1, function (el) { el.firstFrame = 4; }, null);
  step("extend every layer to " + FRAMES + " frames", function () {
    var s = [];
    for (var i = 0; i < tl.layers.length; i++) {
      var n = tl.layers[i].frameCount;
      if (n < FRAMES) { useLayer(tl.layers[i].name); tl.insertFrames(FRAMES - n, false, n); }
      s.push(tl.layers[i].name + "=" + tl.layers[i].frameCount);
    }
    return s.join(" ");
  });
  step("save FLA", function () { return fl.saveDocument(doc, OUT + NAME + ".fla"); });
  step("reopen saved FLA, export SWF from it", function () {
    fl.closeDocument(doc, false);
    doc = fl.openDocument(OUT + NAME + ".fla");
    var s = afterReopen();
    doc.exportSWF(OUT + "flash8.swf", true);
    return doc.name + " -> " + FLfile.exists(OUT + "flash8.swf") + s;
  });
} catch (e) { FAILS.push("top: " + e); log("ERROR: " + e); }
log("FAILED STEPS: " + FAILS.length);
for (var i = 0; i < FAILS.length; i++) log("  - " + FAILS[i]);
try { if (doc) fl.closeDocument(doc, false); } catch (e) { log("close: " + e); }
log("DONE");
fl.quit(false);
