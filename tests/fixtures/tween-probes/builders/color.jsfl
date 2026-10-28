// Tween probe: color (run with tests/fixtures/tween-probes/builders/build.ps1)
// Colour effects and filters in motion tweens. One Arrow movie clip per
// layer, not moved, tweened from frame 0 to frame 20 between two colour
// effects (tint, brightness, alpha, advanced) or two filter lists (same
// kinds, different kinds, added, removed, reordered, adjust colour).
var NAME = "color";
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
function colorInfo(el) {
  var ks = ["colorMode", "colorAlphaPercent", "colorAlphaAmount", "colorRedPercent", "colorRedAmount",
            "colorGreenPercent", "colorGreenAmount", "colorBluePercent", "colorBlueAmount"], s = [];
  for (var i = 0; i < ks.length; i++) { var v; try { v = el[ks[i]]; } catch (e) { v = "ERR"; } s.push(ks[i].replace("color", "") + "=" + v); }
  return s.join(" ");
}
// doc.getFilters() is only called on instances known to have filters (the
// first run of this probe hung right after placing an instance without any).
function filterInfo(has) {
  if (!has) return "no filters";
  var fs = doc.getFilters(), s = fs.length + " filter(s)";
  for (var i = 0; i < fs.length; i++) {
    s += " [";
    for (var k in fs[i]) s += k + "=" + fs[i][k] + " ";
    s += "]";
  }
  return s;
}
function advanced(el, v) {
  el.colorMode = "advanced";
  el.colorRedPercent = v[0]; el.colorRedAmount = v[1];
  el.colorGreenPercent = v[2]; el.colorGreenAmount = v[3];
  el.colorBluePercent = v[4]; el.colorBlueAmount = v[5];
  el.colorAlphaPercent = v[6]; el.colorAlphaAmount = v[7];
}
function setF(i, props) { for (var k in props) doc.setFilterProperty(k, i, props[k]); }
// A layer with an Arrow at (x, y) tweened over frames 0..LAST; start() and
// end() act on the selected instance of each keyframe.
function probe(name, x, y, start, end, f0, f1) {
  return step("layer " + name, function () {
    newLayer(name);
    var el = place(name, "Arrow", x, y);
    doc.selectNone(); doc.selection = [el];
    if (start) start(el);
    el = selAt(name, 0);
    var s = colorInfo(el) + " " + filterInfo(f0);
    key(name, LAST);
    el = selAt(name, LAST);
    end(el);
    el = selAt(name, LAST);
    s += " -> " + colorInfo(el) + " " + filterInfo(f1);
    var L = useLayer(name);
    tl.createMotionTween(0, LAST - 1);
    var f = tl.layers[L].frames[0];
    f.motionTweenRotate = "auto"; f.motionTweenRotateTimes = 0; f.motionTweenScale = true; f.tweenEasing = 0;
    return s + " tween=" + f.tweenType;
  });
}
function afterReopen() { return ""; }

