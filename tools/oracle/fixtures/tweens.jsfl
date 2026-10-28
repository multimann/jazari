// Fixture builder: tweens  (run with tools/oracle/build-all.ps1)
// Motion tweens: rotation CW / CCW / auto, scale, easing in and out, alpha,
// several consecutive tweens on one layer, and a 10-frame graphic symbol
// placed with loop / play once / single frame and firstFrame offsets.
var NAME = "tweens";
var OUT = JZ_OUT;   // tests/fixtures/NAME/ (from the prelude build-all.ps1 prepends)
var LOG = JZ_LOG;   // target/oracle/logs/NAME.log
var FRAMES = 60;
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
  for (var k in o) { var v = o[k]; if (v && typeof v == "object") v = dump(v); s += k + ":" + (typeof v == "number" ? Math.round(v * 1000) / 1000 : v) + " "; }
  return s + "}";
}
function solidFill(color) { var f = doc.getCustomFill(); f.style = "solid"; f.color = color; doc.setCustomFill(f); }
function noStroke() { var s = doc.getCustomStroke(); s.style = "noStroke"; doc.setCustomStroke(s); }
// ---- layer helpers (see symbols.jsfl: selecting elements moves the current layer)
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
  var el = els[els.length - 1];
  if (name) el.name = name;
  doc.selectNone(); doc.selection = [el];
  return el;
}
// ---- tween helpers
function key(layer, frame) { useLayer(layer); tl.insertKeyframe(frame); }
function sel(layer, frame) {
  var L = useLayer(layer);
  tl.currentFrame = frame;
  var el = tl.layers[L].frames[frame].elements[0];
  doc.selectNone(); doc.selection = [el];
  return el;
}
function tween(layer, start, end, props) {
  var L = useLayer(layer);
  tl.createMotionTween(start, end);
  var f = tl.layers[L].frames[start];
  for (var k in props) f[k] = props[k];
  return "frames " + start + "-" + end + " tweenType=" + f.tweenType + " rotate=" + f.motionTweenRotate + "x" +
         f.motionTweenRotateTimes + " easing=" + f.tweenEasing + " scale=" + f.motionTweenScale;
}
function extend(layer) {
  var L = useLayer(layer), n = tl.layers[L].frameCount;
  if (n < FRAMES) tl.insertFrames(FRAMES - n, false, n);
  return layer + " " + n + " -> " + tl.layers[L].frameCount;
}

