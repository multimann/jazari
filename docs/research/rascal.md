# Research: rascal (verified)

## Recommendation

Use Rascal 0.3.6 as a library dependency with `rascal = "=0.3.6"` and `swf = "0.3.0"`. Compile with CompileOptions::default().with_swf_version(8) and OptimizationOptions::full(). Rascal has no API for raw bytes, but standalone snippets do work. Serve each frame script or handler body from an in-memory SourceProvider under a virtual path (e.g. \"@frame/<id>.as\"), build, call to_swf, then parse the resulting uncompressed SWF with swf::decompress_swf and parse_swf and take the single Tag::DoAction payload. It already ends with ActionEnd 0x00 and can be placed directly into our own DoAction, ButtonAction.action_data or ClipAction.action_data. The probe confirmed this on this machine at SWF version 8, on CPU only.\n\nPut a thin front-end of our own in front of Rascal, because Rascal does not understand on()/onClipEvent at all and accepts them without error but compiles them wrongly:\n(1) Split instance scripts (xfl.xsd:4398) into handlers with a small tokenizer that respects strings and comments.\n(2) Compile each body separately, with everything outside the body replaced by spaces of the same byte length, so Rascal's line and column match the original text.\n(3) Map events to swf::ButtonActionCondition for buttons (DefineButton2) and to swf::ClipEventFlag plus key_code for clips (PlaceObject2 ClipActions), using the mappings in F7.\n(4) Handle #initclip and, optionally, scene-qualified gotos in the same front-end.\n\nCompile classes in two passes. Frame and handler snippets are compiled against stub classes (`class a.b.C {}`) while the provider records the requested class names. Then one real class build produces the __Packages.<FQN> DoInitAction modules in dependency order; discard that build's DoAction. This prevents Rascal's automatic `X.main(this)` from leaking into frames and compiles each class only once.\n\nWrap build() and compile() in catch_unwind and keep panic = \"unwind\". Strip the UTF-8 BOM yourself. For the Output panel, take byte spans from the Error's Debug output (FileId mapped to its path), compute line and column from our own copy of the text, and fall back to the first '╭▸ path:L:C' line of to_string_plain().\n\nIn parallel, prepare small additive upstream PRs: action_blocks(), structured Error::diagnostics(), an option to disable the static-main entry point, and the parser gaps (postfix ++ on members, do-while, dotted type annotations). Hold a pinned fork via [patch.crates-io] only if a PR is not accepted. The rest of this design does not depend on any of these PRs landing.

## Dependencies

- rascal = "=0.3.6" (crates.io; MIT OR Apache-2.0; pure Rust; already in the tree as ruffle_core@07d3e430's build-dependency via build_playerglobal)
- swf = "0.3.0" (crates.io; same copy rascal links, used to parse Rascal's FWS output and to write our CWS SWF 8; default features flate2+lzma)
- Transitive through rascal: indexmap 2.14 (serde), serde 1 (derive), annotate-snippets 0.12.16+, winnow 1.0.x
- Transitive through swf 0.3.0: bitflags 2, bitstream-io 4, encoding_rs, enum-map 2, flate2 1.1 (miniz_oxide/zlib-rs, crc32fast), log, lzma-rs 0.3, num-derive, num-traits, simple_asn1 0.6 (num-bigint, thiserror, time)
- Rust toolchain >= 1.88 (edition 2024 + let-chains; no rust-version declared). Built with rustc 1.99.1 at C:\dev\rust
- Optional later: [patch.crates-io] rascal = { git = <fork>, rev = ... } for the upstream patches; this also patches ruffle_core's build-dep use, so changes must be additive

## Findings (verifier status in brackets; corrections override the claim)

- **F1-api-surface** [confirmed] The public API is small. The crate root re-exports CompileOptions, CompiledProgram, OptimizationOptions, Program, ProgramBuilder, SwfOptions, FileSystemSourceProvider and SourceProvider. Every other module is private, including error, internal, provider, sources and swf. The flow is: ProgramBuilder::new(provider: P) where P: SourceProvider, then add_script(&mut self, path: &str), add_class(&mut self, path: &str) or add_pcode(&mut self, path: &str), with_compile_options(self, CompileOptions) -> Self, then build(self) -> Result<Program, Error>. After that, Program::compile(self) -> CompiledProgram, then CompiledProgram::to_swf(&self, &SwfOptions) -> swf::error::Result<Vec<u8>>. The README 'Library' example is out of date: it uses `rascal::provider::...`, chained add_script, compile(15) and to_swf(24.0), none of which compile against 0.3.6.
  - evidence: C:\dev\ref\rascal\crates\rascal\src\lib.rs:1-13; C:\dev\ref\rascal\crates\rascal\src\program.rs:115 (compile), 147-151 (to_swf), 196-234 (ProgramBuilder: add_script 205, add_pcode 209, add_class 213, with_compile_options 217, new 224, build 234); C:\dev\ref\rascal\README.md (Library section) versus program.rs
- **F2-options** [confirmed] SwfOptions has private fields and builder setters: with_frame_rate(f32), with_stage_size(x_min, y_min, x_max, y_max: f64) in pixels, and with_network_sandbox(bool). Its defaults are 24.0 fps, a 0..100 x 0..100 px stage and no network sandbox. CompileOptions has with_swf_version(u8) and with_optimizations(OptimizationOptions); its defaults are swf_version 15 and OptimizationOptions::full(). OptimizationOptions::full() and ::none() can be adjusted with with_fold_constants(bool) and with_promote_variables_to_registers(bool).
  - evidence: C:\dev\ref\rascal\crates\rascal\src\program.rs:17-38 (SwfOptions and defaults), 40-64 (setters), 66-91 (CompileOptions, default 15 at line 75), 153-194 (OptimizationOptions)
- **F3-whole-swf-only** [confirmed] Rascal has no public API that returns raw AVM1 bytes. The action encoder (ActionEncoder) and the compiled Actions are crate-private, and the only output is a whole uncompressed SWF ('FWS'). Its tag order is: FileAttributes, SetBackgroundColor(WHITE), one DoAction per pcode file, one DoAction for all scripts concatenated, then for each class or interface a DefineSprite{id: 1.., num_frames: 0} + ExportAssets("__Packages.<FQN>") + DoInitAction{id}, then ShowFrame. The header has num_frames = 1. Each action block already ends with ActionEnd (0x00). A snippet can therefore be extracted with a round trip: swf::decompress_swf, swf::parse_swf, then take Tag::DoAction(bytes) and pair DoInitAction with ExportAssets by id. A script that produces no code emits no DoAction. CompiledProgram implements serde::Serialize, but its fields are pub(crate), so serializing it gives pcode as data, not bytes.
  - evidence: C:\dev\ref\rascal\crates\rascal\src\swf.rs:10-94 (pcode_to_swf: End appended at 18/25/33; DoAction at 48-50; DefineSprite/ExportAssets/DoInitAction at 58-73; Compression::None at 79; version at 80; num_frames 1 at 88); program.rs:139-151. Probe output (CPU only, built in the scratchpad): C:\Users\A04E~1\AppData\Local\Temp\claude\C--Users-----------CluadeCowork---------------\b1f4a133-7216-4e29-a076-c5e0c76642d5\scratchpad\rascal_probe\out.txt shows magic "FWS", version 8, 'DoAction 48 bytes, last byte 0x00', the 'empty script' case with no DoAction, and the class modules 'DefineSprite id=1 / ExportAssets __Packages.com.x.Foo / DoInitAction id=1'
- **F4-snippets-via-provider** [confirmed] Standalone snippets do compile: implement SourceProvider { fn load(&self, path: &str) -> Result<String, std::io::Error>; fn is_file(&self, path: &str) -> bool } so that it serves a virtual path such as "@frame/12.as" from memory, then call add_script on that path. The provider also resolves classes: a class lookup passes the path "a/b/C.as" (from type_path_to_file_path), and #include passes its string literal unchanged. Both are relative to whatever roots the provider uses; FileSystemSourceProvider::with_roots(Vec<PathBuf>) is the classpath. Rascal has no `impl SourceProvider for &P`, so to share one provider across builds you need a newtype. Ruffle's test harness does the same thing with a VFS-backed provider.
  - evidence: C:\dev\ref\rascal\crates\rascal\src\provider.rs:3-46; C:\dev\ref\rascal\crates\rascal\src\internal\as2.rs:10-12 (type_path_to_file_path); C:\dev\ref\rascal\crates\rascal\src\internal\as2\resolver.rs:569-591 (#include goes through provider.get_or_load(path)); C:\dev\ref\ruffle\tests\framework\src\compiler\rascal.rs:149-163 (VfsSourceProvider). Probe C:\Users\A04E~1\AppData\Local\Temp\claude\C--Users-----------CluadeCowork---------------\b1f4a133-7216-4e29-a076-c5e0c76642d5\scratchpad\rascal_probe\src\main.rs (in-memory provider; all cases built)
- **F5-swf-version** [confirmed] Set version 8 with CompileOptions::default().with_swf_version(8); the CLI equivalent is `-v 8` and its default is 15. Only three things depend on the version: (a) the header version byte; (b) when a class extends another, versions 7 and up use the Extends opcode, while lower versions emit `prototype = new Super()`; (c) promoting function locals to DefineFunction2 registers runs only when version >= 7. At version 8 both (b) and (c) are active, and all emitted opcodes exist in SWF 7. FileAttributes is always the first tag, which SWF 8 requires. String constants are UTF-8; the probe showed an Arabic literal stored as d9 85 d8 b1... in the ConstantPool, which is correct for SWF 6 and later. Ruffle compiles its own AVM1 playerglobal at version 8.
  - evidence: C:\dev\ref\rascal\crates\rascal\src\program.rs:75, 82-85, 103-113 (>= 7 gate at line 109); C:\dev\ref\rascal\crates\rascal\src\internal\as2_codegen.rs:104-122 (Extends at >= 7); C:\dev\ref\rascal\crates\rascal\src\swf.rs:44-47, 80; C:\dev\ref\rascal\crates\cli\src\main.rs:61-68; grep of swf_version (only these sites); C:\dev\ref\ruffle\core\build_playerglobal\src\avm1.rs:12; probe out.txt (ConstantPool with UTF-8 Arabic bytes)
- **F6-no-button-clip-handlers** [confirmed] on(...) and onClipEvent(...) are not supported. They are not keywords and not statements. Worse, Rascal accepts them without any error: `on (press) { trace("x"); }` compiles to a call `on(press)` followed by a plain block, and onClipEvent(enterFrame){...} becomes a call `onClipEvent(enterFrame)` followed by the body running once. Rascal has no notion of DefineButton2 ButtonCondAction or PlaceObject2 ClipActions. Instance scripts must therefore be split by our own front-end before Rascal sees them, and each handler body compiled as a separate snippet.
  - evidence: C:\dev\ref\rascal\crates\rascal\src\internal\as2\lexer\tokens.rs:128-175 (Keyword enum: no On, OnClipEvent or Do); C:\dev\ref\rascal\crates\rascal\src\internal\as2\parser\statement.rs:17-152 (statement grammar); grep of crates for 'onClipEvent|ClipAction|ButtonCond' finds nothing; probe out.txt cases 'button on(press) passed directly' (pool ["press","on","x"], CallFunction) and 'onClipEvent(enterFrame) passed directly'
- **F7-handler-body-technique** [confirmed] Handler bodies can be compiled with line and column positions kept. Replace every character outside the body's braces with spaces, keep the newlines, and compile the result as a script. The probe reported btn_inst_1.as:3:11 for an error on line 3 of the original `on(release, keyPress "<Enter>") {...}` text. Flag mapping follows Ruffle and the swf crate. Buttons use swf::ButtonActionCondition: press→OVER_UP_TO_OVER_DOWN, release→OVER_DOWN_TO_OVER_UP, releaseOutside→OUT_DOWN_TO_IDLE, rollOver→IDLE_TO_OVER_UP, rollOut→OVER_UP_TO_IDLE, dragOver→OUT_DOWN_TO_OVER_DOWN, dragOut→OVER_DOWN_TO_OUT_DOWN, keyPress→ButtonActionCondition::from_key_code(k) with <Left>=1 <Right>=2 <Home>=3 <End>=4 <Insert>=5 <Delete>=6 <Backspace>=8 <Enter>=13 <Up>=14 <Down>=15 <PageUp>=16 <PageDown>=17 <Tab>=18 <Escape>=19 <Space>=32, otherwise ASCII. Clip events use swf::ClipEventFlag: LOAD, ENTER_FRAME, UNLOAD, MOUSE_MOVE, MOUSE_DOWN, MOUSE_UP, KEY_DOWN, KEY_UP, DATA, INITIALIZE, CONSTRUCT, and for on() placed on a MovieClip: PRESS, RELEASE, RELEASE_OUTSIDE, ROLL_OVER, ROLL_OUT, DRAG_OVER, DRAG_OUT, KEY_PRESS (+ ClipAction.key_code). The swf writer copies action_data verbatim, so Rascal's trailing 0x00 End must stay. crates.io swf 0.3.0 types.rs is byte-identical to Ruffle's in-tree copy.
  - evidence: Probe out2.txt ('on() body blanked' gives btn_inst_1.as:3:11); C:\dev\ref\ruffle\swf\src\types.rs:455-498 (ClipAction{events, key_code: Option<u8>, action_data: &[u8]}, ClipActions{all_event_flags, records}, ClipEventFlag), 1112-1143 (ButtonAction{conditions, action_data}, ButtonActionCondition, from_key_code); C:\dev\ref\ruffle\core\src\display_object\avm1_button.rs:455-495 (event to condition); C:\dev\ref\ruffle\core\src\events.rs:684-700 (ButtonKeyCode values); C:\dev\ref\ruffle\swf\src\write.rs:1886-1905; `diff` of C:\dev\rust\cargo\registry\src\index.crates.io-1949cf8c6b5b557f\swf-0.3.0\src\types.rs against the Ruffle copy is empty; XFL sources: C:\dev\ref\xfl-schema\xfl.xsd:2558 (DOMFrame Actionscript), 4398 (symbol instance Actionscript), 4659 (ActionscriptType/script)
- **F8-include-classes** [confirmed] #include "path" is supported since 0.3.3/0.3.4 and resolves through the provider; a missing file reports "Could not load file '...': File not found". No other directive exists: #initclip/#endinitclip fails with "expected `include`". AS2 classes and interfaces in external .as files work: extends, implements, static members, get/set properties, super(), import, and dotted packages. Classes are pulled in automatically when a script uses import, new X, a dotted path a.b.C, or a type annotation. The output is one __Packages.<FQN> DoInitAction per class, in dependency order. A class defined inside a script is rejected ('Classes may only be defined in external ActionScript 2.0 class scripts.'), as in Flash.
  - evidence: C:\dev\ref\rascal\crates\rascal\src\internal\as2\parser\statement.rs:118-134; resolver.rs:140-156 (add_dependency), 569-591 (include), 689-702, 838-846, 1014-1017 (type names are required dependencies); program.rs:353-384 (class loading loop), 122-129 (rev order); C:\dev\ref\rascal\crates\rascal\CHANGELOG.md (0.3.3/0.3.4 #include); probe out2.txt 'class features' (IThing, Base, Derived modules); probe out.txt '#initclip' error
- **F9-static-main-gotcha** [confirmed] Any loaded class with `static function main()` causes `<Class>.main(this)` to be appended to the compiled script. This is the MTASC entry-point convention, which Flash IDE does not have. If a frame script references such a class, that frame's DoAction calls main. If two loaded classes have a static main, build() fails with the hard error 'Conflicting entry points found on classes: ...'. No option turns this off. A probe confirmed a workaround for frame snippets: while compiling them, have the provider return empty stubs `class a.b.C {}` for class files and record the requested names; then compile the real classes once in a separate build and keep only their DoInitAction modules. The second build still fails if two real classes both have a static main.
  - evidence: C:\dev\ref\rascal\crates\rascal\src\program.rs:353-393, 409-447 (call_main_method); probe out.txt case 'include + class with static main' (frame DoAction pool contains "this","main"; CallMethod); C:\Users\A04E~1\AppData\Local\Temp\claude\C--Users-----------CluadeCowork---------------\b1f4a133-7216-4e29-a076-c5e0c76642d5\scratchpad\rascal_probe\src\bin\stub.rs, whose run output shows pass 1 with no main in the pool and requested classes [com.x.Foo, com.x.Other], then pass 2 failing with 'Conflicting entry points'
- **F10-language-coverage** [confirmed] The probe compiled these at version 8 without problems: _global, X.prototype.m = function, getURL (1-3 arguments, GET/POST), setInterval, onEnterFrame/onRelease assignment, tellTarget, with, switch, try/catch/finally/throw, for..in, typeof, instanceof, delete, ifFrameLoaded, set()/eval(), duplicateMovieClip, attachMovie, fscommand, newline, loadMovieNum, startDrag, removeMovieClip, unloadMovie, stopAllSounds, toggleHighQuality, targetPath, call, print, getVersion, getTimer, random, int, length, chr, ord, mb*, _root/_level0/_parent special properties, function hoisting, arguments, hex/float literals, ternary, comma in for, Key listeners, prefix ++/-- on members, and typed vars with built-in types. Special functions are matched case-insensitively, as in Flash.
  - evidence: C:\dev\ref\rascal\crates\rascal\src\internal\as2\resolver\special_functions.rs:13-58; samples in C:\dev\ref\rascal\samples\as2\ (switch, with, try_catch, tell_target, legacy_get_set, include ...); probe C:\Users\A04E~1\AppData\Local\Temp\claude\C--Users-----------CluadeCowork---------------\b1f4a133-7216-4e29-a076-c5e0c76642d5\scratchpad\rascal_probe\out3.txt and out4.txt (each construct compiled separately)
- **F11-unsupported-partial** [confirmed] Unsupported or partial at 0.3.6, each verified by probe or source:
(1) on()/onClipEvent: accepted without error but compiled wrongly (F6).
(2) do { } while (c): there is no `do` keyword; it fails with an error whose message is EMPTY.
(3) Postfix ++/-- on member or index expressions (o.a++, this._x++, arr[0]++) fails with 'invalid expression' because can_postfix only accepts bare identifiers. Prefix ++o.a works.
(4) Fully qualified type annotations (var b:flash.display.BitmapData, p:flash.geom.Point) fail; they need an import.
(5) #initclip/#endinitclip is not supported.
(6) Scenes do not exist: gotoAndPlay/gotoAndStop("Scene", n) checks then IGNORES the scene, and nextScene()/prevScene() go to frame 0.
(7) Non-ASCII identifiers are rejected (var س: 'invalid variable name'). Arabic string literals are fine.
(8) Open bugs: #87 `new F().x` is parsed as new (F().x); #71 makeFunc()() and new (makeClass())() are miscompiled; @PCode cannot contain comments (#76) or strings with } (#1); __bytecode__ is not supported (#88).
(9) The default constructor has no implicit super() (TODO).
(10) A single action longer than 65535 bytes is a hard error (#65, which was only mitigated).
(11) A UTF-8 BOM shifts error spans by 3 bytes: the lexer strips the BOM but diagnostics use the unstripped text.
  - evidence: C:\dev\ref\rascal\crates\rascal\src\internal\as2\lexer\tokens.rs:128-175; parser\expression.rs:50-52 (can_postfix), 277-284 (type_name = single identifier); parser\statement.rs:118-134; resolver\special_functions.rs:168-200, 600-640; lexer.rs:17-22 (BOM strip), 302-307 (ASCII identifiers); resolver.rs:545 ('TODO super()'); swf.rs:113-134 (length overflow errors); GitHub https://api.github.com/repos/Dinnerbone/Rascal/issues (open #1, #71, #76, #87, #88; closed #65); probe out.txt (do-while gives an empty-label error, BOM case reports 1:9 instead of 2:1, Arabic identifier error) and out3.txt/out4.txt (L5/L8/L9/L10 postfix failures, L31/L45 dotted types, L1 arr[0]++ fails, L2 ++arr[0] ok)
- **F12-panics** [confirmed] Some inputs panic instead of returning an error. In compile(): `"abc" = 5;` hits unimplemented!("Cannot set a direct value"), and other unimplemented! calls in access.rs cover get-and-set, new and call on direct values or special properties. Invalid inline @PCode panics in codegen. A malformed `@PCode` token panics in the lexer, which runs during build(). An editor must wrap build() and compile() in std::panic::catch_unwind, which requires panic = "unwind" in our release profile, or run them in a worker thread or subprocess.
  - evidence: C:\dev\ref\rascal\crates\rascal\src\internal\as2_codegen\access.rs:125, 150, 176, 197, 220; as2_codegen\statement.rs:105-111; internal\as2\lexer.rs:216-217; probe out.txt 'assign to literal' and 'inline pcode bad' both give 'PANIC caught'
- **F13-diagnostics** [partly-wrong] Rascal reports errors only; it has no warnings. build() returns rascal::Error, which is Debug, Display (ANSI-styled outside tests), std::error::Error, Send and Sync. Its public methods are to_string_plain() and to_string_styled(), which render annotate-snippets text in the form '╭▸ path:LINE:COL' with 1-based lines and 1-based columns counted in chars (tabs are rendered as 4 spaces). The header holds only the first error of each file; later errors appear as gutter lines. No public structured access exists: ErrorSet and ParsingError{error: String, span: Span{start, end: byte offsets, file: Option<FileId>}} are crate-private. The Debug output does contain everything, e.g. `ParsingError { error: "...", span: 21..29 }` grouped under Some(FileId(n)), plus SourceSet{sources:[SourceFile{path}]} where n is the index. That can be parsed as a stopgap, though the Debug format is not a stable API. A syntax error stops parsing the file, so only the first parse error is reported; resolver errors (wrong argument counts, missing classes, ...) are all reported. Missing-file errors (io_errors) and misc errors such as conflicting entry points carry no span.
  - evidence: C:\dev\ref\rascal\crates\rascal\src\error.rs:8-13, 39-81 (report), 97-137; C:\dev\ref\rascal\crates\rascal\src\internal\as2\error.rs:6-22; C:\dev\ref\rascal\crates\rascal\src\internal\span.rs:326-331; program.rs:266-273 (parse error returns early); grep for 'warn' in src finds nothing; CHANGELOG 0.3.4 (Sync error); probe bin col.rs output ('@a.as:1:17' for byte offset 21 after an Arabic string; Debug shows span: 21..29 and 32..40); snapshots C:\dev\ref\rascal\crates\rascal\src\snapshots\rascal__program__tests__fail_samples@geturl.as.snap
  - CORRECTION: The citation C:\dev\ref\rascal\crates\rascal\src\internal\span.rs:326-331 is wrong: span.rs has only 121 lines. Span{start, end, file: Option<FileId>} is at span.rs:91-95, and its custom Debug impl, which prints only start..end, is at span.rs:117-121. SourceFile has a custom Debug that prints only the path (sources.rs:47-53). FileId(n) indexes SourceSet.sources in load order (sources.rs:26, 37). The rest is confirmed: error.rs:8-13 and 97-137, ParsingError at internal\as2\error.rs:6-10, and the early return at program.rs:266-273. Re-running the already-built probe col.exe gives '@a.as:1:17' for byte 21 after a 5-letter Arabic string (16 chars precede it), so columns are 1-based and counted in chars. Its Debug output is `span: 21..29` under `Some(FileId(0))`.
- **F14-ruffle-usage** [confirmed] Ruffle at rev 07d3e430 uses Rascal in two places. (1) ruffle_core's build.rs, through the build-dependency build_playerglobal, compiles core/src/avm1/globals/globals.as (which uses #include) with FileSystemSourceProvider::with_root, CompileOptions::default().with_swf_version(8) and SwfOptions::default(), producing playerglobal_avm1.swf. (2) The test framework's RascalCompiler uses a VfsSourceProvider and per-test swf_version, frame_rate, stage_rect, use_network and optimization flags. The workspace requests rascal = "0.3.5" and Cargo.lock resolves 0.3.6, which links crates.io swf 0.3.0, separate from Ruffle's in-tree path copy of swf 0.3.0. As a result, depending on ruffle_core already compiles Rascal in our tree.
  - evidence: C:\dev\ref\ruffle\Cargo.toml:121; C:\dev\ref\ruffle\Cargo.lock:4476-4486 and 5761-5792 (two swf 0.3.0 packages); C:\dev\ref\ruffle\core\Cargo.toml:97-98; C:\dev\ref\ruffle\core\build.rs:7-8; C:\dev\ref\ruffle\core\build_playerglobal\src\avm1.rs:1-19; C:\dev\ref\ruffle\core\src\avm1\globals\globals.as (#include lines); C:\dev\ref\ruffle\tests\framework\src\compiler\rascal.rs:8-163
- **F15-license-msrv-deps** [confirmed] License: MIT OR Apache-2.0, compatible with an Unlicense app, but the MIT/Apache notices must ship in the portable bundle. MSRV is not declared: there is no rust-version, and crates.io rust_version is null. Edition 2024 needs 1.85, and let-chains (program.rs:331-342) need Rust >= 1.88. The installed toolchain, rustc 1.99.1, built it. Library dependencies: indexmap 2.14 (serde), serde 1 (derive), annotate-snippets 0.12.16 (anstyle, memchr, unicode-width), winnow 1.0 (its anstream/terminal_size only come with the winnow-debug feature), and swf 0.3.0. swf brings bitflags 2, bitstream-io 4, encoding_rs, enum-map, flate2 (miniz_oxide/zlib-rs, crc32fast), log, lzma-rs, num-derive, num-traits, and simple_asn1 (num-bigint, thiserror, time). All of these are pure Rust with no system libraries or runtime dependencies, so they fit the portable constraint. A clean dev build of the probe (rascal + swf + deps) took 54.5 s with -j 2 on this PC, CPU only. Latest release is 0.3.6 (2026-09-06), and the checkout HEAD 3f1a80c is tagged rascal_cli-v0.3.6.
  - evidence: C:\dev\ref\rascal\Cargo.toml:9-17; C:\dev\ref\rascal\crates\rascal\Cargo.toml:12-17, 22-24; C:\dev\ref\rascal\LICENSE-MIT, LICENSE-APACHE; C:\dev\ref\rascal\Cargo.lock (flate2 deps crc32fast/miniz_oxide/zlib-rs; simple_asn1 deps; swf deps); https://crates.io/api/v1/crates/rascal (max_version 0.3.6, rust_version None); C:\dev\ref\ruffle\swf\Cargo.toml (default features flate2 + lzma); probe cargo build log ('Finished dev ... in 54.52s')

## Risks

- Rascal accepts on(...) and onClipEvent(...) without error and compiles them wrongly, as calls named `on`/`onClipEvent` followed by a block. If an instance script ever reaches Rascal without being split by our front-end, the SWF will misbehave without any error. Add a guard: reject or route any snippet whose first statement is `on (` or `onClipEvent (`.
- Panics: compile() panics on `"abc" = 5;`, and on related cases in access.rs and invalid inline @PCode; the lexer panics on a malformed @PCode token. Editor checks must use catch_unwind, which needs panic = "unwind", or isolate the compiler in a worker thread or subprocess. A panic must become a Diagnostic, not crash the app.
- Static main: Rascal treats any class with `static function main()` as the entry point. Frame scripts that reference such a class get `X.main(this)` appended (fixed by the stub pass), and two such classes make the class pass fail with 'Conflicting entry points' (no fix without patching Rascal).
- The diagnostics API is unstructured. Only rendered text (to_string_plain) is public, and its header shows only the first error per file. Parsing Debug output for byte spans depends on an unstable format. Pin rascal "=0.3.6" and cover the parser with a unit test that breaks on upgrade.
- Language gaps that real Flash 8 content uses: no do-while (and its error message is empty), no postfix ++/-- on members or indexes (this._x++), no fully qualified type annotations without import, no #initclip, no scenes (gotoAndPlay("Scene 2", n) ignores the scene; next/prevScene go to frame 0), ASCII-only identifiers, open miscompilations #87 (new F().x) and #71 (f()()). Legacy FLA content that compiled in Flash 8 may fail or behave differently.
- BOM: Rascal strips the UTF-8 BOM before lexing but renders errors against the unstripped text, so positions are off by 3 bytes. Strip the BOM ourselves for snippets and disk files; Flash-era .as files often have one.
- Cost of the whole-SWF round trip: every snippet compile serializes and re-parses a tiny SWF. This is negligible per frame, but #include files are re-lexed for each snippet that includes them. Cache results by hash of (snippet text, include texts).
- Character-ID collisions: Rascal numbers its class sprites from 1. The publisher must re-number them, dedupe modules by __Packages name, and place them before the first DoAction of the class export frame.
- Two copies of `swf 0.3.0` will be compiled: Ruffle's git path crate and the crates.io one. Their types are identical but cannot be mixed across crate boundaries. Pass only &[u8] between our publisher and ruffle_core.
- License compliance for the portable bundle: the MIT/Apache notices of rascal and its dependency tree must ship with the binary, e.g. a generated THIRD-PARTY-LICENSES file.

## Open questions

- Will upstream (Dinnerbone/Rascal) accept small additive PRs: (a) CompiledProgram::action_blocks() returning initializer and module bytes without building a SWF, (b) Error::diagnostics() returning path, message and byte range, (c) CompileOptions::with_entry_point(false) to stop auto-calling static main, (d) postfix ++/-- on Field and Index, do-while, dotted type annotations? If not, should we keep a fork via [patch.crates-io], which also changes Ruffle's playerglobal build?
- How should scenes be compiled? Flash resolves gotoAndPlay("Scene", n), nextScene() and prevScene() to absolute frames on the main timeline at compile time, and Rascal cannot. Should our front-end rewrite these calls into absolute frame numbers before Rascal, keeping offsets, or should M1 ship without scene support?
- Base directory for #include: Flash resolves it relative to the FLA, and for nested includes possibly relative to the including file. Rascal passes the literal path to the provider. Confirm Flash 8 behaviour before fixing the provider's root order.
- Where should class DoInitActions go: frame 1, or the 'Export frame for classes' value from PublishSettings? This needs a check of the XFL PublishSettings fields.
- Should M1 accept Rascal's language gaps and show a clear 'not supported by the compiler' message, or must legacy FLA test content (do-while, this._x++) compile on day one, which forces patches?
- How should the instance script splitter handle text outside handlers? Flash reports an error for code outside on()/onClipEvent blocks on instances; confirm the exact Flash 8 rule.

## Verifier: problems in the code sketch

- The sketch should type-check apart from the omitted helpers (ProgramBuilder<ByRef<'_>>, catch_unwind+AssertUnwindSafe, the `??`, swf::UTF_8 re-exported at C:\dev\ref\ruffle\swf\src\string.rs:3, SwfStr::to_str_lossy(&'static Encoding) at string.rs:216, and Tag::DoInitAction{id, action_data}). The real defects are in behaviour, listed below.
- diagnostics_from() is unimplemented!(). It is called inside catch_unwind, so every ordinary syntax error in a frame script would surface as an 'internal compiler error'. In compile_classes it is also called inside catch_unwind. Implement it before anything else, or fall back to e.to_string_plain().
- ByRef::load uses std::fs::read_to_string, which fails with 'stream did not contain valid UTF-8' on legacy Flash-era .as files saved in an ANSI code page (Windows-1256 for Arabic projects) or as UTF-16LE with a BOM. Decode with encoding_rs instead: sniff the BOM (UTF-8/UTF-16LE/BE), else fall back to UTF-8 and then a configurable legacy code page. Do the same for #include files.
- The stub pass does change frame codegen. optimize_virtual_properties (C:\dev\ref\rascal\crates\rascal\src\internal\as2\hir\optimize_virtual_properties.rs:55-80) rewrites `typedVar.prop = v` into `typedVar.__set__prop(v)`, and reads into __get__, only when it knows the class definition. With empty stubs, frame scripts keep plain property access. That is still correct at runtime because classes register addProperty (C:\dev\ref\rascal\crates\rascal\src\internal\as2_codegen.rs:206), but the output differs from a full build. Document it, and do not treat byte-identity with a full Rascal build as a test oracle.
- Stubs are emitted as `class fqn {}` even for interface files. This is harmless for frame snippets, but defines_type() must accept `interface` as well, or interface files will be returned verbatim and loaded for real. A real interface has no main, so this is only a performance and diagnostics inconsistency.
- compile_button_handlers returns early with `return Err(vec![h.error(..)])` on an invalid event and discards the diagnostics already collected. Push to diags and `continue` instead, to match the aggregate-error design.
- The two-pass class design compiles only classes reached from snippets. Classes reached only through symbol linkage (xfl.xsd:3281-3337 linkageExportForAS/linkageClassName/linkageIdentifier) are never added, and Flash-style Object.registerClass(linkageId, a.b.C) is not emitted anywhere. compile_classes must also add_class every linkageClassName, and the publisher must emit registerClass in the symbol's DoInitAction.
- The roots vector mixes the FLA directory (for #include) with the classpath. That matches Flash's default '.' classpath, but every `new X` and every dotted prefix probes is_file on all roots (resolver.rs:806, 839-846). An include file whose name collides with a dotted reference, e.g. utils.as with `utils.x`, would then be loaded as a class and fail with 'Classes may only be defined...'. Keep a separate include root list, or read PublishSettings PackagePaths (PublishSettings.xsd:101) as the classpath.
- ClipActions: when KEY_PRESS is set, ClipAction.key_code must be Some(k), because write.rs:1891-1897 sizes the record from key_code. Each on(keyPress ...) on a MovieClip needs its own record. The sketch's comment says this, but the todo!() body must enforce it.
- env.snippets is never cleared, so long editor sessions accumulate every virtual snippet. Remove the entry after each compile, or key snippets by content hash.

## Verifier: missed points

- String escapes: Rascal decodes only \n \r \t \\ \" \' (C:\dev\ref\rascal\crates\rascal\src\internal\as2\parser.rs:40-55). Every other escape, including \uXXXX, \xNN, \b, \f and octal, is kept literally as a backslash plus the character, with no error. Arabic Flash content often writes text as \u0645 escapes, so these strings will be silently wrong. Either pre-decode in our front-end (with offsets preserved, which is hard) or patch upstream. This is a must-fix for the RTL/Arabic constraint.
- Any '@' outside strings or comments panics the lexer, not only malformed @PCode (lexer.rs:73 -> lex_pcode panic at lexer.rs:217). @PCode and inline pcode are Rascal extensions that are not Flash AS2. Our front-end should reject '@' with a proper diagnostic before calling Rascal.
- Flash 8's mx.* class library is not available: mx.utils.Delegate, mx.transitions.Tween/easing, and the v2 components. Rascal's GLOBAL_TYPES (internal\as2\global_types.rs) covers only player built-ins. Real Flash 8 content that imports mx.* fails with 'The class or interface ... could not be loaded'. Adobe's sources cannot be redistributed, so M1 needs either clean-room replacements (Delegate and Tween are small) or a clear diagnostic.
- The `intrinsic` keyword is unsupported (no match in rascal src; Keyword enum at tokens.rs:127-174). Classpaths that contain Flash-style intrinsic declaration files will fail to parse. Exclude them, or treat intrinsic classes as known types.
- XFL PublishSettings already answers several open questions. PackageExportFrame gives the frame for class DoInitActions, PackagePaths gives the AS2 classpath, ActionScriptVersion selects AS1 or AS2, Version is the SWF version, and UseNetwork maps to SwfOptions::with_network_sandbox (C:\dev\ref\xfl-schema\PublishSettings.xsd:97-101, 112). The publisher should read these values rather than hard-code them.
- Symbol linkage classes (xfl.xsd:3281, 3288, 3337) and Object.registerClass emission are entirely outside Rascal. Flash emits these per exported symbol in DoInitAction before first use. Without them, AS2 classes bound to MovieClip symbols will not attach.
- Very common Flash 8 idioms fail outright. `this.onEnterFrame = function(){ this._x++; }` fails with 'invalid expression' (probe out3.txt L4/L5/L8), as does `do {} while`, whose error label is empty. M1 test content should be screened early. A source-to-source rewrite of postfix member ++/-- into `(o.a += 1) - 1` form would be possible in the front-end but cannot keep byte offsets exact, so an upstream patch is preferable.
- SWF version alone does not set AS2 semantics. Rascal compiles AS1 and AS2 identically: there is no type checking of member existence, and annotations are used only for dependency loading and the getter/setter optimization. Flash 8 compile errors such as 'There is no property with the name' will not be reported, and the Output panel must not promise Flash-equivalent checking.

## Code sketch (researcher; check against corrections above)
```rust
// ---------- Cargo.toml (crate jazari-as2) ----------
// rascal = "=0.3.6"   # MIT OR Apache-2.0; same version ruffle_core@07d3e430 resolves for its build-dep
// swf    = "0.3.0"    # crates.io swf = the copy rascal links (types identical to Ruffle's in-tree swf)
// [profile.release] panic = "unwind"   # required: Rascal panics on some inputs (catch_unwind below)

use rascal::{CompileOptions, OptimizationOptions, ProgramBuilder, SourceProvider, SwfOptions};
use std::cell::{Cell, RefCell};
use std::collections::{BTreeSet, HashMap};
use std::ops::Range;
use std::panic::{catch_unwind, AssertUnwindSafe};
use std::path::PathBuf;

#[derive(Clone, Debug)]
pub struct Diagnostic {
    pub file: String,               // virtual "@frame/..", "@button/..#i", or class path "a/b/C.as"
    pub line: u32, pub column: u32, // 1-based; column counted in chars (Arabic-safe)
    pub byte_range: Option<Range<usize>>,
    pub message: String,
}

/// One compiler session per publish (or per editor check).
pub struct As2Env {
    snippets: RefCell<HashMap<String, String>>, // virtual path -> text (BOM already stripped)
    roots: Vec<PathBuf>,                        // [xfl/fla dir (for #include), classpath roots ...]
    stub_classes: Cell<bool>,                   // pass 1 = true
    requested: RefCell<BTreeSet<String>>,       // FQNs referenced by snippets
}

struct ByRef<'a>(&'a As2Env); // rascal has no blanket `impl SourceProvider for &P`

impl SourceProvider for ByRef<'_> {
    fn load(&self, path: &str) -> std::io::Result<String> {
        let env = self.0;
        if let Some(s) = env.snippets.borrow().get(path) { return Ok(s.clone()); }
        let file = env.roots.iter().map(|r| r.join(path)).find(|p| p.is_file())
            .ok_or_else(|| std::io::Error::new(std::io::ErrorKind::NotFound, "File not found"))?;
        let mut text = std::fs::read_to_string(file)?;
        if text.starts_with('\u{FEFF}') { text.drain(..3); }   // Rascal spans ignore the BOM
        if env.stub_classes.get() {
            if let Some(fqn) = path.strip_suffix(".as").map(|p| p.replace('/', ".")) {
                if defines_type(&text, &fqn) {                // "class fqn" / "interface fqn" present
                    env.requested.borrow_mut().insert(fqn.clone());
                    return Ok(format!("class {fqn} {{}}\n")); // no bodies, no static main
                }
            }
        }
        Ok(text) // #include files are returned verbatim
    }
    fn is_file(&self, path: &str) -> bool {
        self.0.snippets.borrow().contains_key(path) || self.0.roots.iter().any(|r| r.join(path).is_file())
    }
}

fn opts() -> CompileOptions {
    CompileOptions::default().with_swf_version(8).with_optimizations(OptimizationOptions::full())
}

/// Core: compile one loose AS2 snippet -> DoAction payload (ConstantPool .. End(0x00)), None if empty.
fn compile_snippet(env: &As2Env, vpath: &str, src: &str) -> Result<Option<Vec<u8>>, Vec<Diagnostic>> {
    let src = src.strip_prefix('\u{FEFF}').unwrap_or(src);
    env.snippets.borrow_mut().insert(vpath.to_owned(), src.to_owned());
    env.stub_classes.set(true);
    let res = catch_unwind(AssertUnwindSafe(|| -> Result<Vec<u8>, Vec<Diagnostic>> {
        let mut b = ProgramBuilder::new(ByRef(env)).with_compile_options(opts());
        b.add_script(vpath);
        let program = b.build().map_err(|e| diagnostics_from(env, &e))?;
        program.compile().to_swf(&SwfOptions::default()).map_err(|e| vec![internal(vpath, e.to_string())])
    }));
    env.stub_classes.set(false);
    let swf_bytes = match res {
        Ok(r) => r?,
        Err(p) => return Err(vec![internal(vpath, format!("internal compiler error: {}", panic_text(&p)))]),
    };
    let buf = swf::decompress_swf(&swf_bytes[..]).map_err(|e| vec![internal(vpath, e.to_string())])?;
    let parsed = swf::parse_swf(&buf).map_err(|e| vec![internal(vpath, e.to_string())])?;
    // DoInitAction tags in this SWF are the stubs -> ignore. Exactly 0 or 1 DoAction (no add_pcode used).
    Ok(parsed.tags.iter().find_map(|t| match t { swf::Tag::DoAction(d) => Some(d.to_vec()), _ => None }))
}

pub fn compile_frame_script(env: &As2Env, frame_id: &str, src: &str) -> Result<Option<Vec<u8>>, Vec<Diagnostic>> {
    // front-end: cut out "#initclip [n] ... #endinitclip" regions (blank them here, compile them
    // separately -> DoInitAction(sprite_id)); optionally rewrite gotoAndPlay("Scene", n) to absolute frames.
    compile_snippet(env, &format!("@frame/{frame_id}.as"), src)
}

/// Pass 2 (once per publish): real class bodies -> [("__Packages.a.B", bytes)] in Rascal's dependency order.
pub fn compile_classes(env: &As2Env) -> Result<Vec<(String, Vec<u8>)>, Vec<Diagnostic>> {
    let mut b = ProgramBuilder::new(ByRef(env)).with_compile_options(opts());
    for fqn in env.requested.borrow().iter() { b.add_class(fqn); }
    let bytes = catch_unwind(AssertUnwindSafe(|| -> Result<Vec<u8>, Vec<Diagnostic>> {
        let p = b.build().map_err(|e| diagnostics_from(env, &e))?; // fails if 2+ classes have static main
        p.compile().to_swf(&SwfOptions::default()).map_err(|e| vec![internal("<classes>", e.to_string())])
    })).map_err(|p| vec![internal("<classes>", panic_text(&p))])??;
    let buf = swf::decompress_swf(&bytes[..]).map_err(|e| vec![internal("<classes>", e.to_string())])?;
    let parsed = swf::parse_swf(&buf).map_err(|e| vec![internal("<classes>", e.to_string())])?;
    let mut names = HashMap::new();
    let mut out = vec![];
    for t in &parsed.tags {
        match t {
            swf::Tag::ExportAssets(a) => for x in a { names.insert(x.id, x.name.to_str_lossy(swf::UTF_8).into_owned()); },
            swf::Tag::DoInitAction { id, action_data } => out.push((names[id].clone(), action_data.to_vec())),
            _ => {} // the DoAction here only holds the auto "X.main(this)" call -> discard
        }
    }
    Ok(out) // publisher: re-number ids; per module emit DefineSprite{num_frames:0}+ExportAssets+DoInitAction
}

pub fn compile_button_handlers(env: &As2Env, inst: &str, src: &str)
    -> Result<Vec<(swf::ButtonActionCondition, Vec<u8>)>, Vec<Diagnostic>> {
    use swf::ButtonActionCondition as C;
    let (mut out, mut diags) = (vec![], vec![]);
    for (i, h) in split_handlers(src, HandlerKind::On)?.into_iter().enumerate() {
        let mut cond = C::empty();
        for ev in &h.events {
            cond |= match ev {
                Ev::Press => C::OVER_UP_TO_OVER_DOWN,   Ev::Release => C::OVER_DOWN_TO_OVER_UP,
                Ev::ReleaseOutside => C::OUT_DOWN_TO_IDLE, Ev::RollOver => C::IDLE_TO_OVER_UP,
                Ev::RollOut => C::OVER_UP_TO_IDLE,      Ev::DragOver => C::OUT_DOWN_TO_OVER_DOWN,
                Ev::DragOut => C::OVER_DOWN_TO_OUT_DOWN, Ev::KeyPress(k) => C::from_key_code(*k), // <Enter>=13 ...
                _ => return Err(vec![h.error("event not valid for a button")]),
            };
        }
        match compile_snippet(env, &format!("@button/{inst}#{i}.as"), &blank_outside(src, h.body.clone())) {
            Ok(Some(b)) => out.push((cond, b)), Ok(None) => {}, Err(d) => diags.extend(d),
        }
    }
    if diags.is_empty() { Ok(out) } else { Err(diags) } // -> swf::ButtonAction { conditions, action_data: &b }
}

pub fn compile_clip_events(env: &As2Env, inst: &str, src: &str)
    -> Result<Vec<(swf::ClipEventFlag, Option<u8>, Vec<u8>)>, Vec<Diagnostic>> {
    use swf::ClipEventFlag as F;
    // onClipEvent(load|enterFrame|unload|mouseMove|mouseDown|mouseUp|keyDown|keyUp|data|initialize|construct)
    //  -> F::LOAD|ENTER_FRAME|UNLOAD|MOUSE_MOVE|MOUSE_DOWN|MOUSE_UP|KEY_DOWN|KEY_UP|DATA|INITIALIZE|CONSTRUCT
    // on(press|release|releaseOutside|rollOver|rollOut|dragOver|dragOut|keyPress "<k>") on a MovieClip
    //  -> F::PRESS|RELEASE|RELEASE_OUTSIDE|ROLL_OVER|ROLL_OUT|DRAG_OVER|DRAG_OUT|KEY_PRESS (+ key_code Some(k))
    // Same loop as compile_button_handlers; the result feeds
    // swf::ClipActions { all_event_flags: OR of all, records: vec![swf::ClipAction { events, key_code, action_data }] }
    todo!()
}

/// Keep byte offsets AND lines: every byte outside `body` becomes ' ' (newlines kept).
fn blank_outside(src: &str, body: Range<usize>) -> String {
    let mut s = String::with_capacity(src.len());
    for (i, c) in src.char_indices() {
        if body.contains(&i) || c == '\n' || c == '\r' { s.push(c) } else { for _ in 0..c.len_utf8() { s.push(' ') } }
    }
    s
}

/// Stopgap until upstream exposes structured errors: parse `format!("{e:?}")`
///   ... parsing_errors: {Some(FileId(N)): [ParsingError { error: "<msg>", span: A..B }, ...]} ...
///   ... source_set: SourceSet { sources: [SourceFile { path: "<p0>" }, SourceFile { path: "<p1>" }] }
/// Map N to sources[N].path; compute line/col from that file's text and byte A; use "syntax error" when
/// <msg> is empty (the do-while case). If nothing parses, fall back to the first '╭▸ path:L:C' of
/// e.to_string_plain() and attach the whole plain text as the message.
fn diagnostics_from(env: &As2Env, e: &rascal::Error) -> Vec<Diagnostic> { /* ... */ unimplemented!() }

// helpers referenced above (not shown): defines_type, split_handlers (tokenizer aware of strings and comments,
// nested braces; returns events + header span + body byte range), HandlerKind, Ev, internal(), panic_text().
```
