#!/usr/bin/env bash
set -euo pipefail

PROJECT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"

usage() {
    cat <<'HELP'
Usage: ./build.sh [--tests] [--install[=PREFIX]]

Build and package SannyGit in Release mode on Linux or macOS.
Missing build dependencies are installed automatically (sudo may be required).

  --tests             Run the Rust test suite before packaging
  --install[=PREFIX]  Install after packaging, optionally into PREFIX
  -h, --help          Show this help without installing or building anything

Default install location: ~/Applications on macOS, ~/.local on Linux.
Packages are written to Cargo's target directory under package/.
HELP
}

for argument in "$@"; do
    case "$argument" in
        -h|--help) usage; exit 0 ;;
        --tests|--install) ;;
        --install=*)
            if [ -z "${argument#--install=}" ]; then
                echo 'Error: --install prefix must not be empty.' >&2
                exit 1
            fi
            ;;
        *) echo "Error: unknown argument: $argument" >&2; usage >&2; exit 1 ;;
    esac
done

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

exec "$PYTHON_BIN" "$PROJECT_DIR/scripts/package.py" "$@"