var doc = null, tl = null;
try {
  FLfile.createFolder(OUT);
  doc = fl.createDocument();
  var lib = doc.library;
  step("stage 550x400 @ 24fps", function () {
    doc.width = 550; doc.height = 400; doc.frameRate = 24; doc.backgroundColor = "#FFFFFF";
    fl.objectDrawingMode = false;
    return doc.width + "x" + doc.height + " @" + doc.frameRate;
  });

  // ---------------------------------------------------------------- library
  step("symbol Arrow (movie clip): blue body + red tip, registration centre", function () {
    lib.addNewItem("movie clip", "Arrow");
    lib.editItem("Arrow");
    noStroke();
    solidFill("#3366CC"); doc.addNewRectangle({left: -30, top: -10, right: 20, bottom: 10}, 0, false, true);
    solidFill("#FF0000"); doc.addNewRectangle({left: 20, top: -5, right: 30, bottom: 5}, 0, false, true);
    doc.exitEditMode();
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
    var s = "frameCount=" + t.frameCount + " keyframe0 elements=" + t.layers[0].frames[0].elements.length +
            " keyframe9 elements=" + t.layers[0].frames[9].elements.length;
    doc.exitEditMode();
    return s;
  });
  tl = doc.getTimeline();

  // ---------------------------------------------------------------- graphics layer (bottom)
  step("layer graphics: Counter x4 (loop ff0, loop ff7, play once ff3, single frame ff6)", function () {
    tl.layers[0].name = "graphics";
    var specs = [[70, "loop", 0], [200, "loop", 7], [330, "play once", 3], [460, "single frame", 6]], s = "";
    for (var i = 0; i < specs.length; i++) {
      var el = place("graphics", "Counter", specs[i][0], 375);
      el.loop = specs[i][1]; el.firstFrame = specs[i][2];
      s += " [" + el.symbolType + " loop=" + el.loop + " firstFrame=" + el.firstFrame + "]";
    }
    return s;
  });

  // ---------------------------------------------------------------- consecutive tweens on one layer
  step("layer consecutive: keys 0/15/30/45, tweens with easing 0/+50/-50", function () {
    newLayer("consecutive");
    place("consecutive", "Arrow", 60, 250);
    key("consecutive", 15); sel("consecutive", 15); doc.moveSelectionBy({x: 140, y: 0});
    key("consecutive", 30); sel("consecutive", 30); doc.moveSelectionBy({x: 0, y: 70}); doc.rotateSelection(90);
    key("consecutive", 45); sel("consecutive", 45); doc.moveSelectionBy({x: -140, y: 0}); doc.rotateSelection(90);
    return tween("consecutive", 0, 14, {tweenEasing: 0}) + " | " + tween("consecutive", 15, 29, {tweenEasing: 50}) +
           " | " + tween("consecutive", 30, 44, {tweenEasing: -50});
  });

  // ---------------------------------------------------------------- alpha
  step("layer alpha: alpha 100 -> 0 (frame 30) -> 100 (frame 60)", function () {
    newLayer("alpha");
    place("alpha", "Arrow", 330, 150);
    key("alpha", 29); sel("alpha", 29); doc.setInstanceAlpha(0);
    key("alpha", 59); sel("alpha", 59); doc.setInstanceAlpha(100);
    return tween("alpha", 0, 28, {}) + " | " + tween("alpha", 29, 58, {});
  });

  // ---------------------------------------------------------------- scale + easing
  step("layer scale_ease: scale x2 ease-in (-100), then x0.5 ease-out (+100)", function () {
    newLayer("scale_ease");
    place("scale_ease", "Arrow", 100, 150);
    key("scale_ease", 23); sel("scale_ease", 23); doc.scaleSelection(2, 2);
    key("scale_ease", 47); sel("scale_ease", 47); doc.scaleSelection(0.25, 0.25);
    return tween("scale_ease", 0, 22, {tweenEasing: -100, motionTweenScale: true}) + " | " +
           tween("scale_ease", 23, 46, {tweenEasing: 100, motionTweenScale: true});
  });

  // ---------------------------------------------------------------- rotate auto
  step("layer rotate_auto: move + rotate 90, rotate auto", function () {
    newLayer("rotate_auto");
    place("rotate_auto", "Arrow", 330, 50);
    key("rotate_auto", 23); sel("rotate_auto", 23); doc.moveSelectionBy({x: 170, y: 0}); doc.rotateSelection(90);
    return tween("rotate_auto", 0, 22, {motionTweenRotate: "auto"}) + " end matrix=" + dump(tl.layers[useLayer("rotate_auto")].frames[23].elements[0].matrix);
  });

  // ---------------------------------------------------------------- rotate CW then CCW x2
  step("layer rotate_cw: CW x1 while moving right, then CCW x2 moving back", function () {
    newLayer("rotate_cw");
    place("rotate_cw", "Arrow", 60, 50);
    key("rotate_cw", 23); sel("rotate_cw", 23); doc.moveSelectionBy({x: 200, y: 0});
    key("rotate_cw", 47); sel("rotate_cw", 47); doc.moveSelectionBy({x: -200, y: 0});
    return tween("rotate_cw", 0, 22, {motionTweenRotate: "clockwise", motionTweenRotateTimes: 1}) + " | " +
           tween("rotate_cw", 23, 46, {motionTweenRotate: "counter-clockwise", motionTweenRotateTimes: 2});
  });

  // ---------------------------------------------------------------- actions
  step("layer actions: frame 1 trace, frame 60 trace + stop()", function () {
    newLayer("actions");
    var L = useLayer("actions");
    tl.layers[L].frames[0].actionScript = 'trace("tweens: frame 1");';
    tl.insertBlankKeyframe(FRAMES - 1);
    tl.layers[L].frames[FRAMES - 1].actionScript = 'trace("tweens: frame " + _currentframe);\nstop();';
    return "actions frameCount=" + tl.layers[L].frameCount;
  });

  step("extend every layer to " + FRAMES + " frames", function () {
    var names = ["graphics", "consecutive", "alpha", "scale_ease", "rotate_auto", "rotate_cw", "actions"], s = [];
    for (var i = 0; i < names.length; i++) s.push(extend(names[i]));
    return s.join(", ") + "; timeline frameCount=" + tl.frameCount;
  });
  step("summary", function () {
    var s = "layers=" + tl.layers.length + " frames=" + tl.frameCount + " library=" + lib.items.length;
    for (var i = 0; i < tl.layers.length; i++) {
      var fr = tl.layers[i].frames, keys = [];
      for (var k = 0; k < fr.length; k++) if (fr[k].startFrame == k) keys.push(k + (fr[k].tweenType == "motion" ? "m" : ""));
      s += "\n     " + tl.layers[i].name + ": frames=" + fr.length + " keys=" + keys.join(",");
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
