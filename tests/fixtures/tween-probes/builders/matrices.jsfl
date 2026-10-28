// Tween probe: matrices (run with tests/fixtures/tween-probes/builders/build.ps1)
// One Arrow movie clip per layer, one motion tween per layer from frame 0 to
// frame 120 (121 frames), between exact matrices set with element.matrix.
// Covers the fine structure of Flash 8's rotation (small steps near 0, 45 and
// 90 degrees), full turns, scale with rotation, skew, mirroring, the "scale"
// option off and rotate modes with extra turns.
var NAME = "matrices";
var OUT = JZ_OUT;
var LOG = JZ_LOG;
var FRAMES = 121;
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
var RAD = Math.PI / 180;
// Matrix from scales, skews (degrees) and translation, Flash's convention:
// a = sx cos(skY), b = sx sin(skY), c = -sy sin(skX), d = sy cos(skX).
function setM(el, sx, sy, skx, sky, tx, ty) {
  var m = el.matrix;
  m.a = sx * Math.cos(sky * RAD); m.b = sx * Math.sin(sky * RAD);
  m.c = -sy * Math.sin(skx * RAD); m.d = sy * Math.cos(skx * RAD);
  m.tx = tx; m.ty = ty;
  el.matrix = m;
  var r = el.matrix;
  return "(" + r.a + "," + r.b + "," + r.c + "," + r.d + "," + r.tx + "," + r.ty + ")";
}
function setRaw(el, a, b, c, d, tx, ty) {
  var m = el.matrix; m.a = a; m.b = b; m.c = c; m.d = d; m.tx = tx; m.ty = ty; el.matrix = m;
  var r = el.matrix;
  return "(" + r.a + "," + r.b + "," + r.c + "," + r.d + "," + r.tx + "," + r.ty + ")";
}
// A layer with an Arrow tweened from frame 0 to frame LAST.
// from/to: [sx, sy, skx, sky]; props: frame properties of the tween.
function probe(name, x, y, from, to, props, raw) {
  return step("layer " + name, function () {
    newLayer(name);
    var el = place(name, "Arrow", x, y), s = "";
    s += raw ? setRaw(el, from[0], from[1], from[2], from[3], x, y) : setM(el, from[0], from[1], from[2], from[3], x, y);
    key(name, LAST);
    el = elAt(name, LAST);
    s += " -> " + (raw ? setRaw(el, to[0], to[1], to[2], to[3], x + 20, y + 10) : setM(el, to[0], to[1], to[2], to[3], x + 20, y + 10));
    var L = useLayer(name);
    tl.createMotionTween(0, LAST - 1);
    var f = tl.layers[L].frames[0];
    for (var k in props) f[k] = props[k];
    return s + " tween=" + f.tweenType + " rotate=" + f.motionTweenRotate + "x" + f.motionTweenRotateTimes +
           " easing=" + f.tweenEasing + " scale=" + f.motionTweenScale;
  });
}

var doc = null, tl = null;
try {
  FLfile.createFolder(OUT);
  doc = fl.createDocument();
  var lib = doc.library;
  step("stage 800x600 @ 24fps", function () {
    doc.width = 800; doc.height = 600; doc.frameRate = 24; doc.backgroundColor = "#FFFFFF";
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
  var auto = {motionTweenRotate: "auto", motionTweenRotateTimes: 0, tweenEasing: 0, motionTweenScale: true};
  probe("r0_6", 60, 50, [1, 1, 0, 0], [1, 1, 6, 6], auto);
  probe("r42_48", 160, 50, [1, 1, 42, 42], [1, 1, 48, 48], auto);
  probe("r84_90", 260, 50, [1, 1, 84, 84], [1, 1, 90, 90], auto, false);
  probe("r0_90", 360, 50, [1, 1, 0, 0], [1, 1, 90, 90], auto);
  probe("r0_m90", 460, 50, [1, 1, 0, 0], [1, 1, -90, -90], auto);
  probe("r_cw360", 560, 50, [1, 1, 0, 0], [1, 1, 0, 0],
        {motionTweenRotate: "clockwise", motionTweenRotateTimes: 1, tweenEasing: 0, motionTweenScale: true});
  probe("r10_100", 660, 50, [1, 1, 10, 10], [1, 1, 100, 100], auto);
  probe("scale_rot", 60, 200, [1, 1, 0, 0], [2, 2, 90, 90], auto);
  probe("skew_x", 160, 200, [1, 1, 0, 0], [1, 1, 40, 0], auto);
  probe("skew_y", 260, 200, [1, 1, 0, 0], [1, 1, 0, 40], auto);
  probe("skew_xy", 360, 200, [1.5, 0.75, 20, -10], [0.8, 1.2, -30, 50], auto);
  probe("mirror", 460, 200, [-1, 0, 0, 1], [0, 1, -1, 0], auto, true);
  probe("no_scale", 560, 200, [1.5, 1.5, 30, 30], [0.5, 0.5, 120, 120],
        {motionTweenRotate: "auto", motionTweenRotateTimes: 0, tweenEasing: 0, motionTweenScale: false});
  probe("aniso", 660, 200, [2, 0.5, 0, 0], [0.5, 2, 60, 60], auto);
  probe("ccw_long", 60, 350, [1, 1, 10, 10], [1, 1, 20, 20],
        {motionTweenRotate: "counter-clockwise", motionTweenRotateTimes: 0, tweenEasing: 0, motionTweenScale: true});
  probe("cw_times2", 160, 350, [1, 1, 0, 0], [1, 1, 90, 90],
        {motionTweenRotate: "clockwise", motionTweenRotateTimes: 2, tweenEasing: 0, motionTweenScale: true});
  probe("none_rot", 260, 350, [1, 1, 0, 0], [1, 1, 90, 90],
        {motionTweenRotate: "none", motionTweenRotateTimes: 0, tweenEasing: 0, motionTweenScale: true});
  probe("r170_m170", 360, 350, [1, 1, 170, 170], [1, 1, -170, -170], auto);
  probe("eased_rot", 460, 350, [1, 1, 0, 0], [1, 1, 90, 90],
        {motionTweenRotate: "auto", motionTweenRotateTimes: 0, tweenEasing: -40, motionTweenScale: true});
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
    doc.exportSWF(OUT + "flash8.swf", true);
    return doc.name + " -> " + FLfile.exists(OUT + "flash8.swf");
  });
} catch (e) { FAILS.push("top: " + e); log("ERROR: " + e); }
log("FAILED STEPS: " + FAILS.length);
for (var i = 0; i < FAILS.length; i++) log("  - " + FAILS[i]);
try { if (doc) fl.closeDocument(doc, false); } catch (e) { log("close: " + e); }
log("DONE");
fl.quit(false);
