// Fixture builder: symbols  (run with tools/oracle/build-all.ps1)
// Nested movie clips with instance names, colour effects (alpha, tint,
// brightness, advanced), Flash 8 filters, blend modes and cacheAsBitmap.
var NAME = "symbols";
var OUT = JZ_OUT;   // tests/fixtures/NAME/ (from the prelude build-all.ps1 prepends)
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
    s += k + ":" + v + " ";
  }
  return s + "}";
}
function solidFill(color) { var f = doc.getCustomFill(); f.style = "solid"; f.color = color; doc.setCustomFill(f); }
function noStroke() { var s = doc.getCustomStroke(); s.style = "noStroke"; doc.setCustomStroke(s); }
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
// Layer helpers.  Selecting an element (doc.selection = [...], mouseClick,
// setInstance*) silently moves Flash's notion of the current layer, and
// addItem() then drops the instance on that layer, so every placement first
// re-selects its target layer by name.
function layerIndex(t, name) {
  for (var i = 0; i < t.layers.length; i++) if (t.layers[i].name == name) return i;
  throw new Error("no layer " + name);
}
function useLayer(name) {
  var t = doc.getTimeline(), i = layerIndex(t, name);
  doc.selectNone();
  t.setSelectedLayers(i, true);
  t.currentLayer = i;
  return i;
}
function newLayer(name) {      // always add on top of the stack
  var t = doc.getTimeline();
  doc.selectNone();
  t.setSelectedLayers(0, true);
  t.currentLayer = 0;
  t.addNewLayer(name);
  return useLayer(name);
}
function findItem(name) {
  var it = doc.library.items;
  for (var i = 0; i < it.length; i++) if (it[i].name == name) return it[i];
  throw new Error("no library item " + name);
}
// Place a library item on the named layer (frame = current frame), name it,
// leave it selected and return it.  The instance is taken from the layer's
// element list, not from doc.selection.
function place(layer, item, x, y, name) {
  var t = doc.getTimeline(), L = useLayer(layer), F = t.currentFrame;
  var before = t.layers[L].frames[F].elements.length;
  // The first addItem() after drawing a shape often returns true but places
  // nothing; retry (no duplicates are possible because nothing appeared).
  var r, els, tries = 0;
  do {
    tries++;
    useLayer(layer);
    r = tries < 4 ? doc.addItem({x: x, y: y}, findItem(item)) : doc.library.addItemToDocument({x: x, y: y}, item);
    els = t.layers[L].frames[F].elements;
  } while (els.length == before && tries < 4);
  if (tries > 1) log("     (" + item + " needed " + tries + " placement attempt(s))");
  if (els.length == before) {
    var where = "";
    for (var i = 0; i < t.layers.length; i++) where += t.layers[i].name + ":" + t.layers[i].frames[F].elements.length + " ";
    throw new Error("addItem(" + item + ") returned " + r + " but nothing appeared on layer " + layer + " [" + where + "]");
  }
  var el = els[els.length - 1];
  if (name) el.name = name;
  doc.selectNone();
  doc.selection = [el];
  return el;
}
function colorInfo(el) {
  var s = "";
  var ks = ["colorMode", "colorAlphaPercent", "colorAlphaAmount", "colorRedPercent", "colorRedAmount",
            "colorGreenPercent", "colorGreenAmount", "colorBluePercent", "colorBlueAmount", "blendMode", "cacheAsBitmap"];
  for (var i = 0; i < ks.length; i++) { var v; try { v = el[ks[i]]; } catch (e) { v = "ERR"; } s += ks[i] + "=" + v + " "; }
  return s;
}
function filterInfo() {
  var fs = doc.getFilters(), s = fs.length + " filter(s)";
  for (var i = 0; i < fs.length; i++) s += " | " + dump(fs[i]);
  return s;
}

