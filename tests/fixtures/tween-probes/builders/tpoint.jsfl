// Tween probe: tpoint (run with tests/fixtures/tween-probes/builders/build.ps1)
// Where the transformation point goes in a motion tween. One movie clip per
// layer, tweened from frame 0 to frame 30 with rotateSelection,
// scaleSelection and moveSelectionBy (which keep the transformation point on
// the instance). Arrow is centred on its registration point; Corner's
// registration point is its top-left corner, so its transformation point
// (the centre of its bounds when placed) is off the registration point.
// Flash 8 JSFL has no get/setTransformationPoint (CS3+).
var NAME = "tpoint";
var OUT = JZ_OUT;
var LOG = JZ_LOG;
var FRAMES = 31;
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
function desc(el) {
  var m = el.matrix;
  return "m=(" + m.a + "," + m.b + "," + m.c + "," + m.d + "," + m.tx + "," + m.ty + ")";
}
// A layer with an Arrow at (x, y); start(el) and end(el) shape the two keyframes.
function probe(name, item, x, y, start, end, props) {
  return step("layer " + name, function () {
    newLayer(name);
    var el = place(name, item, x, y);
    doc.selectNone(); doc.selection = [el];
    if (start) start(el);
    var s = desc(selAt(name, 0));
    key(name, LAST);
    el = selAt(name, LAST);
    end(el);
    s += " -> " + desc(selAt(name, LAST));
    var L = useLayer(name);
    tl.createMotionTween(0, LAST - 1);
    var f = tl.layers[L].frames[0];
    f.motionTweenRotate = "auto"; f.motionTweenRotateTimes = 0; f.motionTweenScale = true; f.tweenEasing = 0;
    for (var k in props) f[k] = props[k];
    return s + " easing=" + f.tweenEasing;
  });
}

var doc = null, tl = null;
try {
  FLfile.createFolder(OUT);
  doc = fl.createDocument();
  var lib = doc.library;
  step("stage 550x400 @ 24fps", function () {
    doc.width = 550; doc.height = 400; doc.frameRate = 24; doc.backgroundColor = "#FFFFFF";
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
  step("symbol Corner (movie clip): registration point at the top-left corner", function () {
    lib.addNewItem("movie clip", "Corner");
    lib.editItem("Corner");
    noStroke();
    solidFill("#33AA66"); doc.addNewRectangle({left: 0, top: 0, right: 60, bottom: 20}, 0, false, true);
    solidFill("#FF0000"); doc.addNewRectangle({left: 60, top: 5, right: 70, bottom: 15}, 0, false, true);
    doc.exitEditMode();
  });
  tl = doc.getTimeline();
  tl.layers[0].name = "base";
  var rot = function () { doc.rotateSelection(90); doc.moveSelectionBy({x: 150, y: 60}); };
  probe("tp_arrow_rot", "Arrow", 60, 60, null, rot);
  probe("tp_corner_rot", "Corner", 60, 160, null, rot);
  probe("tp_corner_scale", "Corner", 60, 260, null, function () { doc.scaleSelection(2, 0.5); doc.moveSelectionBy({x: 150, y: 60}); });
  probe("tp_corner_ease", "Corner", 300, 60, null, rot, {tweenEasing: -100});
  probe("tp_corner_cw", "Corner", 300, 160, null, function () {
    doc.rotateSelection(45); doc.scaleSelection(1.5, 1.5); doc.moveSelectionBy({x: 100, y: 0});
  }, {motionTweenRotate: "clockwise", motionTweenRotateTimes: 1});
  probe("tp_corner_rot_tl", "Corner", 300, 260, null, function () {
    doc.rotateSelection(90, "top left"); doc.moveSelectionBy({x: 150, y: 60});
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
    doc.exportSWF(OUT + "flash8.swf", true);
    return doc.name + " -> " + FLfile.exists(OUT + "flash8.swf");
  });
} catch (e) { FAILS.push("top: " + e); log("ERROR: " + e); }
log("FAILED STEPS: " + FAILS.length);
for (var i = 0; i < FAILS.length; i++) log("  - " + FAILS[i]);
try { if (doc) fl.closeDocument(doc, false); } catch (e) { log("close: " + e); }
log("DONE");
fl.quit(false);
