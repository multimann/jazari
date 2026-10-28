# Build environment for Al-Jazari (Git Bash on Windows, or bash on Linux).
#
# Source it, do not run it:   source tools/env.sh
#
# Agent shells do not inherit an up-to-date PATH, so this sets everything the
# build needs explicitly. See docs/ARCHITECTURE.md, section 3.

case "$(uname -s)" in
    MINGW* | MSYS* | CYGWIN*)
        # ASCII-only paths: Ruffle's build script passes them to java.
        export CARGO_HOME="C:\dev\rust\cargo"
        export RUSTUP_HOME="C:\dev\rust\rustup"
        _jazari_cargo_bin="/c/dev/rust/cargo/bin"
        # Java is needed at build time only (ruffle_core builds its AVM2
        # playerglobal with asc.jar). Ruffle reads PATH, never JAVA_HOME.
        _jazari_java_bin="/c/Program Files/Eclipse Adoptium/jdk-21.0.12.101-hotspot/bin"
        ;;
    *)
        # Linux: rustup's defaults, and java from the distribution
        # (default-jre-headless) already on PATH.
        _jazari_cargo_bin="${CARGO_HOME:-$HOME/.cargo}/bin"
        _jazari_java_bin=""
        ;;
esac

for _jazari_dir in "$_jazari_java_bin" "$_jazari_cargo_bin"; do
    if [ -n "$_jazari_dir" ]; then
        case ":$PATH:" in
            *":$_jazari_dir:"*) ;;
            *) PATH="$_jazari_dir:$PATH" ;;
        esac
    fi
done
export PATH
unset _jazari_dir _jazari_cargo_bin _jazari_java_bin

# Never put rendering load on the GPU of a development machine: Al-Jazari and
# its tests use the software adapter (WARP, lavapipe or llvmpipe) when this is
# set.
export JAZARI_FORCE_CPU=1
