// Fixture builder: text  (run with tools/oracle/build-all.ps1)
// Static text: English multi-line, alignments left/center/right, Arial and
// Times New Roman, several sizes/colours, bold italic; static Arabic text in
// Tahoma (Flash 8 has no RTL/shaping, so it is stored and drawn as isolated
// letters left to right - expected); a dynamic text field with English text.
var NAME = "text";
var OUT = JZ_OUT;   // tests/fixtures/NAME/ (from the prelude build-all.ps1 prepends)
var LOG = JZ_LOG;   // target/oracle/logs/NAME.log
var FAILS = [];
FLfile.write(LOG, "");
function log(m) { FLfile.write(LOG, m + "\n", "append"); }
function step(label, fn) {
  try { var r = fn(); log("OK   " + label + (r === undefined ? "" : " -> " + r)); return true; }
  catch (e) { FAILS.push(label + ": " + e); log("FAIL " + label + ": " + e); return false; }
}
function codes(s) { var r = []; for (var i = 0; i < s.length; i++) { var h = s.charCodeAt(i).toString(16).toUpperCase(); while (h.length < 4) h = "0" + h; r.push("U+" + h); } return r.join(" "); }
function esc(s) { return String(s).replace(/\r/g, "\\r").replace(/\n/g, "\\n"); }
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
// Add a text field on the named layer and style it.
function text(layer, rect, str, attrs, props) {
  var t = doc.getTimeline(), L = useLayer(layer), before = t.layers[L].frames[0].elements.length;
  doc.addNewText(rect);
  doc.setTextString(str);
  var els = t.layers[L].frames[0].elements;
  if (els.length == before) throw new Error("addNewText created nothing");
  var el = els[els.length - 1];
  if (props) for (var p in props) el[p] = props[p];
  for (var a in attrs) el.setTextAttr(a, attrs[a]);
  doc.selectNone();
  var s = "type=" + el.textType + " lineType=" + el.lineType + " text=\"" + esc(el.getTextString()) + "\"";
  var ks = ["face", "size", "fillColor", "alignment", "bold", "italic"];
  for (var i = 0; i < ks.length; i++) s += " " + ks[i] + "=" + el.getTextAttr(ks[i]);
  s += " bounds=" + Math.round(el.left) + "," + Math.round(el.top) + " " + Math.round(el.width) + "x" + Math.round(el.height);
  if (el.name) s += " name=" + el.name;
  return s;
}

var doc = null, tl = null;
try {
  FLfile.createFolder(OUT);
  doc = fl.createDocument();
  tl = doc.getTimeline();
  step("stage 550x400 @ 12fps", function () {
    doc.width = 550; doc.height = 400; doc.frameRate = 12; doc.backgroundColor = "#FFFFFF";
    tl.layers[0].name = "static";
    return doc.width + "x" + doc.height;
  });
  step("static: Arial 18 black, left, 2 lines", function () {
    return text("static", {left: 20, top: 20, right: 260, bottom: 80}, "Left aligned, Arial 18\rsecond line of static text",
                {face: "Arial", size: 18, fillColor: "#000000", alignment: "left"});
  });
  step("static: Times New Roman 24 dark red, center, 2 lines", function () {
    return text("static", {left: 290, top: 20, right: 530, bottom: 90}, "Centered, Times 24\rline two",
                {face: "Times New Roman", size: 24, fillColor: "#CC0000", alignment: "center"});
  });
  step("static: Arial 12 blue, right, 3 lines", function () {
    return text("static", {left: 20, top: 110, right: 260, bottom: 180}, "Right aligned, Arial 12\ra third line follows\rand a fourth",
                {face: "Arial", size: 12, fillColor: "#0033CC", alignment: "right"});
  });
  step("static: Times New Roman 30 bold italic green", function () {
    return text("static", {left: 290, top: 110, right: 530, bottom: 160}, "Bold Italic 30",
                {face: "Times New Roman", size: 30, fillColor: "#008800", bold: true, italic: true});
  });
  step("static Arabic: Tahoma 28 (marhaban bil-alam)", function () {
    newLayer("arabic");
    var s = "\u0645\u0631\u062D\u0628\u0627 \u0628\u0627\u0644\u0639\u0627\u0644\u0645";
    var r = text("arabic", {left: 20, top: 200, right: 330, bottom: 250}, s, {face: "Tahoma", size: 28, fillColor: "#000000"});
    var el = tl.layers[layerIndex(tl, "arabic")].frames[0].elements[0];
    return r + " | stored codes: " + codes(el.getTextString());
  });
  step("dynamic: Arial 16, instance dyn_txt, single line", function () {
    newLayer("dynamic");
    return text("dynamic", {left: 20, top: 290, right: 330, bottom: 320}, "Dynamic text field",
                {face: "Arial", size: 16, fillColor: "#333333"}, {textType: "dynamic", lineType: "single line", name: "dyn_txt"});
  });
  step("layer actions: frame 1 trace", function () {
    var L = newLayer("actions");
    tl.layers[L].frames[0].actionScript = 'trace("text: frame 1");\ntrace("dyn_txt.text = " + dyn_txt.text);';
  });
  step("summary", function () {
    var s = "layers=" + tl.layers.length + " frames=" + tl.frameCount;
    for (var i = 0; i < tl.layers.length; i++) s += " | " + tl.layers[i].name + ":" + tl.layers[i].frames[0].elements.length;
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
