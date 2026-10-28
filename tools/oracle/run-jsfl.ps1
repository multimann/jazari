<#
.SYNOPSIS
  Run a JSFL script in Macromedia Flash 8 and wait for it to finish.

.DESCRIPTION
  Starts Flash.exe with the given .jsfl file, then polls the log file that the
  script writes until a line reading exactly "DONE" appears.  The log is printed
  at the end.  Flash is killed if:
    * the timeout expires (exit code 2);
    * a modal dialog (window class #32770) is open while the log has not grown
      for -StallSec seconds, i.e. the script is blocked on a dialog (exit code 3);
  Exit code 4 means Flash exited without writing DONE.  When the blocking
  dialog is Flash's start-up "Flash 8 Language" dialog and the script never
  started, Flash is killed and relaunched after 10 s (up to 3 attempts).
  After DONE the script is expected to call fl.quit(false); if Flash is still
  running after -QuitGraceSec seconds it is killed and a warning is printed
  (exit code stays 0 because the script itself finished).  Flash 8 sometimes
  shows its "Macromedia Product Registration" nag at quit; that window is
  recognised and Flash is killed at once.  The nag also counts as a blocking
  dialog during the run.

  Only one Flash 8 instance may run at a time, across every agent on this
  machine.  Before starting Flash the script takes the lock directory
  <repo>\target\flash8.lock (see flash-lock.ps1: wait a minute and retry for
  up to -LockWaitMinutes; a lock older than 30 minutes with no Flash.exe
  running is stale and is taken over) and removes it once Flash has exited.
  A caller that already holds the lock (capture-trace.ps1, which keeps Flash
  open between two scripts) passes -LockHeld; then this script neither takes
  nor releases it.  -KeepFlashOpen requires -LockHeld.

  If Flash is already running the script is handed to that instance
  (Flash.exe forwards it) and that instance is the one being watched.  Use
  -KeepFlashOpen when the script deliberately leaves Flash running (for
  example a Test Movie that a second script reads).

  The JSFL script must write the same log the -Log parameter names, e.g.
    var LOG = "file:///C|/dev/jazari/target/oracle/logs/foo.log";
    FLfile.write(LOG, "");                      // truncate
    FLfile.write(LOG, msg + "\n", "append");    // log
    ... FLfile.write(LOG, "DONE\n", "append"); fl.quit(false);
  (build-all.ps1 generates such scripts with the paths filled in.)

.EXAMPLE
  powershell -NoProfile -ExecutionPolicy Bypass -File C:\dev\jazari\tools\oracle\run-jsfl.ps1 `
      -Script C:\dev\jazari\target\oracle\gen\probe.jsfl -Log C:\dev\jazari\target\oracle\logs\probe.log
#>
param(
    [Parameter(Mandatory = $true)] [string] $Script,
    [Parameter(Mandatory = $true)] [string] $Log,
    [int] $TimeoutSec = 120,
    [int] $StallSec = 20,
    [int] $QuitGraceSec = 20,
    [switch] $KeepFlashOpen,
    [switch] $LockHeld,
    [string] $LockDir = '',
    [int] $LockWaitMinutes = 30,
    [string] $FlashExe = 'C:\Program Files (x86)\Macromedia\Flash 8\Flash.exe'
)

$ErrorActionPreference = 'Stop'
. (Join-Path $PSScriptRoot 'flash-lock.ps1')
if (-not $LockDir) { $LockDir = Get-FlashLockDir }
if ($KeepFlashOpen -and -not $LockHeld) {
    throw '-KeepFlashOpen leaves Flash running: take the lock first (Enter-FlashLock in flash-lock.ps1) and pass -LockHeld.'
}

if (-not ('JsflRunner.Win' -as [type])) {
    Add-Type -TypeDefinition @'
using System;
using System.Collections.Generic;
using System.Runtime.InteropServices;
using System.Text;
namespace JsflRunner {
  public static class Win {
    delegate bool EnumProc(IntPtr h, IntPtr l);
    [DllImport("user32.dll")] static extern bool EnumWindows(EnumProc cb, IntPtr l);
    [DllImport("user32.dll")] static extern bool EnumChildWindows(IntPtr p, EnumProc cb, IntPtr l);
    [DllImport("user32.dll")] static extern uint GetWindowThreadProcessId(IntPtr h, out uint pid);
    [DllImport("user32.dll")] static extern bool IsWindowVisible(IntPtr h);
    [DllImport("user32.dll", CharSet = CharSet.Unicode)] static extern int GetWindowText(IntPtr h, StringBuilder s, int n);
    [DllImport("user32.dll", CharSet = CharSet.Unicode)] static extern int GetClassName(IntPtr h, StringBuilder s, int n);

    static string Text(IntPtr h) { var s = new StringBuilder(1024); GetWindowText(h, s, s.Capacity); return s.ToString(); }
    static string Cls(IntPtr h) { var s = new StringBuilder(256); GetClassName(h, s, s.Capacity); return s.ToString(); }

    // Visible top-level windows of a process: "class|title|child texts".
    public static List<string> Windows(int pid, bool dialogsOnly) {
      var res = new List<string>();
      EnumWindows(delegate (IntPtr h, IntPtr l) {
        uint p; GetWindowThreadProcessId(h, out p);
        if (p != (uint)pid || !IsWindowVisible(h)) return true;
        string c = Cls(h);
        // dialogs: standard dialog class, or the registration nag (class is a GUID)
        if (dialogsOnly && c != "#32770" && Text(h).IndexOf("Registration") < 0) return true;
        var kids = new List<string>();
        EnumChildWindows(h, delegate (IntPtr k, IntPtr l2) {
          if (IsWindowVisible(k)) { string t = Text(k).Trim(); if (t.Length > 0) kids.Add(t.Replace("\r", " ").Replace("\n", " ")); }
          return true;
        }, IntPtr.Zero);
        res.Add(c + " | \"" + Text(h) + "\" | " + string.Join(" / ", kids.ToArray()));
        return true;
      }, IntPtr.Zero);
      return res;
    }
  }
}
'@
}

function Show-Windows([int]$procId, [bool]$dialogsOnly) {
    $w = [JsflRunner.Win]::Windows($procId, $dialogsOnly)
    if ($w.Count -eq 0) { Write-Host '  (no visible windows)' }
    foreach ($x in $w) { Write-Host ('  ' + $x) }
}

function Show-Log {
    Write-Host '----- log -----'
    if (Test-Path -LiteralPath $Log) { Get-Content -LiteralPath $Log | ForEach-Object { Write-Host $_ } }
    else { Write-Host '(log file was never written)' }
    Write-Host '---------------'
}

function Get-Flash {
    Get-Process -Name Flash -ErrorAction SilentlyContinue |
        Where-Object { try { $_.Path -eq $FlashExe } catch { $false } } |
        Select-Object -First 1
}

function Invoke-Jsfl {
    $Script = (Resolve-Path -LiteralPath $Script).Path
    if (-not (Test-Path -LiteralPath $FlashExe)) { throw "Flash not found: $FlashExe" }
    if (Test-Path -LiteralPath $Log) { Remove-Item -LiteralPath $Log -Force }
    # Flash 8 must start in its own folder: with a non-ASCII working directory it
    # cannot find its language folder and blocks on a "Flash 8 Language" dialog.
    $flashDir = Split-Path -Parent $FlashExe

    $proc = Get-Flash
    if ($proc) {
        Write-Host "Flash already running (pid $($proc.Id)); handing the script to it."
        Start-Process -FilePath $FlashExe -ArgumentList ('"' + $Script + '"') -WorkingDirectory $flashDir | Out-Null
    } else {
        $proc = Start-Process -FilePath $FlashExe -ArgumentList ('"' + $Script + '"') -WorkingDirectory $flashDir -PassThru
        Write-Host "Started Flash (pid $($proc.Id)) with $Script"
    }

    $start = Get-Date
    $lastLen = -1
    $lastChange = Get-Date
    $result = $null

    while ($true) {
        Start-Sleep -Milliseconds 500
        $now = Get-Date
        $len = 0
        if (Test-Path -LiteralPath $Log) {
            $len = (Get-Item -LiteralPath $Log).Length
            $content = [System.IO.File]::ReadAllText($Log)
            if ($content -match '(?m)^DONE\s*$') { $result = 'done'; break }
        }
        if ($len -ne $lastLen) { $lastLen = $len; $lastChange = $now }

        $proc.Refresh()
        if ($proc.HasExited) { $result = 'exited'; break }

        $stalled = ($now - $lastChange).TotalSeconds
        if ($stalled -ge $StallSec) {
            $dlg = [JsflRunner.Win]::Windows($proc.Id, $true)
            if ($dlg.Count -gt 0) { $result = 'dialog'; break }
        }
        if (($now - $start).TotalSeconds -ge $TimeoutSec) { $result = 'timeout'; break }
    }

    $elapsed = [int]((Get-Date) - $start).TotalSeconds
    $code = 0
    switch ($result) {
        'done' {
            Write-Host "Script finished (DONE after ${elapsed}s)."
            if (-not $KeepFlashOpen) {
                # fl.quit() sometimes raises the "Macromedia Product Registration"
                # nag; the script has finished by then, so kill Flash at once.
                $quitStart = Get-Date
                while (-not $proc.WaitForExit(500)) {
                    $nag = @([JsflRunner.Win]::Windows($proc.Id, $true) | Where-Object { $_ -like '*Registration*' })
                    if ($nag.Count -gt 0) {
                        Write-Host 'NOTE: the registration nag appeared at quit; killing Flash (harmless, the script had finished).'
                        Stop-Process -Id $proc.Id -Force
                        break
                    }
                    if (((Get-Date) - $quitStart).TotalSeconds -ge $QuitGraceSec) {
                        Write-Host "WARNING: Flash did not quit within ${QuitGraceSec}s after DONE. Visible windows:"
                        Show-Windows $proc.Id $false
                        Write-Host 'Killing Flash.'
                        Stop-Process -Id $proc.Id -Force
                        break
                    }
                }
            }
        }
        'exited' {
            Write-Host "ERROR: Flash exited after ${elapsed}s without writing DONE (exit code $($proc.ExitCode))."
            $code = 4
        }
        'dialog' {
            Write-Host "ERROR: Flash is blocked on a dialog (log unchanged for ${StallSec}s). Dialog windows:"
            Show-Windows $proc.Id $true
            $lang = @([JsflRunner.Win]::Windows($proc.Id, $true) | Where-Object { $_ -like '*Flash 8 Language*' }).Count -gt 0
            Write-Host 'Killing Flash.'
            Stop-Process -Id $proc.Id -Force
            $proc.WaitForExit(15000) | Out-Null
            $code = 3
            # The "Flash 8 Language" dialog sometimes appears at start-up even from
            # Flash's own folder (MANIFEST J18); before the script ran it is retryable.
            if ($lang -and -not (Test-Path -LiteralPath $Log)) { $code = 6 }
        }
        'timeout' {
            Write-Host "ERROR: timeout after ${TimeoutSec}s without DONE. Visible Flash windows:"
            Show-Windows $proc.Id $false
            Write-Host 'Killing Flash.'
            Stop-Process -Id $proc.Id -Force
            $code = 2
        }
    }
    Show-Log
    return $code
}

if (-not $LockHeld) { Enter-FlashLock -LockDir $LockDir -WaitMinutes $LockWaitMinutes }
$exitCode = 5
try {
    for ($attempt = 1; $attempt -le 3; $attempt++) {
        $exitCode = Invoke-Jsfl | Select-Object -Last 1
        if ($exitCode -ne 6) { break }
        Write-Host "Flash stopped at its start-up language dialog before the script ran; retrying in 10 s (attempt $attempt of 3)."
        Start-Sleep -Seconds 10
    }
    if ($exitCode -eq 6) { $exitCode = 3 }
} finally {
    if (-not $LockHeld) {
        # Release only once no Flash is left running (every path above waits
        # for Flash to quit or kills it; this is the safety net).
        foreach ($p in Get-FlashProcess) {
            if (-not $p.WaitForExit(15000)) { Write-Host "Killing leftover Flash (pid $($p.Id))."; Stop-Process -Id $p.Id -Force; $p.WaitForExit(15000) | Out-Null }
        }
        Start-Sleep -Seconds 3   # let Flash finish with its preference files before the next launch
        Exit-FlashLock -LockDir $LockDir
    }
}
exit $exitCode