var doc = null, tl = null;
try {
  FLfile.createFolder(OUT);
  doc = fl.createDocument();
  var lib = doc.library;
  step("stage 600x400 @ 24fps", function () {
    doc.width = 600; doc.height = 400; doc.frameRate = 24; doc.backgroundColor = "#FFFFFF";
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
  // ---- colour effects
  probe("c_none_tint", 60, 40, null, function () { doc.setInstanceTint("#FF0000", 100); });
  probe("c_tint_tint", 160, 40, function () { doc.setInstanceTint("#FF0000", 50); }, function () { doc.setInstanceTint("#0000FF", 80); });
  probe("c_bright", 260, 40, function () { doc.setInstanceBrightness(-50); }, function () { doc.setInstanceBrightness(70); });
  probe("c_alpha_tint", 360, 40, function () { doc.setInstanceAlpha(30); }, function () { doc.setInstanceTint("#00FF00", 60); });
  probe("c_adv", 460, 40, function (el) { advanced(el, [50, 100, 120, -50, 80, 20, 70, 10]); },
        function (el) { advanced(el, [150, -100, 30, 200, 100, 0, 100, -50]); });
  probe("c_tint_none", 560, 40, function () { doc.setInstanceTint("#0000FF", 60); }, function (el) { el.colorMode = "none"; });
  probe("c_alpha", 60, 100, function () { doc.setInstanceAlpha(100); }, function () { doc.setInstanceAlpha(7); });
  // ---- filters
  probe("f_drop", 60, 180, function () {
    doc.addFilter("dropShadowFilter");
    setF(0, {blurX: 5, blurY: 5, strength: 100, quality: "low", angle: 45, distance: 5, color: "#000000"});
  }, function () {
    setF(0, {blurX: 20, blurY: 10, strength: 50, quality: "low", angle: 200, distance: 30, color: "#FF000080"});
  }, true, true);
  probe("f_glow", 160, 180, function () {
    doc.addFilter("glowFilter");
    setF(0, {blurX: 5, blurY: 5, strength: 100, quality: "medium", color: "#FF0000"});
  }, function () {
    setF(0, {blurX: 20, blurY: 30, strength: 300, quality: "medium", color: "#0000FF"});
  }, true, true);
  probe("f_blur", 260, 180, function () {
    doc.addFilter("blurFilter");
    setF(0, {blurX: 0, blurY: 0, quality: "low"});
  }, function () {
    setF(0, {blurX: 20, blurY: 10, quality: "high"});
  }, true, true);
  probe("f_bevel", 360, 180, function () {
    doc.addFilter("bevelFilter");
    setF(0, {blurX: 4, blurY: 4, strength: 100, distance: 4, angle: 45, highlightColor: "#FFFFFF", shadowColor: "#000000"});
  }, function () {
    setF(0, {blurX: 10, blurY: 2, strength: 200, distance: 10, angle: 135, highlightColor: "#FFFF00", shadowColor: "#0000FF"});
  }, true, true);
  probe("f_mismatch", 460, 180, function () {
    doc.addFilter("dropShadowFilter");
    setF(0, {blurX: 4, blurY: 4, distance: 4});
  }, function () {
    setF(0, {blurX: 12, blurY: 12, distance: 12});
    doc.addFilter("blurFilter");
    setF(1, {blurX: 8, blurY: 8});
  }, true, true);
  probe("f_add", 560, 180, null, function () {
    doc.addFilter("glowFilter");
    setF(0, {blurX: 16, blurY: 16, strength: 200, color: "#00AA00"});
  }, false, true);
  probe("f_remove", 60, 280, function () {
    doc.addFilter("glowFilter");
    setF(0, {blurX: 16, blurY: 16, strength: 200, color: "#00AA00"});
  }, function () { doc.removeAllFilters(); }, true, false);
  probe("f_order", 160, 280, function () {
    doc.addFilter("blurFilter"); setF(0, {blurX: 2, blurY: 2});
    doc.addFilter("glowFilter"); setF(1, {blurX: 6, blurY: 6, color: "#FF00FF"});
  }, function () {
    doc.removeAllFilters();
    doc.addFilter("glowFilter"); setF(0, {blurX: 18, blurY: 18, color: "#FF00FF"});
    doc.addFilter("blurFilter"); setF(1, {blurX: 10, blurY: 10});
  }, true, true);
  probe("f_adjust", 260, 280, function () {
    doc.addFilter("adjustColorFilter");
    setF(0, {brightness: 0, contrast: 0, saturation: 0, hue: 0});
  }, function () {
    setF(0, {brightness: 50, contrast: 30, saturation: -40, hue: 60});
  }, true, true);
  probe("f_drop_knock", 360, 280, function () {
    doc.addFilter("dropShadowFilter");
    setF(0, {blurX: 5, blurY: 5, distance: 5, inner: false, knockout: false, hideObject: false});
  }, function () {
    setF(0, {blurX: 15, blurY: 15, distance: 15, inner: true, knockout: true, hideObject: false});
  }, true, true);
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
