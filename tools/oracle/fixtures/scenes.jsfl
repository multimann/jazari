// Fixture builder: scenes  (run with tools/oracle/build-all.ps1 -Names scenes)
// Three scenes, Intro (4 frames), Main (5) and Outro (4), with frame labels
// and frame scripts that trace the scene, the frame and _currentframe. The
// scripts use nextScene(); play(), an unqualified gotoAndPlay(4) inside Main,
// gotoAndPlay("Intro", 3), gotoAndPlay("finale") (a label in another scene),
// prevScene(); play() and gotoAndPlay("Outro", "outro"), and end with stop(). A
// global counter (hops) makes the path deterministic and finite. Each scene has
// its own background colour and a Dot movie clip tweened across the scene.
var NAME = "scenes";
var OUT = JZ_OUT;   // tests/fixtures/NAME/ (from the prelude build-all.ps1 prepends)
var LOG = JZ_LOG;   // target/oracle/logs/NAME.log
var FAILS = [];
FLfile.write(LOG, "");
function log(m) { FLfile.write(LOG, m + "\n", "append"); }
function step(label, fn) {
  try { var r = fn(); log("OK   " + label + (r === undefined ? "" : " -> " + r)); return true; }
  catch (e) { FAILS.push(label + ": " + e); log("FAIL " + label + ": " + e); return false; }
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
  var el = els[els.length - 1];
  if (name) el.name = name;
  doc.selectNone(); doc.selection = [el];
  return el;
}
function sel(layer, frame) {
  var t = doc.getTimeline(), L = useLayer(layer);
  t.currentFrame = frame;
  var el = t.layers[L].frames[frame].elements[0];
  doc.selectNone(); doc.selection = [el];
  return el;
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

// ---- the scenes: name, frame count, background colour, dot y, labels and scripts (0-based frames)
var SCENES = [
  {name: "Intro", frames: 4, color: "#FFD8D8", dotY: 120,
   labels: {0: "intro"},
   scripts: {
     0: 'if (hops == undefined) {\n\thops = 0;\n}\ntrace("Intro 1 [intro]: root " + _currentframe);\n',
     2: 'trace("Intro 3: root " + _currentframe + ", hops=" + hops);\n',
     // nextScene()/prevScene() compile to a bare GotoFrame, which stops the
     // playhead (a first build without play() traced only up to Main 1).
     3: 'trace("Intro 4: nextScene(); play()");\nnextScene();\nplay();\n'}},
  {name: "Main", frames: 5, color: "#D8FFD8", dotY: 200,
   labels: {0: "main", 3: "mainLoop"},
   scripts: {
     0: 'trace("Main 1 [main]: root " + _currentframe);\n',
     1: 'if (hops == 0) {\n\thops = 1;\n\ttrace("Main 2: gotoAndPlay(4)");\n\tgotoAndPlay(4);\n} else {\n' +
        '\ttrace("Main 2: root " + _currentframe + ", hops=" + hops);\n}\n',
     3: 'trace("Main 4 [mainLoop]: root " + _currentframe + ", hops=" + hops);\n',
     4: 'if (hops == 1) {\n\thops = 2;\n\ttrace("Main 5: gotoAndPlay(\\"Intro\\", 3)");\n\tgotoAndPlay("Intro", 3);\n' +
        '} else if (hops == 2) {\n\thops = 3;\n\ttrace("Main 5: gotoAndPlay(\\"finale\\")");\n\tgotoAndPlay("finale");\n' +
        '} else {\n\ttrace("Main 5: gotoAndPlay(\\"Outro\\", \\"outro\\")");\n\tgotoAndPlay("Outro", "outro");\n}\n'}},
  {name: "Outro", frames: 4, color: "#D8D8FF", dotY: 280,
   labels: {0: "outro", 2: "finale"},
   scripts: {
     0: 'trace("Outro 1 [outro]: root " + _currentframe);\n',
     2: 'trace("Outro 3 [finale]: root " + _currentframe + ", hops=" + hops);\n',
     3: 'if (hops == 3) {\n\thops = 4;\n\ttrace("Outro 4: prevScene(); play()");\n\tprevScene();\n\tplay();\n} else {\n' +
        '\ttrace("Outro 4: stop(), root " + _currentframe);\n\tstop();\n}\n'}}
];

function buildScene(S) {
  var t = doc.getTimeline(), N = S.frames;
  t.layers[0].name = "art";
  useLayer("art");
  noStroke();
  solidFill(S.color);
  doc.addNewRectangle({left: 25, top: 25, right: 525, bottom: 375}, 0, false, true);
  newLayer("dot");
  place("dot", "Dot", 75, S.dotY);
  useLayer("dot");
  t.insertKeyframe(N - 1);
  sel("dot", N - 1); doc.moveSelectionBy({x: 400, y: 0});
  t.createMotionTween(0, N - 2);
  var L = newLayer("labels"), lab = [];
  for (var f in S.labels) {
    var n = Number(f);
    if (n > 0) t.insertBlankKeyframe(n);
    t.layers[L].frames[n].name = S.labels[f];
    lab.push((n + 1) + "=" + t.layers[L].frames[n].name);
  }
  var A = newLayer("actions"), scr = [];
  for (f in S.scripts) {
    n = Number(f);
    if (n > 0) t.insertBlankKeyframe(n);
    t.layers[A].frames[n].actionScript = S.scripts[f];
    scr.push(n + 1);
  }
  var ext = extendAll(N);
  return "scene " + t.name + ": frames=" + t.frameCount + " labels " + lab.join(",") + " scripts on " + scr.join(",") + " [" + ext + "]";
}

var doc = null;
try {
  FLfile.createFolder(OUT);
  doc = fl.createDocument();
  var lib = doc.library;
  step("stage 550x400 @ 12fps", function () {
    doc.width = 550; doc.height = 400; doc.frameRate = 12; doc.backgroundColor = "#FFFFFF";
    fl.objectDrawingMode = false;
    return doc.width + "x" + doc.height;
  });
  step("symbol Dot (movie clip): black disc r=12", function () {
    lib.addNewItem("movie clip", "Dot");
    lib.editItem("Dot");
    noStroke(); solidFill("#000000");
    doc.addNewOval({left: -12, top: -12, right: 12, bottom: 12}, false, true);
    doc.exitEditMode();
  });
  step("scene 1 renamed Intro", function () {
    doc.editScene(0);
    var r = doc.renameScene("Intro");
    return "renameScene=" + r + " timelines=" + doc.timelines.length + " name=" + doc.getTimeline().name;
  });
  step("scene Intro content", function () { doc.editScene(0); return buildScene(SCENES[0]); });
  step("add scene Main", function () {
    var r = doc.addNewScene("Main");
    return "addNewScene=" + r + " timelines=" + doc.timelines.length + " current=" + doc.getTimeline().name;
  });
  step("scene Main content", function () { doc.editScene(1); return buildScene(SCENES[1]); });
  step("add scene Outro", function () {
    doc.editScene(1);
    var r = doc.addNewScene("Outro");
    return "addNewScene=" + r + " timelines=" + doc.timelines.length + " current=" + doc.getTimeline().name;
  });
  step("scene Outro content", function () { doc.editScene(2); return buildScene(SCENES[2]); });
  step("summary", function () {
    var s = "scenes=" + doc.timelines.length + " library=" + lib.items.length;
    for (var i = 0; i < doc.timelines.length; i++) {
      var t = doc.timelines[i];
      s += "\n     " + i + " " + t.name + " frames=" + t.frameCount + " layers=";
      for (var k = 0; k < t.layers.length; k++) s += t.layers[k].name + "(" + t.layers[k].frameCount + ") ";
    }
    return s;
  });
  step("save FLA", function () { doc.editScene(0); return fl.saveDocument(doc, OUT + NAME + ".fla"); });
  // Publish from the saved file (MANIFEST J15).
  step("reopen saved FLA, export SWF from it", function () {
    fl.closeDocument(doc, false);
    doc = fl.openDocument(OUT + NAME + ".fla");
    doc.exportSWF(OUT + "flash8.swf", true);
    var s = doc.name + " -> " + FLfile.exists(OUT + "flash8.swf") + "; scenes:";
    for (var i = 0; i < doc.timelines.length; i++) s += " " + doc.timelines[i].name + "(" + doc.timelines[i].frameCount + ")";
    return s;
  });
} catch (e) { FAILS.push("top: " + e); log("ERROR: " + e); }
log("FAILED STEPS: " + FAILS.length);
for (var i = 0; i < FAILS.length; i++) log("  - " + FAILS[i]);
try { if (doc) fl.closeDocument(doc, false); } catch (e) { log("close: " + e); }
log("DONE");
fl.quit(false);
