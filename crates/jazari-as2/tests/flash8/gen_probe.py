"""Flash 8 oracle probe for jazari-as2: writes as2probe.jsfl, which makes the Flash 8 IDE compile
AS2 snippets so their bytecode can be compared with what Rascal produces.

Each frame script is on its own keyframe so a compile error in one does not hide the others;
each instance gets its own symbol (button or movie clip) and layer.

Run (one Flash instance at a time):
    python gen_probe.py
    powershell -File C:/dev/oracle/run-jsfl.ps1 -Script <this dir>/as2probe.jsfl -Log C:/dev/jazari/target/a-as2/oracle/as2probe.log
then copy as2probe.swf and as2probe-output.txt from C:/dev/jazari/target/a-as2/oracle next to this script.
src/oracle_tests.rs compares them with our compiler.
"""
import json
import os

HERE = os.path.dirname(os.path.abspath(__file__))
DIR_URI = "file:///C|/dev/jazari/target/a-as2/oracle/"

FRAMES = [
    # 0: ++/-- on members and indexes
    'var o = {a:1};\nvar arr = [5, 6];\nvar i = 0;\no.a++;\no.a--;\nvar x = o.a++;\nvar y = o.a--;\n'
    'arr[i]++;\nvar z = arr[i++]++;\nthis._x++;\n++o.a;\n_x++;\n'
    'trace(o.a + " " + x + " " + y + " " + arr + " " + z + " " + i);\n',
    # 1: do-while with continue/break, and a braceless body
    'var n = 0;\ndo {\n\tn++;\n\tif (n == 2) continue;\n\tif (n > 5) break;\n} while (n < 3);\n'
    'do n++; while (n < 10)\ntrace(n);\n',
    # 2..15: string escapes
    'var s = "\\u0645\\u0631\\u062D\\u0628\\u0627";\n',
    'var s = "\\xE9";\n',
    'var s = "\\xC3\\xA9";\n',
    'var s = "a\\bb\\fc";\n',
    'var s = "\\101\\60x";\n',
    'var s = "\\0end";\n',
    'var s = "\\q\\z";\n',
    'var s = "\\uD83D\\uDE00";\n',
    'var s = "\\u00e9\\u00E9";\n',
    'var s = "\\x41\\x4a";\n',
    'var s = "\\u12";\n',
    'var s = "\\x4";\n',
    'var s = "\\8\\9";\n',
    'var s = "\\377\\400";\n',
    # 16..25: errors and edge cases
    '"abc" = 5;\n',
    '5++;\n',
    'trace(1;\n',
    'on (press) { trace(1); }\n',
    'var a = 1;\n@foo;\n',
    'do { } while (false) trace("after");\n',
    'getTimer()++;\n',
    'onClipEvent (load) { }\n',
    'var q = 1;\nvar w = q +* 2;\n',
    'var t = "\\u0645";\nvar u = \'\\x41\';\n',
]

INSTANCES = [
    ("button", "btn_all",
     'on (press, release, releaseOutside, rollOver, rollOut, dragOver, dragOut) {\n\ttrace("all");\n}\n'
     'on (keyPress "<Enter>") { trace("enter"); }\n'
     'on (keyPress "a") { trace("a"); }\n'
     'on (keyPress "<Left>") { trace("left"); }\n'
     'on (keyPress "<PageUp>") {}\n'
     'on (keyPress "<Space>") { trace("space"); }\n'
     'on (keyPress "<Escape>") { trace("esc"); }\n'
     'on (keyPress "A") { trace("A"); }\n'
     'on (keyPress "<Home>") {} on (keyPress "<End>") {} on (keyPress "<Insert>") {}\n'
     'on (keyPress "<Delete>") {} on (keyPress "<Backspace>") {} on (keyPress "<Up>") {}\n'
     'on (keyPress "<Down>") {} on (keyPress "<PageDown>") {} on (keyPress "<Tab>") {}\n'
     'on (keyPress "<Right>") {}\n'),
    ("button", "btn_case", 'on (Press) { trace(1); }\n'),
    ("button", "btn_mixed", 'on (release, keyPress "<Tab>") { trace(2); }\n'),
    ("button", "btn_clipev", 'onClipEvent (load) { trace(3); }\n'),
    ("button", "btn_outside", 'trace("x");\non (release) { trace(4); }\n'),
    ("button", "btn_keycase", 'on (keyPress "<enter>") { trace(5); }\n'),
    ("movie clip", "mc_all",
     'onClipEvent (load) { trace("load"); }\n'
     'onClipEvent (enterFrame) { }\n'
     'onClipEvent (unload) { trace(1); }\n'
     'onClipEvent (mouseMove) { trace(2); }\n'
     'onClipEvent (mouseDown) { trace(3); }\n'
     'onClipEvent (mouseUp) { trace(4); }\n'
     'onClipEvent (keyDown) { trace(5); }\n'
     'onClipEvent (keyUp) { trace(6); }\n'
     'onClipEvent (data) { trace(7); }\n'
     'onClipEvent (initialize) { trace(8); }\n'
     'onClipEvent (construct) { trace(9); }\n'
     'on (press) { trace(10); }\n'
     'on (release, releaseOutside) { trace(11); }\n'
     'on (rollOver) {} on (rollOut) {} on (dragOver) {} on (dragOut) {}\n'
     'on (keyPress "<Space>") { trace(12); }\n'
     'on (keyPress "x") { trace(13); }\n'),
    ("movie clip", "mc_multi", 'onClipEvent (load, enterFrame) { trace(1); }\n'),
    ("movie clip", "mc_case", 'onClipEvent (Load) { trace(1); }\n'),
    ("movie clip", "mc_nested",
     '// a comment with } brace\n'
     'onClipEvent (load) {\n'
     '\tvar s = "}{"; /* } */\n'
     '\tif (true) { trace(s); }\n'
     "\ttrace('it\\'s }');\n"
     '}\n'),
    ("movie clip", "mc_two_keys", 'on (keyPress "a", keyPress "b") { trace(1); }\n'),
    ("movie clip", "mc_onkey_frame", 'on (keyPress "<Enter>", press) { trace(2); }\n'),
]

