# Flash 8 oracle tooling

These scripts drive the real Macromedia Flash 8 IDE (WIN 8,0,0,478) through
JSFL to answer "what does Flash 8 do?". They build the fixtures in
`tests/fixtures/` (see `tests/fixtures/MANIFEST.md`) and run one-off probes.
They were first written in `C:\dev\oracle` and copied here with repository-relative
paths; the original folder is left as it was.

Everything here is Windows-only and CPU-only. Flash 8 publishes without any GPU
work; `preview_check.exe` (used to look at the results) renders on WARP when
`JAZARI_FORCE_CPU=1` is set, which `tools/env.ps1` and `tools/env.sh` do.

## Requirements

| what | where |
|---|---|
| Flash 8 IDE | `C:\Program Files (x86)\Macromedia\Flash 8\Flash.exe` (`run-jsfl.ps1 -FlashExe` overrides) |
| Windows PowerShell 5.1 | `powershell.exe` (the scripts start nested `powershell -File` processes) |
| Python 3 | `python` on PATH; `make-images.py` also needs Pillow |
| Java 21 | `java` on PATH (`. tools\env.ps1` adds the JDK), else the Adoptium path in `build-all.ps1` |
| JPEXS FLA Convert 3.0.0 | `C:\dev\tools\jpexs-fla-convert-3.0.0\fla_convert.jar` (`build-all.ps1 -Jar` overrides) |

The repository path must be ASCII: Flash 8 JSFL cannot open non-ASCII paths
(`ConvertTo-FileUri` refuses them).

## One Flash at a time

Only one Flash 8 instance may run on the machine, across every agent and tool.
The lock is the empty directory `target\flash8.lock`:

- creating it takes the lock (creating a directory is atomic and fails when it exists);
- a holder that finds it already there waits a minute and retries, for up to 30 minutes;
- a lock older than 30 minutes while no `Flash.exe` runs is stale: remove it and take it;
- the holder removes it as soon as Flash has exited.

`flash-lock.ps1` implements this (`Enter-FlashLock`, `Exit-FlashLock`).
`run-jsfl.ps1` takes and releases the lock around every run unless it is given
`-LockHeld`; `capture-trace.ps1` holds it across both of its Flash runs. Keep
the directory empty so that anyone can remove a stale lock with `rmdir`. If you
start Flash by any other means, take the lock first.

## Files

| file | what it does |
|---|---|
| `flash-lock.ps1` | Dot-source it. The Flash lock, `Get-JazariRoot`, `Get-FlashProcess` and `ConvertTo-FileUri` (`C:\a b` becomes `file:///C|/a%20b`). |
| `run-jsfl.ps1` | Runs one JSFL script in Flash 8 and waits for the line `DONE` in the log the script writes. Parameters: `-Script -Log [-TimeoutSec 120] [-StallSec 20] [-QuitGraceSec 20] [-KeepFlashOpen -LockHeld] [-LockDir] [-LockWaitMinutes 30] [-FlashExe]`. Starts Flash in its own folder (MANIFEST J1). Exit codes: 0 done, 2 timeout, 3 blocked on a dialog (its title and texts are printed), 4 Flash exited without `DONE`. The start-up "Flash 8 Language" dialog (J18) is retried up to 3 times. The "Product Registration" nag at quit (J17) is killed. |
| `build-all.ps1` | `-Names a,b` or `-All`, plus `[-SkipTrace] [-OutRoot] [-SrcDir] [-WorkDir] [-PlaySec] [-TimeoutSec 180] [-Java] [-Jar]`. For each fixture: writes `target\oracle\gen\NAME.jsfl` (a path prelude plus the builder), runs it, converts the FLA with JPEXS into `xfl\`, then captures the Test Movie trace. Rebuilding a committed fixture replaces its truth files: do it only on purpose, or use `-OutRoot` to build elsewhere. |
| `capture-trace.ps1` | `-Name X [-PlaySec 4] [-FixturesRoot] [-WorkDir] [-NoInstall]`. Copies `X.fla` to `target\oracle\tracecap\X\`, runs Flash 8's **Test Movie** there, waits, saves the Output panel to `trace.txt` and installs it as `tests\fixtures\X\flash8-trace.txt` (UTF-8 with BOM, CRLF, exactly as Flash saves it). Nothing clicks, so button handlers never fire. |
| `fixtures\NAME.jsfl` | The builder of fixture NAME. Each step is wrapped in try/catch and logged; the log ends with `FAILED STEPS: n` and `DONE`. Builders save `NAME.fla`, close it, **reopen it and publish `flash8.swf` from the reopened file** (MANIFEST J15). |
| `make-smoke.jsfl` | The builder of the `smoke` fixture (`build-all.ps1 -Names smoke`). It publishes straight from the edited document, as the committed fixture was made. |
| `swfdump.py` | `python swfdump.py file.swf [--header] [--shapes-full]`. SWF 8 dumper: header, shapes (styles, LINESTYLE2, focal gradients, per-style edge usage; `--shapes-full` adds per-style edge bounds in pixels), PlaceObject2/3 (matrix, cxform, ratio, name, clipDepth, filters, blend mode, cacheAsBitmap, clip actions), sprites, buttons, text, fonts, bitmaps, frame labels and an AVM1 disassembler (GotoFrame2 shows its scene bias). |
| `xfl-summary.py` | `python xfl-summary.py tests/fixtures/NAME/xfl`. Readable summary of an XFL folder: layers (type, `parentLayerIndex`), frames, elements, group matrices, styles, filters, colour transforms and per-style edge bounds in the shape's own space. |
| `verify-fixtures.py` | `python verify-fixtures.py [--root DIR] [--tracecap DIR] [NAME ...]`. Prints the MANIFEST verification table (header bytes, stage, fps, frame count, XFL presence), the tag census, JPEXS notes other than the routine ones, the trace, and whether the Test Movie SWF still in `target\oracle\tracecap` equals `flash8.swf` after decompression. Exit code 1 if a fixture fails. |
| `make-images.py` | Regenerates `tests\fixtures\_src\alpha-disc.png` and `bars.jpg` (bitmaps fixture). |
| `make-gradient-swf.py` | Regenerates `tests\fixtures\_src\grad-src.swf` (gradients fixture, MANIFEST J3). |

Generated files go to `target\oracle\` (never committed): `gen\` (the scripts
Flash actually runs), `logs\NAME.log` (builder logs), `tracecap\NAME\` (Test
Movie copies, the Test Movie SWF and the trace).

## How builders get their paths

A builder never names an absolute path. `build-all.ps1` prepends a prelude that
defines these variables as `file:///` URIs:

