// Fixture builder: smoke  (run with tools/oracle/build-all.ps1 -Names smoke)
// The first hand-made fixture: a red box movie clip on a 20-frame motion tween
// and a frame script that traces "hello from Flash 8". Unlike the builders in
// fixtures\, it publishes straight from the edited document (no reopen, see
// MANIFEST J15) and has no step log; that is how the committed fixture was made.
var NAME = "smoke";
var OUT = JZ_OUT;   // tests/fixtures/smoke/ (from the prelude build-all.ps1 prepends)
var LOG = JZ_LOG;   // target/oracle/logs/smoke.log
function log(m) { FLfile.write(LOG, m + "\n", "append"); }
FLfile.write(LOG, "");
try {
  FLfile.createFolder(OUT);
  var doc = fl.createDocument();
  doc.width = 550; doc.height = 400; doc.frameRate = 24;
  var tl = doc.getTimeline();
  var fill = doc.getCustomFill();
  fill.style = "solid"; fill.color = "#CC3333";
  doc.setCustomFill(fill);
  doc.addNewRectangle({left: 40, top: 60, right: 160, bottom: 140}, 0, false, true);
  log("rect ok, elements=" + tl.layers[0].frames[0].elements.length);
  doc.selectAll();
  doc.convertToSymbol("movie clip", "Box", "center");
  log("symbol ok, items=" + doc.library.items.length);
  tl.insertKeyframe(19);
  tl.currentFrame = 19;
  doc.selectAll();
  doc.moveSelectionBy({x: 300, y: 150});
  tl.createMotionTween(0, 18);
  log("tween ok, frameCount=" + tl.frameCount);
  tl.addNewLayer("actions");
  tl.layers[tl.currentLayer].frames[0].actionScript = 'trace("hello from Flash 8");';
  log("actions ok on layer " + tl.currentLayer + " name=" + tl.layers[tl.currentLayer].name);
  fl.saveDocument(doc, OUT + NAME + ".fla");
  log("saved");
  doc.exportSWF(OUT + "flash8.swf", true);
  log("exported");
  fl.closeDocument(doc, false);
} catch (e) { log("ERROR: " + e); }
log("DONE");
fl.quit(false);
