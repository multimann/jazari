# Patches to vendored Rascal

This directory holds the exact crates.io package `rascal 0.3.6`
(<https://crates.io/crates/rascal/0.3.6>, `.crate` SHA-256
`ba78442f12cbfa28edad7c0f1e02c370cb2d985fb7de98962e2c6893e6bd4d85`, built from
<https://github.com/Dinnerbone/Rascal> commit
`3f1a80c85dcc657cf4a67b72754b9c7869a6d9c5`, path `crates/rascal`). The root
`Cargo.toml` routes every use of the crate, including Ruffle's
`build_playerglobal` build dependency, here through `[patch.crates-io]`.

Rascal is dual-licensed MIT OR Apache-2.0. The published package does not
include its licence files, so `LICENSE-MIT` and `LICENSE-APACHE` were copied
from the upstream repository at the same commit. Our changes are released under
the same terms as Rascal.

## Policy

- Keep every patch **small and upstreamable**: one concern per patch, in
  Rascal's own style, with tests, so it can be sent upstream as a pull request.
- Patches must be **additive**. The same code also compiles Ruffle's AVM1
  playerglobal at build time, so existing behaviour must not change.
- **List each patch below**: what it changes, which files it touches, its
  tests, and the upstream PR once there is one.
- Do not reformat or reorganise upstream files. When upstream releases a
  version that contains our patches, replace this directory with the new
  package and drop the entries it covers.

## How the patches were checked

- **Tests:** `src/patch_tests.rs` (27 tests, run with the patches applied).
  The vendored package has no `samples/`, so Rascal's own suite was run in a
  copy of the upstream repository at the same commit with `src/` replaced by
  this one: `INSTA_UPDATE=no cargo test -p rascal -- --test-threads=1`.
  Every upstream snapshot (lexer tokens, parser output, compiled programs,
  PCode) is unchanged. The only failures, 26 `fail_samples@*.as`, fail in
  exactly the same way without the patches: they are a Windows artefact
  (case-insensitive file system: `getProperty(...)` finds the sample file
  `getproperty.as` as a class).
- **Ruffle:** `core/src/avm1/globals/globals.as` at Ruffle `07d3e430`, built
  as `build_playerglobal` does it (SWF 8), gives a byte-identical
  `playerglobal_avm1.swf` (11710 bytes) with and without the patches. It
  contains no `do`, `@`, backslash escapes or postfix `++` on members.
- **Flash 8:** the expected bytecode, escapes and messages come from probe
  documents compiled by the Flash 8 IDE. The probes and Flash's output are in
  `crates/jazari-as2/tests/flash8/`, and `jazari-as2` compares its output
  with them in `src/oracle_tests.rs`.

## Patches

### 1. Postfix `++`/`--` on members and indexes

`this._x++`, `o.a--` and `arr[i]++` failed with "invalid expression", because
the parser allowed postfix operators only after a constant or identifier.
`can_postfix` now also accepts a field access (which covers `a.b` and
`a[b]`), a call and a parenthesised expression; calls and parentheses that do
not name a variable are then rejected by patch 4's assignability check with
Flash's own message.

- Files: `src/internal/as2/parser/expression.rs` (`can_postfix`).
- Code generation is unchanged: it already handled fields, as prefix `++o.a`
  shows. Its output is the same, action for action, as Flash 8's for
  `o.a++`, `var x = o.a--`, `arr[i]++` and `this._x++`. Flash 8 also
  evaluates the object and key expressions again for the store (so
  `arr[i++]++` increments `i` three times in both).
- Tests: `postfix_*` in `src/patch_tests.rs`.

### 2. `do { } while ( )`

There was no `do` keyword; `do ... while` failed with an empty error message.

- Files: `lexer/tokens.rs` and `lexer.rs` (keyword `do`), `ast.rs` and
  `hir.rs` (`StatementKind::DoWhile`), `parser/statement.rs`
  (`do_while_loop`; a braceless body may end with `;`), `resolver.rs`,
  `hir/visitor.rs`, `as2_codegen/statement.rs` (`gen_do_while_loop`: body,
  condition, `If` back to the start; `continue` re-tests the condition,
  `break` leaves).
- The bytecode matches Flash 8's exactly, jump offsets included.
- Rascal accepts a statement right after `while (...)` without `;` or a line
  break, where Flash 8 reports "Syntax error."; that leniency is kept.
- Tests: `do_while_*` in `src/patch_tests.rs`.

### 3. String escapes

Only `\n \r \t \\ \" \'` were decoded; everything else was kept as a
backslash plus the character, so `"\u0645"` (common in Arabic Flash content)
compiled to the six characters `\u0645`. `unescape` in
`src/internal/as2/parser.rs` now decodes what the Flash 8 compiler decodes,
with its observed behaviour for malformed escapes:

