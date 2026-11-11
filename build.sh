#!/usr/bin/env bash
set -euo pipefail

PROJECT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"

usage() {
    cat <<'HELP'
Usage: ./build.sh [--tests] [--install[=PREFIX]] [--sign=IDENTITY]

Build SannyGit in Release mode on Linux or macOS.
Missing build dependencies are installed automatically (sudo may be required).

  --tests             Run the Rust test suite after building
  --install[=PREFIX]  Install after building, optionally into PREFIX
  --sign=IDENTITY     Sign macOS builds with an existing code-signing certificate
  -h, --help          Show this help without installing or building anything

Default install location: ~/Applications on macOS, ~/.local on Linux.
Distribution packages are produced by CI.
HELP
}

run_tests=0
run_install=0
install_prefix=""
signing_identity=""
for argument in "$@"; do
    case "$argument" in
        -h|--help) usage; exit 0 ;;
        --tests) run_tests=1 ;;
        --sign=*)
            signing_identity="${argument#--sign=}"
            if [ -z "$signing_identity" ] || [ "$signing_identity" = - ]; then
                echo 'Error: --sign requires a certificate name or SHA-1 identity, not an ad-hoc signature.' >&2
                exit 1
            fi
            ;;
        --install) run_install=1 ;;
        --install=*)
            run_install=1
            install_prefix="${argument#--install=}"
            if [ -z "${argument#--install=}" ]; then
                echo 'Error: --install prefix must not be empty.' >&2
                exit 1
            fi
            ;;
        *) echo "Error: unknown argument: $argument" >&2; usage >&2; exit 1 ;;
    esac
done

if [ -n "$signing_identity" ] && [ "$(uname -s)" != Darwin ]; then
    echo 'Error: --sign is supported only on macOS.' >&2
    exit 1
fi

run_as_root() {
    if [ "$EUID" -eq 0 ]; then
        "$@"
    elif command -v sudo >/dev/null 2>&1; then
        sudo "$@"
    else
        echo 'Error: sudo is required to install system build dependencies.' >&2
        exit 1
    fi
}

ensure_linux_dependencies() {
    local missing=0 tool
    for tool in cc pkg-config python3 git curl; do
        command -v "$tool" >/dev/null 2>&1 || missing=1
    done
    if ! pkg-config --exists dbus-1 wayland-client xkbcommon x11 xrandr xi xcursor gl xcb xcb-randr 2>/dev/null; then
        missing=1
    fi
    [ "$missing" -eq 1 ] || return 0
    local ID='' ID_LIKE=''
    if [ -r /etc/os-release ]; then
        . /etc/os-release
    fi
    echo 'Installing Linux build dependencies...'
    case " $ID $ID_LIKE " in
        *' debian '*|*' ubuntu '*)
            run_as_root apt-get update
            run_as_root apt-get install -y build-essential pkg-config python3 git curl \
                libdbus-1-dev libwayland-dev libxkbcommon-dev libx11-dev \
                libxrandr-dev libxi-dev libxcursor-dev libgl1-mesa-dev libxcb1-dev libxcb-randr0-dev
            ;;
        *' fedora '*|*' rhel '*)
            run_as_root dnf install -y gcc gcc-c++ pkgconf-pkg-config python3 git curl \
                dbus-devel wayland-devel libxkbcommon-devel libX11-devel \
                libXrandr-devel libXi-devel libXcursor-devel mesa-libGL-devel libxcb-devel
            ;;
        *' suse '*|*' opensuse-tumbleweed '*|*' opensuse-leap '*)
            run_as_root zypper --non-interactive install gcc gcc-c++ pkgconf-pkg-config python3 git curl \
                dbus-1-devel wayland-devel libxkbcommon-devel libX11-devel \
                libXrandr-devel libXi-devel libXcursor-devel Mesa-libGL-devel libxcb-devel
            ;;
        *' arch '*)
            run_as_root pacman -S --needed --noconfirm base-devel pkgconf python git curl \
                dbus wayland libxkbcommon libx11 libxrandr libxi libxcursor mesa libxcb
            ;;
        *)
            echo "Error: automatic dependency installation is unavailable for ${ID:-this distribution}." >&2
            echo 'Install a C/C++ toolchain, Python 3.9+, Git, curl, pkg-config and DBus, Wayland, X11/XCB, xkbcommon and OpenGL development libraries.' >&2
            exit 1
            ;;
    esac
}

ensure_macos_dependencies() {
    if ! xcode-select -p >/dev/null 2>&1; then
        xcode-select --install
        echo 'Complete the Xcode Command Line Tools installation, then run this script again.' >&2
        exit 1
    fi
    local tool formula
    for tool in python3 git curl; do
        command -v "$tool" >/dev/null 2>&1 && continue
        if ! command -v brew >/dev/null 2>&1; then
            echo "Error: $tool is missing. Install Homebrew from https://brew.sh and run this script again." >&2
            exit 1
        fi
        formula="$tool"
        [ "$tool" != python3 ] || formula=python
        brew install "$formula"
    done
}

case "$(uname -s)" in
    Linux) ensure_linux_dependencies ;;
    Darwin) ensure_macos_dependencies ;;
    *) echo 'Error: use build.ps1 on Windows; build.sh supports Linux and macOS.' >&2; exit 1 ;;
esac

PYTHON_BIN=python3
if ! "$PYTHON_BIN" -c 'import sys; sys.exit(0 if sys.version_info >= (3, 9) else 1)'; then
    if [ "$(uname -s)" = Darwin ] && command -v brew >/dev/null 2>&1; then
        brew install python
        PYTHON_BIN="$(brew --prefix python)/bin/python3"
    else
        echo 'Error: Python 3.9+ is required; update Python and run this script again.' >&2
        exit 1
    fi
