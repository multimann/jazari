// Tween probe: ease (run with tests/fixtures/tween-probes/builders/build.ps1)
// One Arrow movie clip per layer, moved 400 px right (and turned, faded or
// scaled where noted) by a motion tween from frame 0 to frame 20. Covers the
// classic ease at several strengths and Flash 8's custom ease curves (one
// curve, two segments, a steep start). Per-property curves
// (useSingleEaseCurve = false) are left out: Flash 8 hung on them.
var NAME = "ease";
var OUT = JZ_OUT;
var LOG = JZ_LOG;
var FRAMES = 21;
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
function selAt(layer, frame) {
  var L = useLayer(layer);
  tl.currentFrame = frame;
  var el = tl.layers[L].frames[frame].elements[0];
  doc.selectNone(); doc.selection = [el];
  return el;
}
function setRaw(el, a, b, c, d, tx, ty) {
  var m = el.matrix; m.a = a; m.b = b; m.c = c; m.d = d; m.tx = tx; m.ty = ty; el.matrix = m;
}
function pts(p) {
  var s = [];
  for (var i = 0; i < p.length; i++) s.push("(" + p[i].x + "," + p[i].y + ")");
  return s.join(" ");
}
// A layer whose Arrow moves from (x, y) to (x + 400, y) over frames 0..LAST.
// end(el) may change the end keyframe's instance; setup(frame) the tween frame.
function probe(name, y, setup, end) {
  return step("layer " + name, function () {
    newLayer(name);
    var el = place(name, "Arrow", 60, y);
    setRaw(el, 1, 0, 0, 1, 60, y);
    key(name, LAST);
    selAt(name, LAST);
    doc.moveSelectionBy({x: 400, y: 0});
    if (end) end(selAt(name, LAST));
    el = selAt(name, LAST);
    var L = useLayer(name);
    tl.createMotionTween(0, LAST - 1);
    var f = tl.layers[L].frames[0];
    f.motionTweenRotate = "auto"; f.motionTweenRotateTimes = 0; f.motionTweenScale = true; f.tweenEasing = 0;
    var s = setup(f);
    return "end m=(" + el.matrix.a + "," + el.matrix.b + "," + el.matrix.c + "," + el.matrix.d + "," + el.matrix.tx + "," +
           el.matrix.ty + ") easing=" + f.tweenEasing + " hasCustomEase=" + f.hasCustomEase + " single=" + f.useSingleEaseCurve +
           (s ? " " + s : "");
  });
}
function classic(v) { return function (f) { f.tweenEasing = v; }; }
function custom(curves, single) {
  return function (f) {
    f.hasCustomEase = true;
    f.useSingleEaseCurve = single;
    var s = "";
    for (var k in curves) {
      f.setCustomEase(k, curves[k]);
      s += k + "=[" + pts(f.getCustomEase(k)) + "] ";
    }
    return s;
  };
}
var P = function (x, y) { return {x: x, y: y}; };

var doc = null, tl = null;
try {
  FLfile.createFolder(OUT);
  doc = fl.createDocument();
  var lib = doc.library;
  step("stage 550x600 @ 24fps", function () {
    doc.width = 550; doc.height = 600; doc.frameRate = 24; doc.backgroundColor = "#FFFFFF";
    fl.objectDrawingMode = false;
    return doc.width + "x" + doc.height;
  });
  step("symbol Arrow (movie clip)", function () {
    lib.addNewItem("movie clip", "Arrow");
    lib.editItem("Arrow");
    noStroke();
    solidFill("#3366CC"); doc.addNewRectangle({left: -30, top: -10, right: 20, bottom: 10}, 0, false, true);
    solidFill("#FF0000"); doc.addNewRectangle({left: 20, top: -5, right: 30, bottom: 5}, 0, false, true);
    doc.exitEditMode();
  });
  tl = doc.getTimeline();
  tl.layers[0].name = "base";
  probe("ease_m100", 30, classic(-100));
  probe("ease_m70", 60, classic(-70));
  probe("ease_m30", 90, classic(-30));
  probe("ease_p30", 120, classic(30));
  probe("ease_p70", 150, classic(70));
  probe("ease_p100", 180, classic(100));
  // Classic ease on rotation, alpha and scale together.
  probe("scale_m50", 440, classic(-50), function (el) { doc.scaleSelection(3, 3); });
  probe("scale_p50", 470, classic(50), function (el) { doc.scaleSelection(3, 3); });
  probe("scale_m100", 500, classic(-100), function (el) { doc.scaleSelection(3, 3); });
  probe("scale_p100", 530, classic(100), function (el) { doc.scaleSelection(3, 3); });
  probe("scale_custom", 560, custom({all: [P(0, 0), P(0.1, 0.4), P(0.2, 0.6), P(0.5, 0.5),
                                            P(0.7, 0.4), P(0.9, 0.9), P(1, 1)]}, true),
        function (el) { doc.scaleSelection(3, 3); });
  probe("ease_all", 210, classic(60), function (el) {
    doc.rotateSelection(90); doc.scaleSelection(2, 2); doc.setInstanceAlpha(20);
  });
  // One custom curve (one cubic segment), overshooting.
  probe("custom_one", 260, custom({all: [P(0, 0), P(0.1, 0.9), P(0.4, 1.2), P(1, 1)]}, true));
  // Two cubic segments.
  probe("custom_two", 290, custom({all: [P(0, 0), P(0.1, 0.4), P(0.2, 0.6), P(0.5, 0.5),
                                          P(0.7, 0.4), P(0.9, 0.9), P(1, 1)]}, true));
  // A curve with a steep start, to see how x is solved.
  probe("custom_steep", 320, custom({all: [P(0, 0), P(0, 1), P(0.2, 1), P(1, 1)]}, true));
  // A custom curve with classic easing also set (which wins?).
  probe("custom_and_classic", 400, function (f) {
    f.tweenEasing = 100;
    return custom({all: [P(0, 0), P(0.6, 0), P(0.9, 0.3), P(1, 1)]}, true)(f);
  });
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
    var t = doc.getTimeline(), s = "";
    for (var i = 0; i < t.layers.length; i++) {
      var f = t.layers[i].frames[0];
      if (f.hasCustomEase) s += " " + t.layers[i].name + ":all=[" + pts(f.getCustomEase("all")) + "]";
    }
    doc.exportSWF(OUT + "flash8.swf", true);
    return doc.name + " -> " + FLfile.exists(OUT + "flash8.swf") + s;
  });
} catch (e) { FAILS.push("top: " + e); log("ERROR: " + e); }
log("FAILED STEPS: " + FAILS.length);
for (var i = 0; i < FAILS.length; i++) log("  - " + FAILS[i]);
try { if (doc) fl.closeDocument(doc, false); } catch (e) { log("close: " + e); }
log("DONE");
fl.quit(false);
