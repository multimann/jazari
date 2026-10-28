# Build environment for Al-Jazari (Windows PowerShell 5.1 or PowerShell 7).
#
# Dot-source it, do not run it:   . C:\dev\jazari\tools\env.ps1
#
# Agent shells do not inherit an up-to-date PATH, so this sets everything the
# build needs explicitly. See docs/ARCHITECTURE.md, section 3.

# ASCII-only paths: Ruffle's build script passes them to java.
$env:CARGO_HOME = 'C:\dev\rust\cargo'
$env:RUSTUP_HOME = 'C:\dev\rust\rustup'

# Java is needed at build time only (ruffle_core builds its AVM2 playerglobal
# with asc.jar). Ruffle reads PATH, never JAVA_HOME.
$jazariPathDirs = @(
    'C:\dev\rust\cargo\bin',
    'C:\Program Files\Eclipse Adoptium\jdk-21.0.12.101-hotspot\bin'
)
$jazariCurrent = @($env:PATH -split ';' | Where-Object { $_ -ne '' })
foreach ($dir in $jazariPathDirs) {
    if ($jazariCurrent -notcontains $dir) {
        $jazariCurrent = @($dir) + $jazariCurrent
    }
}
$env:PATH = $jazariCurrent -join ';'
Remove-Variable jazariPathDirs, jazariCurrent, dir -ErrorAction SilentlyContinue

# Never put rendering load on the GPU of a development machine: Al-Jazari and
# its tests use the software adapter (WARP) when this is set.
$env:JAZARI_FORCE_CPU = '1'