| source | result | Flash 8 |
|---|---|---|
| `\b`, `\f` | U+0008, U+000C | same |
| `\uXXXX` (any hex case) | that UTF-16 unit | same |
| `\uD83D\uDE00` | one character, U+1F600 | each surrogate as its own UTF-8 sequence (CESU-8) |
| lone surrogate | U+FFFD | CESU-8 bytes |
| `\xNN` | U+00NN | the byte decoded through the system ANSI code page (`\xC3` gave U+0623 on a Windows-1256 machine) |
| `\0` to `\377` (1-3 octal digits, `\400` is `\40` + `0`) | U+0000 to U+00FF | same |
| `\u12`, `\x4` (too few digits) | `u12`, `x4` | same |
| `\q`, `\8` (unknown escape) | `\` (the character is dropped) | same |
| NUL in the result | the string ends there | same |

- This changes what upstream produced for unknown escapes (`\q` gave `\q`);
  the new results are Flash 8's.
- Files: `src/internal/as2/parser.rs`.
- Tests: `escapes_*` in `src/patch_tests.rs`.

### 4. Panics turned into errors

| input | before | now |
|---|---|---|
| `@` not followed by `PCode {` | lexer panic | an `Unknown` token, so the parser reports a syntax error there |
| invalid inline `@PCode { ... }` | code generator panic | resolver error "Invalid inline PCode." |
| `"abc" = 5`, `5++`, `--5`, `f() = 1`, `getTimer()++`, `null = 1`, `(1 + 2) -= 3` | code generator panic (`Cannot set a direct value`) | resolver error "Left side of assignment operator must be variable or property." (Flash 8's message) |
| `typed.prop++` or `typed.prop -= 1` where `prop` is a getter/setter of a known class | the virtual-property pass made the target a `__get__` call, then the code generator panicked | the target stays a member access (the getter and setter still run through `addProperty`) |
| `delete _x`, `_x()`, `new _x()` (special properties) | `unimplemented!` panic | `delete` pushes `false`; call and `new` act on the property's value, as `(_x)()` would |

- Files: `lexer.rs`, `resolver.rs` (`is_assignment`, `check_assignable`,
  the inline PCode check), `hir/optimize_virtual_properties.rs`,
  `as2_codegen/access.rs`.
- Panics that remain: `Label already exists` (an inline PCode label that
  collides with a generated `locNNNN` label) and the defensive
  `unreachable!`/`expect` calls that cannot be reached from source text.
  `jazari-as2` rejects `@` before Rascal sees it and runs every build inside
  `catch_unwind` on a thread with a large stack.
- Tests: `at_sign_*`, `invalid_inline_pcode_*`, `assignment_*`,
  `special_property_*` in `src/patch_tests.rs`.

### 5. Structured diagnostics (`Error::diagnostics`)

`Error` could only be rendered as text, and only the first error of each file
appeared in the header. The new public `Error::diagnostics()` returns every
error as a `Diagnostic { path, message, span }` (byte range in that file), and
`rascal::Error` and `rascal::Diagnostic` are now exported. This lets
`jazari-as2` show positions in its own Output panel instead of parsing
`Debug` output. Additive only.

- Files: `src/error.rs`, `src/lib.rs`.
- Tests: `diagnostics_*` in `src/patch_tests.rs`.

### 6. Scenes

A standalone compiler knows no scenes, so `nextScene()`/`prevScene()` went to
frame 0 and `gotoAndPlay("Scene", n)` ignored the scene. Flash resolves them
when compiling. The new public `Scenes` (every scene's name and first frame,
plus the current scene) is passed with `CompileOptions::with_scenes`; it
applies to scripts only, not classes. With it:

| source | compiled (Flash 8, `tests/fixtures/scenes`) |
|---|---|
| `nextScene()` / `prevScene()` | `GotoFrame` to the first frame of the next / previous scene, no `Play` |
| `gotoAndPlay(n)` | `GotoFrame` first frame of the current scene + n − 1, `Play` |
| `gotoAndPlay("Scene", n)` | `GotoFrame` first frame of that scene + n − 1, `Play` (an unknown scene counts as the current one) |
| `gotoAndPlay("Scene", "label")`, `gotoAndPlay("label")` | `GotoLabel`, `Play` (the scene is dropped) |
| `gotoAndPlay(expr)` | `GotoFrame2` with the scene bias (not checked against Flash) |

`gotoAndStop` is the same without `Play`. `nextScene()` on the last scene and
`prevScene()` on the first keep the old behaviour (frame 0).

- Additive: a new HIR variant `GotoSceneFrame` is produced only when scenes
  are given, and `CompileOptions.scenes` is skipped when serialising, so the
  upstream snapshots do not change. Ruffle's `playerglobal_avm1.swf` is
  byte-identical (SHA-256 `e88de60f…`).
- Files: `src/program.rs` (`Scenes`, `with_scenes`), `src/lib.rs`,
  `src/internal/as2/hir.rs`, `hir/visitor.rs`, `resolver.rs`,
  `resolver/special_functions.rs` (`scene_goto`, `scene_step`),
  `as2_codegen/statement.rs`.
- Tests: `scenes_*` in `src/patch_tests.rs` (27 patch tests in all, run in a
  standalone copy of this directory).

## Known upstream problems not patched

- `substring(s, i, n)` and `mbsubstring(...)` resolve `i` from the first
  argument (`resolver/special_functions.rs`, `fn_substring` and
  `fn_mbsubstring` use `args[0]` twice), so the start index is the string.
- `typed.prop += v` on a getter/setter property compiles to
  `typed.__set__prop(v)`, dropping the addition
  (`optimize_virtual_properties.rs`).
- A syntax error at the very end of a file has no span (`ParsingError` uses
  the default span when the error offset is past the last token).
- `a\n++b` is read as `a++; b` (postfix across a line break).
- `if (true)` keeps its test where Flash 8 drops it (same behaviour).