| variable | value |
|---|---|
| `JZ_OUT` | `tests/fixtures/NAME/` (or `-OutRoot\NAME\`) |
| `JZ_SRC` | `tests/fixtures/_src/` |
| `JZ_LOG` | `target/oracle/logs/NAME.log` |
| `JZ_WORK` | `target/oracle/logs/` (for side outputs such as an exported publish profile) |

So a builder starts with `var OUT = JZ_OUT; var LOG = JZ_LOG;`. Keep builders
ASCII: Flash 8 reads JSFL in the system code page, so write non-ASCII text with
`\uXXXX` escapes.

## Adding a fixture

1. Read `tests/fixtures/MANIFEST.md`, above all the JSFL workarounds J1 to J19.
   The ones that bite every builder: `getCustomStroke()` returns a stub (use the
   stroke template, J7); `addItem` often places nothing on the first try (retry
   and check the layer, J10); selecting things moves the current layer (`useLayer`
   before every placement, J11); new layers are added above the current one (J12);
   publish from the reopened FLA (J15).
2. Copy the helpers of an existing builder (`symbols.jsfl` for symbols and filters,
   `tweens.jsfl` for tweens, `masks.jsfl` for layer types, `scenes.jsfl` for scenes)
   into `fixtures\NAME.jsfl`. Log every step, read things back into the log, trace
   something from frame 1, and keep the movie small (60 frames at most).
3. Build it: `powershell -NoProfile -ExecutionPolicy Bypass -File tools\oracle\build-all.ps1 -Names NAME`.
   The summary line shows failed JSFL steps, the JPEXS result and the trace line
   count. Read `target\oracle\logs\NAME.log`; rerun after a timeout or a dialog (J17, J18).
4. Check the result:
   - `python -I tools\oracle\swfdump.py tests\fixtures\NAME\flash8.swf --shapes-full`
   - `python -I tools\oracle\xfl-summary.py tests\fixtures\NAME\xfl`
   - render frames and look at them:
     `target\tools\preview_check.exe target\oracle\png\NAME tests\fixtures\NAME\flash8.swf:1,10`
     (use one output folder per SWF: the PNG names are `flash8-fNNN.png`).
5. Append a section to `tests/fixtures/MANIFEST.md` in the style of the others
   (covers, stage and frames, library, timeline table, expected trace, JSFL
   problems, JPEXS problems, SWF facts) and add the fixture to the verification
   table: `python -I tools\oracle\verify-fixtures.py NAME` prints the row.
6. Never edit a fixture's `flash8.swf` or `flash8-trace.txt` by hand: they are
   what Flash 8 did.

## Probes

For a one-off question, write a small JSFL script that logs to a file and ends
with `DONE` and `fl.quit(false)`, keep it outside the repository (or under
`target\oracle\`), and run it with `run-jsfl.ps1 -Script probe.jsfl -Log probe.log`.
The launcher takes the Flash lock for you. Parse what it publishes with
`swfdump.py`.
