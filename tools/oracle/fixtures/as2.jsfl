// Fixture builder: as2  (run with tools/oracle/build-all.ps1)
// ActionScript 2: frame scripts (variables, functions, trace), frame labels +
// gotoAndPlay("label"), stop() at the end, a 4-state button with an
// on(release) handler, a movie clip with onClipEvent(load)/(enterFrame), and a
// dynamic text field bound to a variable.  Without input the trace sequence is
// deterministic (the button only traces when clicked).
var NAME = "as2";
var OUT = JZ_OUT;   // tests/fixtures/NAME/ (from the prelude build-all.ps1 prepends)
var LOG = JZ_LOG;   // target/oracle/logs/NAME.log
var FRAMES = 12;
var FAILS = [];
FLfile.write(LOG, "");
function log(m) { FLfile.write(LOG, m + "\n", "append"); }
function step(label, fn) {
  try { var r = fn(); log("OK   " + label + (r === undefined ? "" : " -> " + r)); return true; }
  catch (e) { FAILS.push(label + ": " + e); log("FAIL " + label + ": " + e); return false; }
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
// ---- layer helpers (see symbols.jsfl)
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

var SCRIPTS = {
  0: 'var passes = 0;\n' +
     'function greet(who) {\n\treturn "Hello, " + who + "!";\n}\n' +
     'function add(a, b) {\n\treturn a + b;\n}\n' +
     'trace("frame 1: " + greet("Flash 8"));\n' +
     'trace("frame 1: add(2, 3) = " + add(2, 3));\n' +
     'trace("frame 1: typeof my_btn = " + typeof my_btn + ", ticker_mc._name = " + ticker_mc._name);\n' +
     'scoreText = "score: 0";\n',
  4: 'passes++;\n' +
     'trace("frame 5 [loop]: pass " + passes);\n' +
     'scoreText = "score: " + (passes * 10);\n',
  7: 'if (passes < 2) {\n\ttrace("frame 8: gotoAndPlay(\\"loop\\")");\n\tgotoAndPlay("loop");\n} else {\n\ttrace("frame 8: continue");\n}\n',
  11: 'trace("frame 12 [end]: _currentframe = " + _currentframe + ", scoreText = " + scoreText);\n' +
      'stop();\n'
};
var BUTTON_AS = 'on (release) {\n\ttrace("my_btn: release");\n}\n';
var CLIP_AS = 'onClipEvent (load) {\n\tthis.n = 0;\n\ttrace("ticker_mc: load");\n}\n' +
              'onClipEvent (enterFrame) {\n\tif (this.n < 3) {\n\t\tthis.n++;\n\t\ttrace("ticker_mc: enterFrame " + this.n);\n\t}\n}\n';

var doc = null, tl = null;
try {
  FLfile.createFolder(OUT);
  doc = fl.createDocument();
  var lib = doc.library;
  step("stage 400x300 @ 12fps", function () {
    doc.width = 400; doc.height = 300; doc.frameRate = 12; doc.backgroundColor = "#FFFFFF";
    fl.objectDrawingMode = false;
    return doc.width + "x" + doc.height + " @" + doc.frameRate;
  });
  step("stroke template", initStroke);

  // ---------------------------------------------------------------- library
  step("symbol Btn (button): up green, over yellow, down red, hit larger black rect", function () {
    lib.addNewItem("button", "Btn");
    lib.editItem("Btn");
    var t = doc.getTimeline();
    var states = [["#33CC33", 8], ["#FFCC00", 8], ["#CC3333", 8], ["#000000", 0]];
    for (var i = 0; i < 4; i++) {
      if (i > 0) t.insertBlankKeyframe(i);
      t.currentFrame = i;
      solidFill(states[i][0]);
      if (i < 3) { solidStroke("#000000", 2); doc.addNewRectangle({left: -50, top: -20, right: 50, bottom: 20}, states[i][1], false, false); }
      else { noStroke(); doc.addNewRectangle({left: -55, top: -25, right: 55, bottom: 25}, 0, false, true); }
    }
    var s = "frames=" + t.frameCount;
    for (i = 0; i < t.layers[0].frames.length; i++) s += " f" + i + ":" + t.layers[0].frames[i].elements.length;
    doc.exitEditMode();
    return s;
  });
  step("symbol Ticker (movie clip): purple disc", function () {
    lib.addNewItem("movie clip", "Ticker");
    lib.editItem("Ticker");
    noStroke(); solidFill("#9933CC");
    doc.addNewOval({left: -25, top: -25, right: 25, bottom: 25}, false, true);
    doc.exitEditMode();
  });
  tl = doc.getTimeline();

  // ---------------------------------------------------------------- stage content
  step("layer button: my_btn with on(release)", function () {
    tl.layers[0].name = "button";
    var el = place("button", "Btn", 110, 220, "my_btn");
    el.actionScript = BUTTON_AS;
    return el.symbolType + " " + el.name + " actionScript=" + JSON_ish(el.actionScript);
  });
  step("layer clip: ticker_mc with onClipEvent(load) / onClipEvent(enterFrame)", function () {
    newLayer("clip");
    var el = place("clip", "Ticker", 300, 220, "ticker_mc");
    el.actionScript = CLIP_AS;
    return el.symbolType + " " + el.name + " actionScript=" + JSON_ish(el.actionScript);
  });
  step("layer text: dynamic text bound to variable scoreText", function () {
    var L = newLayer("text");
    doc.addNewText({left: 20, top: 20, right: 380, bottom: 50});
    doc.setTextString("score: ?");
    var el = tl.layers[L].frames[0].elements[0];
    el.textType = "dynamic";
    el.variableName = "scoreText";
    el.lineType = "single line";
    el.setTextAttr("face", "_sans");
    el.setTextAttr("size", 20);
    el.setTextAttr("fillColor", "#000000");
    doc.selectNone();
    return "textType=" + el.textType + " variableName=" + el.variableName + " text=" + el.getTextString() +
           " face=" + el.getTextAttr("face") + " size=" + el.getTextAttr("size");
  });
  step("layer labels: 'loop' at frame 5, 'end' at frame 12", function () {
    var L = newLayer("labels");
    tl.insertBlankKeyframe(4); tl.layers[L].frames[4].name = "loop";
    tl.insertBlankKeyframe(11); tl.layers[L].frames[11].name = "end";
    return "frame5 label=" + tl.layers[L].frames[4].name + "/" + tl.layers[L].frames[4].labelType +
           " frame12 label=" + tl.layers[L].frames[11].name + "/" + tl.layers[L].frames[11].labelType;
  });
  step("layer actions: scripts on frames 1, 5, 8, 12", function () {
    var L = newLayer("actions"), s = "";
    for (var f in SCRIPTS) {
      var n = Number(f);
      if (n > 0) tl.insertBlankKeyframe(n);
      tl.layers[L].frames[n].actionScript = SCRIPTS[f];
      s += " f" + (n + 1) + ":" + tl.layers[L].frames[n].actionScript.length + "ch";
    }
    return s;
  });
  step("extend every layer to " + FRAMES + " frames", function () {
    var names = ["button", "clip", "text", "labels", "actions"], s = [];
    for (var i = 0; i < names.length; i++) {
      var L = useLayer(names[i]), n = tl.layers[L].frameCount;
      if (n < FRAMES) tl.insertFrames(FRAMES - n, false, n);
      s.push(names[i] + " " + n + "->" + tl.layers[L].frameCount);
    }
    return s.join(", ") + "; timeline frameCount=" + tl.frameCount;
  });
  step("summary", function () {
    var s = "layers=" + tl.layers.length + " frames=" + tl.frameCount + " library=" + lib.items.length;
    for (var i = 0; i < tl.layers.length; i++) {
      var fr = tl.layers[i].frames, keys = [];
      for (var k = 0; k < fr.length; k++) if (fr[k].startFrame == k) keys.push(k + (fr[k].name ? "[" + fr[k].name + "]" : "") + (fr[k].actionScript ? "*" : ""));
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
function JSON_ish(s) { return String(s).replace(/\n/g, "\\n").replace(/\t/g, "\\t"); }
log("FAILED STEPS: " + FAILS.length);
for (var i = 0; i < FAILS.length; i++) log("  - " + FAILS[i]);
try { if (doc) fl.closeDocument(doc, false); } catch (e) { log("close: " + e); }
log("DONE");
fl.quit(false);