var doc = null;
try {
  FLfile.createFolder(OUT);
  doc = fl.createDocument();
  var lib = doc.library;
  step("stage 640x480 @ 12fps", function () {
    doc.width = 640; doc.height = 480; doc.frameRate = 12; doc.backgroundColor = "#FFFFFF";
    fl.objectDrawingMode = false;
    return doc.width + "x" + doc.height;
  });
  step("stroke template", initStroke);

  // ---------------------------------------------------------------- library
  step("symbol Inner (movie clip): orange disc + black marker", function () {
    lib.addNewItem("movie clip", "Inner");
    lib.editItem("Inner");
    noStroke();
    solidFill("#FF9900");
    doc.addNewOval({left: -20, top: -20, right: 20, bottom: 20}, false, true);
    solidFill("#000000");
    doc.addNewRectangle({left: 10, top: -4, right: 18, bottom: 4}, 0, false, true);
    var n = doc.getTimeline().layers[0].frames[0].elements.length;
    doc.exitEditMode();
    return n + " element(s)";
  });
  step("symbol Outer (movie clip): rounded rect + innerA_mc + innerB_mc (scaled 0.5)", function () {
    lib.addNewItem("movie clip", "Outer");
    lib.editItem("Outer");
    var t = doc.getTimeline();
    solidFill("#3366CC");
    solidStroke("#000000", 2, "round", "miter");
    doc.addNewRectangle({left: -60, top: -35, right: 60, bottom: 35}, 10, false, false);
    t.layers[0].name = "shape";
    newLayer("inner");
    place("inner", "Inner", -25, 0, "innerA_mc");
    var b = place("inner", "Inner", 30, 0, "innerB_mc");
    doc.scaleSelection(0.5, 0.5);
    var s = "layers=" + t.layers.length + " inner elements=" + t.layers[t.currentLayer].frames[0].elements.length +
            " B.matrix=" + dump(b.matrix);
    doc.exitEditMode();
    return s;
  });
  step("symbol Box (movie clip): 60x60 four colour quadrants, black border", function () {
    lib.addNewItem("movie clip", "Box");
    lib.editItem("Box");
    noStroke();
    var q = [["#FF0000", -30, -30], ["#00CC00", 0, -30], ["#0000FF", -30, 0], ["#FFFFFF", 0, 0]];
    for (var i = 0; i < 4; i++) { solidFill(q[i][0]); doc.addNewRectangle({left: q[i][1], top: q[i][2], right: q[i][1] + 30, bottom: q[i][2] + 30}, 0, false, true); }
    solidStroke("#000000", 2, "round", "miter");
    doc.addNewRectangle({left: -30, top: -30, right: 30, bottom: 30}, 0, true, false);
    doc.exitEditMode();
    return lib.items.length + " library item(s)";
  });

  var tl = doc.getTimeline();
  // ---------------------------------------------------------------- backdrop
  step("layer backdrop: vertical colour stripes under the blend-mode row", function () {
    tl.layers[0].name = "backdrop";
    noStroke();
    var cols = ["#FF0000", "#00FF00", "#0000FF", "#FFFF00", "#000000", "#FFFFFF", "#808080", "#00FFFF"];
    for (var i = 0; i < 8; i++) { solidFill(cols[i]); doc.addNewRectangle({left: 40 + i * 50, top: 290, right: 90 + i * 50, bottom: 390}, 0, false, true); }
    return tl.layers[0].frames[0].elements.length + " element(s)";
  });

  // ---------------------------------------------------------------- nested
  step("layer nested: outer_mc (contains innerA_mc, innerB_mc)", function () {
    newLayer("nested");
    var el = place("nested", "Outer", 90, 70, "outer_mc");
    return el.name + " " + el.libraryItem.name + " " + dump(el.matrix);
  });

  // ---------------------------------------------------------------- colour effects
  newLayer("color");
  step("plain_mc (no colour effect)", function () { return colorInfo(place("color", "Box", 220, 70, "plain_mc")); });
  step("alpha_mc: setInstanceAlpha(40)", function () { var el = place("color", "Box", 310, 70, "alpha_mc"); doc.setInstanceAlpha(40); return colorInfo(el); });
  step("tint_mc: setInstanceTint(#0000FF, 60)", function () { var el = place("color", "Box", 400, 70, "tint_mc"); doc.setInstanceTint("#0000FF", 60); return colorInfo(el); });
  step("bright_mc: setInstanceBrightness(-50)", function () { var el = place("color", "Box", 490, 70, "bright_mc"); doc.setInstanceBrightness(-50); return colorInfo(el); });
  step("advanced_mc: R 50%+100, G 100%-60, B 20%+0, A 80%+0", function () {
    var el = place("color", "Box", 580, 70, "advanced_mc");
    try { el.colorMode = "advanced"; } catch (e) { log("     colorMode not settable: " + e); }
    el.colorRedPercent = 50; el.colorRedAmount = 100;
    el.colorGreenPercent = 100; el.colorGreenAmount = -60;
    el.colorBluePercent = 20; el.colorBlueAmount = 0;
    el.colorAlphaPercent = 80; el.colorAlphaAmount = 0;
    return colorInfo(el);
  });

  // ---------------------------------------------------------------- filters
  newLayer("filters");
  step("drop_mc: dropShadowFilter distance 8 angle 45 blur 6 strength 80 quality high", function () {
    place("filters", "Box", 80, 200, "drop_mc");
    doc.addFilter("dropShadowFilter");
    doc.setFilterProperty("distance", 0, 8); doc.setFilterProperty("angle", 0, 45);
    doc.setFilterProperty("blurX", 0, 6); doc.setFilterProperty("blurY", 0, 6);
    doc.setFilterProperty("strength", 0, 80); doc.setFilterProperty("quality", 0, "high");
    doc.setFilterProperty("color", 0, "#333333");
    return filterInfo();
  });
  step("blur_mc: blurFilter 10x4 quality medium", function () {
    place("filters", "Box", 200, 200, "blur_mc");
    doc.addFilter("blurFilter");
    doc.setFilterProperty("blurX", 0, 10); doc.setFilterProperty("blurY", 0, 4); doc.setFilterProperty("quality", 0, "medium");
    return filterInfo();
  });
  step("glow_mc: glowFilter red blur 12 strength 200", function () {
    place("filters", "Box", 320, 200, "glow_mc");
    doc.addFilter("glowFilter");
    doc.setFilterProperty("color", 0, "#FF0000"); doc.setFilterProperty("blurX", 0, 12); doc.setFilterProperty("blurY", 0, 12);
    doc.setFilterProperty("strength", 0, 200); doc.setFilterProperty("quality", 0, "low");
    return filterInfo();
  });
  step("bevel_mc: bevelFilter distance 5 angle 45 blur 5 strength 100 type inner", function () {
    place("filters", "Box", 440, 200, "bevel_mc");
    doc.addFilter("bevelFilter");
    doc.setFilterProperty("distance", 0, 5); doc.setFilterProperty("angle", 0, 45);
    doc.setFilterProperty("blurX", 0, 5); doc.setFilterProperty("blurY", 0, 5);
    doc.setFilterProperty("strength", 0, 100); doc.setFilterProperty("type", 0, "inner");
    doc.setFilterProperty("highlightColor", 0, "#FFFFFF"); doc.setFilterProperty("shadowColor", 0, "#000000");
    return filterInfo();
  });
  step("multi_mc: glowFilter (inner, yellow) then dropShadowFilter", function () {
    place("filters", "Box", 560, 200, "multi_mc");
    doc.addFilter("glowFilter");
    doc.setFilterProperty("color", 0, "#FFFF00"); doc.setFilterProperty("inner", 0, true);
    doc.setFilterProperty("blurX", 0, 8); doc.setFilterProperty("blurY", 0, 8);
    doc.addFilter("dropShadowFilter");
    doc.setFilterProperty("distance", 1, 6);
    return filterInfo();
  });

  // ---------------------------------------------------------------- blend modes + cacheAsBitmap
  newLayer("blend");
  step("normal_mc (reference over stripes)", function () { return colorInfo(place("blend", "Box", 90, 340, "normal_mc")); });
  step("multiply_mc: setBlendMode(multiply)", function () { var el = place("blend", "Box", 190, 340, "multiply_mc"); doc.setBlendMode("multiply"); return colorInfo(el); });
  step("screen_mc: setBlendMode(screen)", function () { var el = place("blend", "Box", 290, 340, "screen_mc"); doc.setBlendMode("screen"); return colorInfo(el); });
  step("add_mc: setBlendMode(add)", function () { var el = place("blend", "Box", 390, 340, "add_mc"); doc.setBlendMode("add"); return colorInfo(el); });
  step("cache_mc: cacheAsBitmap = true", function () {
    var el = place("blend", "Box", 560, 340, "cache_mc");
    el.cacheAsBitmap = true;
    return colorInfo(el);
  });

  step("layer actions: frame 1 script", function () {
    newLayer("actions");
    tl.layers[layerIndex(tl, "actions")].frames[0].actionScript =
      'trace("symbols: frame 1");\n' +
      'trace("nested: " + outer_mc.innerA_mc._name + ", " + outer_mc.innerB_mc._name);\n' +
      'trace("innerB_mc._xscale = " + outer_mc.innerB_mc._xscale);\n' +
      'trace("alpha_mc._alpha = " + Math.round(alpha_mc._alpha));\n' +
      'trace("drop_mc.filters.length = " + drop_mc.filters.length);\n' +
      'trace("multi_mc.filters.length = " + multi_mc.filters.length);\n' +
      'trace("multiply_mc.blendMode = " + multiply_mc.blendMode);\n' +
      'trace("add_mc.blendMode = " + add_mc.blendMode);\n' +
      'trace("cache_mc.cacheAsBitmap = " + cache_mc.cacheAsBitmap);\n';
    return "ok";
  });
  step("summary", function () {
    var s = "layers=" + tl.layers.length + " frames=" + tl.frameCount + " library=" + lib.items.length;
    for (var i = 0; i < tl.layers.length; i++) {
      var els = tl.layers[i].frames[0].elements, n = [];
      for (var k = 0; k < els.length; k++) n.push(els[k].elementType + (els[k].name ? ":" + els[k].name : ""));
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
