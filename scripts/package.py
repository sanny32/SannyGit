#!/usr/bin/env python3
"""Build a native app bundle/staging directory, without installing it."""
import argparse
import json
from pathlib import Path
import plistlib
import shutil
import subprocess
import sys

ROOT = Path(__file__).resolve().parent.parent
parser = argparse.ArgumentParser(description=__doc__)
parser.add_argument("--debug", action="store_true", help="Package a debug build")
args = parser.parse_args()
metadata = json.loads(subprocess.check_output(
    ["cargo", "metadata", "--no-deps", "--format-version", "1"], cwd=ROOT
))
package = next(p for p in metadata["packages"] if p["name"] == "sannygit")
subprocess.run(["cargo", "build"] + ([] if args.debug else ["--release"]), cwd=ROOT, check=True)
target = Path(metadata["target_directory"])
binary = target / ("debug" if args.debug else "release") / ("sannygit.exe" if sys.platform == "win32" else "sannygit")
output = target / "package"
icons = ROOT / "assets/app"

if sys.platform == "darwin":
    bundle = output / "SannyGit.app"
    executable = bundle / "Contents/MacOS/sannygit"
    resources = bundle / "Contents/Resources"
    executable.parent.mkdir(parents=True, exist_ok=True)
    resources.mkdir(parents=True, exist_ok=True)
    shutil.copy2(binary, executable)
    shutil.copy2(icons / "sannygit.icns", resources / "sannygit.icns")
    with (bundle / "Contents/Info.plist").open("wb") as file:
        plistlib.dump({
            "CFBundleName": "SannyGit",
            "CFBundleDisplayName": "SannyGit",
            "CFBundleIdentifier": "org.sannygit.SannyGit",
            "CFBundleExecutable": "sannygit",
            "CFBundleIconFile": "sannygit.icns",
            "CFBundlePackageType": "APPL",
            "CFBundleShortVersionString": package["version"],
            "CFBundleVersion": package["version"],
            "NSHighResolutionCapable": True,
        }, file)
    print(bundle)
elif sys.platform == "win32":
    output.mkdir(parents=True, exist_ok=True)
    shutil.copy2(binary, output / "SannyGit.exe")
    print(output / "SannyGit.exe")
else:
    staging = output / "sannygit"
    for directory in ["bin", "share/applications", "share/icons/hicolor/256x256/apps", "share/icons/hicolor/scalable/apps"]:
        (staging / directory).mkdir(parents=True, exist_ok=True)
    shutil.copy2(binary, staging / "bin/sannygit")
    shutil.copy2(ROOT / "packaging/linux/SannyGit.desktop", staging / "share/applications/SannyGit.desktop")
    shutil.copy2(icons / "sannygit.png", staging / "share/icons/hicolor/256x256/apps/sannygit.png")
    shutil.copy2(icons / "sannygit.svg", staging / "share/icons/hicolor/scalable/apps/sannygit.svg")
    print(staging)
