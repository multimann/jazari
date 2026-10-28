<#
.SYNOPSIS
  Record the trace() output of a fixture as Flash 8's own Test Movie player prints it.

.DESCRIPTION
  1. Copies <FixturesRoot>\<Name>\<Name>.fla to <WorkDir>\<Name>\ (Test Movie
     writes a SWF next to the FLA; the copy keeps the fixture folder untouched).
  2. Takes the Flash lock (flash-lock.ps1) for the whole capture.
  3. Phase 1 JSFL: opens the copy, clears the Output panel, runs
     document.testMovie() and leaves Flash running.
  4. Waits -PlaySec seconds while the movie plays in Flash's test window.
  5. Phase 2 JSFL: saves the Output panel to <WorkDir>\<Name>\trace.txt and
     quits Flash. Then the lock is released.
  6. Unless -NoInstall, copies trace.txt to <FixturesRoot>\<Name>\flash8-trace.txt
     (byte for byte: UTF-8 with BOM and CRLF, as Flash saves it).
  Both phases go through run-jsfl.ps1 -LockHeld.  Mouse input never happens,
  so button handlers do not fire.  The Test Movie SWF stays in
  <WorkDir>\<Name>\<Name>.swf; verify-fixtures.py compares it with flash8.swf.

.EXAMPLE
  powershell -NoProfile -ExecutionPolicy Bypass -File C:\dev\jazari\tools\oracle\capture-trace.ps1 -Name as2 -PlaySec 4
#>
param(
    [Parameter(Mandatory = $true)] [string] $Name,
    [int] $PlaySec = 4,
    [string] $FixturesRoot = '',
    [string] $WorkDir = '',
    [switch] $NoInstall
)
$ErrorActionPreference = 'Stop'
. (Join-Path $PSScriptRoot 'flash-lock.ps1')
$repo = Get-JazariRoot
if (-not $FixturesRoot) { $FixturesRoot = Join-Path $repo 'tests\fixtures' }
if (-not $WorkDir) { $WorkDir = Join-Path $repo 'target\oracle\tracecap' }

$root = Join-Path $WorkDir $Name
$src = Join-Path $FixturesRoot "$Name\$Name.fla"
if (-not (Test-Path -LiteralPath $src)) { throw "missing $src" }
New-Item -ItemType Directory -Force $root | Out-Null
Copy-Item -LiteralPath $src -Destination (Join-Path $root "$Name.fla") -Force
Remove-Item -LiteralPath (Join-Path $root 'trace.txt'), (Join-Path $root 'trace-sync.txt'), (Join-Path $root "$Name.swf") -ErrorAction SilentlyContinue

$uriRoot = (ConvertTo-FileUri $root) + '/'
$log1 = Join-Path $root 'phase1.log'
$log2 = Join-Path $root 'phase2.log'
$p1 = @"
var LOG = "${uriRoot}phase1.log";
function log(m) { FLfile.write(LOG, m + "\n", "append"); }
FLfile.write(LOG, "");
try {
  var doc = fl.openDocument("${uriRoot}$Name.fla");
  log("opened " + doc.name);
  fl.outputPanel.clear();
  doc.testMovie();
  log("testMovie returned");
  fl.outputPanel.save("${uriRoot}trace-sync.txt");
} catch (e) { log("ERROR: " + e); }
log("DONE");
"@
$p2 = @"
var LOG = "${uriRoot}phase2.log";
function log(m) { FLfile.write(LOG, m + "\n", "append"); }
FLfile.write(LOG, "");
try {
  fl.outputPanel.save("${uriRoot}trace.txt");
  log("saved output panel");
  var docs = fl.documents;
  for (var i = docs.length - 1; i >= 0; i--) fl.closeDocument(docs[i], false);
} catch (e) { log("ERROR: " + e); }
log("DONE");
fl.quit(false);
"@
Set-Content -LiteralPath (Join-Path $root 'phase1.jsfl') -Value $p1 -Encoding ASCII
Set-Content -LiteralPath (Join-Path $root 'phase2.jsfl') -Value $p2 -Encoding ASCII

$runner = Join-Path $PSScriptRoot 'run-jsfl.ps1'
Enter-FlashLock
try {
    & powershell -NoProfile -ExecutionPolicy Bypass -File $runner -Script (Join-Path $root 'phase1.jsfl') -Log $log1 -TimeoutSec 90 -KeepFlashOpen -LockHeld
    if ($LASTEXITCODE -ne 0) { throw "phase 1 failed ($LASTEXITCODE)" }
    Start-Sleep -Seconds $PlaySec
    & powershell -NoProfile -ExecutionPolicy Bypass -File $runner -Script (Join-Path $root 'phase2.jsfl') -Log $log2 -TimeoutSec 60 -LockHeld
    if ($LASTEXITCODE -ne 0) { throw "phase 2 failed ($LASTEXITCODE)" }
} finally {
    foreach ($p in Get-FlashProcess) {
        if (-not $p.WaitForExit(15000)) { Write-Host "Killing leftover Flash (pid $($p.Id))."; Stop-Process -Id $p.Id -Force; $p.WaitForExit(15000) | Out-Null }
    }
    Start-Sleep -Seconds 3   # let Flash finish with its preference files before the next launch
    Exit-FlashLock
}

$trace = Join-Path $root 'trace.txt'
Write-Host '----- trace (synchronous, right after testMovie returned) -----'
if (Test-Path -LiteralPath (Join-Path $root 'trace-sync.txt')) { Get-Content -LiteralPath (Join-Path $root 'trace-sync.txt') }
Write-Host "----- trace after $PlaySec s -----"
if (Test-Path -LiteralPath $trace) {
    Get-Content -LiteralPath $trace
    if (-not $NoInstall) {
        $dest = Join-Path $FixturesRoot "$Name\flash8-trace.txt"
        Copy-Item -LiteralPath $trace -Destination $dest -Force
        Write-Host "installed $dest"
    }
} else { Write-Host '(no trace.txt)' }
