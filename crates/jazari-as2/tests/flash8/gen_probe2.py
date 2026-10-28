"""Flash 8 oracle probe for jazari-as2: writes as2probe2.jsfl, which makes the Flash 8 IDE compile
AS2 snippets so their bytecode can be compared with what Rascal produces.

Each frame script is on its own keyframe so a compile error in one does not hide the others;
each instance gets its own symbol (button or movie clip) and layer.

Run (one Flash instance at a time):
    python gen_probe2.py
    powershell -File C:/dev/oracle/run-jsfl.ps1 -Script <this dir>/as2probe2.jsfl -Log C:/dev/jazari/target/a-as2/oracle/as2probe2.log
then copy as2probe2.swf and as2probe2-output.txt from C:/dev/jazari/target/a-as2/oracle next to this script.
src/oracle_tests.rs compares them with our compiler.
"""
import json
import os

HERE = os.path.dirname(os.path.abspath(__file__))
DIR_URI = "file:///C|/dev/jazari/target/a-as2/oracle/"

FRAMES = [
    'trace("p2");\n',
]

INSTANCES = [
    ("button", "b_press", 'on (press) { trace(1); }\n'),
    ("button", "b_release", 'on (release) { trace(1); }\n'),
    ("button", "b_relout", 'on (releaseOutside) { trace(1); }\n'),
    ("button", "b_rollover", 'on (rollOver) { trace(1); }\n'),
    ("button", "b_rollout", 'on (rollOut) { trace(1); }\n'),
    ("button", "b_dragover", 'on (dragOver) { trace(1); }\n'),
    ("button", "b_dragout", 'on (dragOut) { trace(1); }\n'),
    ("button", "b_comment_only", 'on (press) {\n\t// nothing\n}\non (release) { /* x */ }\n'),
    ("button", "b_arabic_key", u'on (keyPress "م") { trace(1); }\n'),
    ("button", "b_multichar_key", 'on (keyPress "ab") { trace(1); }\n'),
    ("button", "b_bad_key", 'on (keyPress "<Foo>") { trace(1); }\n'),
    ("button", "b_bad_event", 'on (foo) { trace(1); }\n'),
    ("button", "b_dup_handlers", 'on (press) { trace(1); }\non (press) { trace(2); }\n'),
    ("button", "b_dup_event", 'on (press, press) { trace(1); }\n'),
    ("button", "b_only_comments", '// just a comment\n/* and another */\n'),
    ("button", "b_unbalanced", 'on (press) { trace(1);\n'),
    ("button", "b_nested_on", 'on (press) {\n\ton (release) { trace(1); }\n}\n'),
    ("button", "b_no_braces", 'on (release) trace(1);\n'),
    ("button", "b_syntax_err", 'on (release) {\n\ttrace(1;\n}\n'),
    ("button", "b_key_quote", "on (keyPress '<Enter>') { trace(1); }\n"),
    ("button", "b_key_space", 'on (keyPress " ") { trace(1); }\n'),
    ("button", "b_space_names", 'on ( press ,\n release ) { trace(1); }\n'),
    ("button", "b_empty_parens", 'on () { trace(1); }\n'),
    ("movie clip", "m_bad_event", 'onClipEvent (foo) { trace(1); }\n'),
    ("movie clip", "m_outside", 'trace("x");\nonClipEvent (load) { trace(1); }\n'),
    ("movie clip", "m_mouse_on_clip_ev", 'onClipEvent (press) { trace(1); }\n'),
    ("movie clip", "m_clip_ev_in_on", 'on (load) { trace(1); }\n'),
    ("movie clip", "m_semicolon", 'onClipEvent (load) { trace(1); };\n'),
    ("movie clip", "m_dragover", 'on (dragOver, dragOut) { trace(1); }\n'),
    ("movie clip", "m_empty_on", 'on (press) { }\n'),
]

JSFL = r'''
var DIR = %(dir)s;
var LOG = DIR + "as2probe2.log";
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
        var el = place("L_" + inst[1], sym, 15 + 12 * k, 50, inst[1]);
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
  step("save FLA", function () { return fl.saveDocument(doc, DIR + "as2probe2.fla"); });
  step("reopen + export SWF", function () {
    fl.closeDocument(doc, false);
    doc = fl.openDocument(DIR + "as2probe2.fla");
    fl.outputPanel.clear();
    doc.exportSWF(DIR + "as2probe2.swf", true);
    fl.outputPanel.save(DIR + "as2probe2-output.txt");
    return FLfile.exists(DIR + "as2probe2.swf");
  });
} catch (e) { FAILS.push("top: " + e); log("ERROR: " + e); }
log("FAILED STEPS: " + FAILS.length);
for (var i = 0; i < FAILS.length; i++) log("  - " + FAILS[i]);
try { if (doc) fl.closeDocument(doc, false); } catch (e) { log("close: " + e); }
log("DONE");
fl.quit(false);
'''

with open(os.path.join(HERE, "as2probe2.jsfl"), "w", encoding="ascii", newline="\r\n") as f:
    f.write(JSFL % {
        "dir": json.dumps(DIR_URI),
        "frames": json.dumps(FRAMES),
        "instances": json.dumps(INSTANCES),
    })
with open(os.path.join(HERE, "probe2-sources.json"), "w", encoding="utf-8") as f:
    json.dump({"frames": FRAMES, "instances": INSTANCES}, f, indent=1)
print("wrote", len(FRAMES), "frames,", len(INSTANCES), "instances")