fi

# Rustup's normal per-user location may not yet be on PATH in a fresh shell.
if ! command -v cargo >/dev/null 2>&1 || ! command -v rustc >/dev/null 2>&1; then
    if [ -d "${CARGO_HOME:-$HOME/.cargo}/bin" ]; then
        export PATH="${CARGO_HOME:-$HOME/.cargo}/bin:$PATH"
    fi
fi
if ! command -v cargo >/dev/null 2>&1 || ! command -v rustc >/dev/null 2>&1; then
    echo 'Installing Rust with rustup...'
    installer="$(mktemp)"
    trap 'rm -f "$installer"' EXIT
    curl --proto '=https' --tlsv1.2 -fsSL https://sh.rustup.rs -o "$installer"
    sh "$installer" -y --profile minimal --default-toolchain stable
    export PATH="${CARGO_HOME:-$HOME/.cargo}/bin:$PATH"
fi

minimum_rust="$(sed -nE 's/^rust-version = "([^"]+)"/\1/p' "$PROJECT_DIR/Cargo.toml")"
if [ -z "$minimum_rust" ]; then
    echo 'Error: cannot read rust-version from Cargo.toml.' >&2
    exit 1
fi
if ! "$PYTHON_BIN" - "$minimum_rust" <<'PY'
import re
import subprocess
import sys
minimum = tuple(map(int, sys.argv[1].split('.')))
version = subprocess.check_output(['rustc', '--version'], text=True)
match = re.search(r'rustc (\d+)\.(\d+)', version)
sys.exit(0 if match and tuple(map(int, match.groups())) >= minimum[:2] else 1)
PY
then
    if ! command -v rustup >/dev/null 2>&1; then
        echo "Error: Rust $minimum_rust+ is required. Install or update Rust through rustup." >&2
        exit 1
    fi
    echo "Updating Rust stable (minimum $minimum_rust)..."
    rustup update stable --no-self-update
    export RUSTUP_TOOLCHAIN=stable
fi

binary="$(cd "$PROJECT_DIR" && cargo build --release --message-format=json-render-diagnostics | "$PYTHON_BIN" -c '
import json, sys
executables = [message["executable"] for line in sys.stdin
               if (message := json.loads(line)).get("reason") == "compiler-artifact"
               and message.get("target", {}).get("name") == "sannygit"
               and message.get("executable")]
if not executables:
    sys.exit("Cargo did not report the SannyGit executable.")
print(executables[-1])
')"
if [ "$run_tests" -eq 1 ]; then
    (cd "$PROJECT_DIR" && cargo test --release)
fi
artifact="$binary"
if [ "$(uname -s)" = Darwin ]; then
    bundle="$(dirname "$binary")/SannyGit.app"
    version="$(sed -nE 's/^version = "([^"]+)"/\1/p' "$PROJECT_DIR/Cargo.toml")"
    mkdir -p "$bundle/Contents/MacOS" "$bundle/Contents/Resources"
    install -m 755 "$binary" "$bundle/Contents/MacOS/sannygit"
    install -m 644 "$PROJECT_DIR/assets/app/sannygit.icns" "$bundle/Contents/Resources/sannygit.icns"
    install -m 644 "$PROJECT_DIR/LICENSE" "$bundle/Contents/Resources/LICENSE"
    install -m 644 "$PROJECT_DIR/assets/fonts/OFL.txt" "$bundle/Contents/Resources/SourceSans3-OFL.txt"
    base_version="${version%%[-+]*}"
    sed "s|@VERSION@|$base_version|g" "$PROJECT_DIR/.github/macos/Info.plist.in" > "$bundle/Contents/Info.plist"
    codesign --force --sign "${signing_identity:--}" "$bundle"
    codesign --verify --strict "$bundle"
    artifact="$bundle"
fi
echo "Built: $artifact"
if [ "$run_install" -eq 0 ]; then
    exit 0
fi

case "$(uname -s)" in
    Darwin)
        install_prefix="${install_prefix:-$HOME/Applications}"
        mkdir -p "$install_prefix"
        installed_bundle="$install_prefix/SannyGit.app"
        if [ "$bundle" != "$installed_bundle" ]; then
            ditto "$bundle" "$installed_bundle"
        fi
        codesign --verify --strict "$installed_bundle"
        echo "Installed: $installed_bundle"
        ;;
    Linux)
        install_prefix="${install_prefix:-$HOME/.local}"
        mkdir -p "$install_prefix/bin" "$install_prefix/share/applications" \
            "$install_prefix/share/icons/hicolor/256x256/apps" \
            "$install_prefix/share/icons/hicolor/scalable/apps" \
            "$install_prefix/share/licenses/sannygit"
        install -m 755 "$binary" "$install_prefix/bin/sannygit"
        install -m 644 "$PROJECT_DIR/.github/linux/usr/share/applications/SannyGit.desktop" "$install_prefix/share/applications/SannyGit.desktop"
        install -m 644 "$PROJECT_DIR/assets/app/sannygit.png" "$install_prefix/share/icons/hicolor/256x256/apps/sannygit.png"
        install -m 644 "$PROJECT_DIR/assets/app/sannygit.svg" "$install_prefix/share/icons/hicolor/scalable/apps/sannygit.svg"
        install -m 644 "$PROJECT_DIR/LICENSE" "$install_prefix/share/licenses/sannygit/LICENSE"
        install -m 644 "$PROJECT_DIR/assets/fonts/OFL.txt" "$install_prefix/share/licenses/sannygit/SourceSans3-OFL.txt"
        echo "Installed: $install_prefix/bin/sannygit"
        ;;
esac