JSFL = r'''
var DIR = %(dir)s;
var LOG = DIR + "as2probe.log";
var FRAMES = %(frames)s;
var INSTANCES = %(instances)s;
var FAILS = [];
FLfile.write(LOG, "");
function log(m) { FLfile.write(LOG, m + "\n", "append"); }
function step(label, fn) {
  try { var r = fn(); log("OK   " + label + (r === undefined ? "" : " -> " + r)); return true; }
  catch (e) { FAILS.push(label + ": " + e); log("FAIL " + label + ": " + e); return false; }
}
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
  if (els.length == before) throw new Error("could not place " + item + " on " + layer);
  var el = els[els.length - 1];
  if (name) el.name = name;
  doc.selectNone(); doc.selection = [el];
  return el;
}
var doc = null, tl = null;
try {
  doc = fl.createDocument();
  var lib = doc.library;
  doc.width = 400; doc.height = 300; doc.frameRate = 12;
  fl.objectDrawingMode = false;
  tl = doc.getTimeline();
  tl.layers[0].name = "base";
  for (var k = 0; k < INSTANCES.length; k++) {
    (function (inst, k) {
      step("instance " + inst[1], function () {
        var sym = "Sym_" + inst[1];
        lib.addNewItem(inst[0], sym);
        lib.editItem(sym);
        var f = doc.getCustomFill(); f.style = "solid"; f.color = "#3366CC"; doc.setCustomFill(f);
        var s = doc.getCustomStroke(); s.style = "noStroke"; doc.setCustomStroke(s);
        doc.addNewRectangle({left: -10, top: -10, right: 10, bottom: 10}, 0, false, true);
        doc.exitEditMode();
        newLayer("L_" + inst[1]);
        var el = place("L_" + inst[1], sym, 20 + 30 * k, 50, inst[1]);
        el.actionScript = inst[2];
        return el.symbolType + " as=" + el.actionScript.length + "ch";
      });
    })(INSTANCES[k], k);
  }
  step("frame scripts", function () {
    var L = newLayer("actions");
    for (var n = 0; n < FRAMES.length; n++) {
      if (n > 0) tl.insertBlankKeyframe(n);
      tl.layers[L].frames[n].actionScript = FRAMES[n];
    }
    return FRAMES.length + " frames";
  });
  step("extend layers", function () {
    for (var i = 0; i < tl.layers.length; i++) {
      var n = tl.layers[i].frameCount;
      if (n < FRAMES.length) { doc.selectNone(); tl.setSelectedLayers(i, true); tl.currentLayer = i; tl.insertFrames(FRAMES.length - n, false, n); }
    }
    return "frameCount=" + tl.frameCount;
  });
  step("save FLA", function () { return fl.saveDocument(doc, DIR + "as2probe.fla"); });
  step("reopen + export SWF", function () {
    fl.closeDocument(doc, false);
    doc = fl.openDocument(DIR + "as2probe.fla");
    fl.outputPanel.clear();
    doc.exportSWF(DIR + "as2probe.swf", true);
    fl.outputPanel.save(DIR + "as2probe-output.txt");
    return FLfile.exists(DIR + "as2probe.swf");
  });
} catch (e) { FAILS.push("top: " + e); log("ERROR: " + e); }
log("FAILED STEPS: " + FAILS.length);
for (var i = 0; i < FAILS.length; i++) log("  - " + FAILS[i]);
try { if (doc) fl.closeDocument(doc, false); } catch (e) { log("close: " + e); }
log("DONE");
fl.quit(false);
'''

with open(os.path.join(HERE, "as2probe.jsfl"), "w", encoding="ascii", newline="\r\n") as f:
    f.write(JSFL % {
        "dir": json.dumps(DIR_URI),
        "frames": json.dumps(FRAMES),
        "instances": json.dumps(INSTANCES),
    })
with open(os.path.join(HERE, "probe-sources.json"), "w", encoding="utf-8") as f:
    json.dump({"frames": FRAMES, "instances": INSTANCES}, f, indent=1)
print("wrote", len(FRAMES), "frames,", len(INSTANCES), "instances")
