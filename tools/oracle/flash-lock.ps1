<#
.SYNOPSIS
  One-Flash-at-a-time lock shared by every tool (and agent) that starts Flash 8.

.DESCRIPTION
  Dot-source this file:   . "$PSScriptRoot\flash-lock.ps1"
  It defines:
    Get-JazariRoot            the repository root (two levels above this file)
    Get-FlashLockDir          <repo>\target\flash8.lock
    Enter-FlashLock [-LockDir] [-WaitMinutes 30] [-StaleMinutes 30]
    Exit-FlashLock  [-LockDir]
    Get-FlashProcess          running Flash.exe processes (any path)
    ConvertTo-FileUri <path>  "C:\a b\c" -> "file:///C|/a%20b/c" (the form Flash 8 JSFL accepts)

  The lock is the empty directory target\flash8.lock. Creating a directory is
  atomic and fails when it exists, so whoever creates it owns Flash:
    * if it exists, wait one minute and retry, for up to -WaitMinutes;
    * if it is older than -StaleMinutes and no Flash.exe runs, it is stale:
      remove it and take it;
    * remove it as soon as Flash has exited.
  The directory is kept empty on purpose, so that any agent can remove a stale
  lock with a plain rmdir.
#>

function Get-JazariRoot {
    (Resolve-Path -LiteralPath (Join-Path $PSScriptRoot '..\..')).Path
}

function Get-FlashLockDir {
    Join-Path (Get-JazariRoot) 'target\flash8.lock'
}

function Get-FlashProcess {
    @(Get-Process -Name Flash -ErrorAction SilentlyContinue)
}

function ConvertTo-FileUri([string] $Path) {
    $full = [System.IO.Path]::GetFullPath($Path)
    if ($full -notmatch '^[A-Za-z]:\\') { throw "not a drive path: $full" }
    if ($full -match '[^\x20-\x7E]') { throw "Flash 8 JSFL cannot use non-ASCII paths: $full" }
    $rest = $full.Substring(3).Replace('\', '/').Replace('%', '%25').Replace(' ', '%20')
    'file:///' + $full.Substring(0, 1) + '|/' + $rest
}

if (-not ('JazariOracle.Dir' -as [type])) {
    Add-Type -Namespace JazariOracle -Name Dir -MemberDefinition @'
[System.Runtime.InteropServices.DllImport("kernel32.dll", CharSet = System.Runtime.InteropServices.CharSet.Unicode, SetLastError = true)]
public static extern bool CreateDirectoryW(string path, System.IntPtr securityAttributes);
'@
}

function Enter-FlashLock {
    param(
        [string] $LockDir = (Get-FlashLockDir),
        [int] $WaitMinutes = 30,
        [int] $StaleMinutes = 30
    )
    $parent = Split-Path -Parent $LockDir
    if (-not (Test-Path -LiteralPath $parent)) { New-Item -ItemType Directory -Force $parent | Out-Null }
    $deadline = (Get-Date).AddMinutes($WaitMinutes)
    while ($true) {
        # CreateDirectoryW fails with ERROR_ALREADY_EXISTS when the lock is held.
        if ([JazariOracle.Dir]::CreateDirectoryW($LockDir, [IntPtr]::Zero)) {
            # Defeat NTFS creation-time tunnelling: a directory re-created within
            # 15 s of being removed can inherit the old creation time.
            try { (Get-Item -LiteralPath $LockDir).CreationTime = Get-Date } catch { }
            Write-Host "Flash lock taken: $LockDir"
            return
        }
        $err = [Runtime.InteropServices.Marshal]::GetLastWin32Error()
        $item = Get-Item -LiteralPath $LockDir -ErrorAction SilentlyContinue
        if (-not $item) {
            # Released between the two calls (retry at once), or not creatable at all.
            if ((Get-Date) -ge $deadline) { throw "cannot create the Flash lock $LockDir (Win32 error $err)" }
            Start-Sleep -Seconds 1
            continue
        }
        $born = $item.CreationTime
        if ($item.LastWriteTime -gt $born) { $born = $item.LastWriteTime }
        $age = ((Get-Date) - $born).TotalMinutes
        if ($age -ge $StaleMinutes -and (Get-FlashProcess).Count -eq 0) {
            Write-Host ("Flash lock is stale ({0:N0} min old, no Flash.exe running); removing it." -f $age)
            Remove-Item -LiteralPath $LockDir -Recurse -Force -ErrorAction SilentlyContinue
            continue
        }
        if ((Get-Date) -ge $deadline) {
            throw ("Timed out after $WaitMinutes min waiting for the Flash lock $LockDir ({0:N0} min old)." -f $age)
        }
        Write-Host ("Flash lock is held ({0:N0} min old); waiting a minute." -f $age)
        Start-Sleep -Seconds 60
    }
}

function Exit-FlashLock {
    param([string] $LockDir = (Get-FlashLockDir))
    if (Test-Path -LiteralPath $LockDir) {
        Remove-Item -LiteralPath $LockDir -Recurse -Force
        Write-Host "Flash lock released: $LockDir"
    }
}
