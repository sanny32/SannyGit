#!/usr/bin/env python3
"""Build and package SannyGit for the current OS, with optional tests and installation."""
import argparse
import json
import os
from pathlib import Path
import plistlib
import shutil
import subprocess
import sys

ROOT = Path(__file__).resolve().parent.parent


def default_install_prefix(platform):
    if platform == "darwin":
        return Path.home() / "Applications"
    if platform == "win32":
        local = os.environ.get("LOCALAPPDATA")
        if not local:
            raise RuntimeError("LOCALAPPDATA is not set; specify --install=PREFIX.")
        return Path(local) / "Programs/SannyGit"
    return Path.home() / ".local"


def package_native(binary, output, platform, version):
    """Copy only application artifacts; never clear an existing output directory."""
    icons = ROOT / "assets/app"
    if platform == "darwin":
        bundle = output / "SannyGit.app"
        executable = bundle / "Contents/MacOS/sannygit"
        resources = bundle / "Contents/Resources"
        executable.parent.mkdir(parents=True, exist_ok=True)
        resources.mkdir(parents=True, exist_ok=True)
        shutil.copy2(binary, executable)
        shutil.copy2(icons / "sannygit.icns", resources / "sannygit.icns")
        shutil.copy2(ROOT / "LICENSE", resources / "LICENSE")
        with (bundle / "Contents/Info.plist").open("wb") as file:
            plistlib.dump({
                "CFBundleName": "SannyGit",
                "CFBundleDisplayName": "SannyGit",
                "CFBundleIdentifier": "org.sannygit.SannyGit",
                "CFBundleExecutable": "sannygit",
                "CFBundleIconFile": "sannygit.icns",
                "CFBundlePackageType": "APPL",
                "CFBundleShortVersionString": version,
                "CFBundleVersion": version,
                "NSHighResolutionCapable": True,
            }, file)
        return bundle
    if platform == "win32":
        output.mkdir(parents=True, exist_ok=True)
        destination = output / "SannyGit.exe"
        shutil.copy2(binary, destination)
        shutil.copy2(ROOT / "LICENSE", output / "LICENSE")
        return destination
    staging = output / "sannygit"
    for directory in ["bin", "share/applications", "share/icons/hicolor/256x256/apps", "share/icons/hicolor/scalable/apps"]:
        (staging / directory).mkdir(parents=True, exist_ok=True)
    licenses = staging / "share/licenses/sannygit"
    licenses.mkdir(parents=True, exist_ok=True)
    shutil.copy2(ROOT / "LICENSE", licenses / "LICENSE")
    shutil.copy2(binary, staging / "bin/sannygit")
    shutil.copy2(ROOT / "packaging/linux/SannyGit.desktop", staging / "share/applications/SannyGit.desktop")
    shutil.copy2(icons / "sannygit.png", staging / "share/icons/hicolor/256x256/apps/sannygit.png")
    shutil.copy2(icons / "sannygit.svg", staging / "share/icons/hicolor/scalable/apps/sannygit.svg")
    return staging


def install_native(artifact, prefix, platform):
    prefix.mkdir(parents=True, exist_ok=True)
    if platform == "darwin":
        destination = prefix / artifact.name
        shutil.copytree(artifact, destination, dirs_exist_ok=True)
        return destination
    if platform == "win32":
        destination = prefix / artifact.name
        shutil.copy2(artifact, destination)
        shutil.copy2(artifact.parent / "LICENSE", prefix / "LICENSE")
        return destination
    # Merge only our files, preserving permissions on existing prefix directories.
    for relative in [
        "bin/sannygit", "share/applications/SannyGit.desktop",
        "share/icons/hicolor/256x256/apps/sannygit.png",
        "share/icons/hicolor/scalable/apps/sannygit.svg",
        "share/licenses/sannygit/LICENSE",
    ]:
        destination = prefix / relative
        destination.parent.mkdir(parents=True, exist_ok=True)
        shutil.copy2(artifact / relative, destination)
    return prefix / "bin/sannygit"


def parse_args(argv=None):
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--debug", action="store_true", help="Package a debug build instead of Release")
    parser.add_argument("--tests", action="store_true", help="Run the Rust test suite before packaging")
    parser.add_argument("--install", nargs="?", const="", metavar="PREFIX",
                        help="Install after packaging (default: ~/Applications, ~/.local or LOCALAPPDATA/Programs/SannyGit)")
    args = parser.parse_args(argv)
    # A bare --install selects the default; an explicit empty prefix is a typo.
    if argv is not None and "--install=" in argv:
        parser.error("--install prefix must not be empty")
    return args


def main(argv=None):
    args = parse_args(sys.argv[1:] if argv is None else argv)
    if sys.platform not in ("darwin", "win32", "linux"):
        raise RuntimeError(f"Unsupported platform: {sys.platform}")
    prefix = None
    if args.install is not None:
        prefix = Path(args.install).expanduser().resolve() if args.install else default_install_prefix(sys.platform)
    metadata = json.loads(subprocess.check_output(
        ["cargo", "metadata", "--no-deps", "--format-version", "1"], cwd=ROOT, text=True
    ))
    package = next(p for p in metadata["packages"] if p["name"] == "sannygit")
    profile = [] if args.debug else ["--release"]
    print(f"Building SannyGit {package['version']} ({'Debug' if args.debug else 'Release'})...", flush=True)
    # Cargo reports the executable path, including custom target directories/config.
    result = subprocess.run(
        ["cargo", "build", *profile, "--message-format=json-render-diagnostics"],
        cwd=ROOT, check=True, stdout=subprocess.PIPE, text=True,
    )
    binary = None
    for line in result.stdout.splitlines():
        message = json.loads(line)
        if (message.get("reason") == "compiler-artifact"
                and message.get("package_id") == package["id"]
                and message.get("target", {}).get("name") == "sannygit"
                and message.get("executable")):
            binary = Path(message["executable"])
    if binary is None:
        raise RuntimeError("Cargo did not report the SannyGit executable.")
    if args.tests:
        subprocess.run(["cargo", "test", *profile], cwd=ROOT, check=True)
    artifact = package_native(binary, Path(metadata["target_directory"]) / "package", sys.platform, package["version"])
    print(f"Package: {artifact}", flush=True)
    if prefix is not None:
        installed = install_native(artifact, prefix, sys.platform)
        print(f"Installed: {installed}", flush=True)
    return 0


if __name__ == "__main__":
    try:
        sys.exit(main())
    except (OSError, RuntimeError, ValueError, subprocess.CalledProcessError) as error:
        print(f"Error: {error}", file=sys.stderr)
        sys.exit(1)
