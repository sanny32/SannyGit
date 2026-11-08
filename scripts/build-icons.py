#!/usr/bin/env python3
"""Regenerate platform icons from SVG. Requires Pillow and the Rust toolchain."""
from pathlib import Path
import subprocess
from PIL import Image

ROOT = Path(__file__).resolve().parent.parent
subprocess.run(["cargo", "run", "--example", "render_icon"], cwd=ROOT, check=True)
with Image.open(ROOT / "assets/app/sannygit-1024.png") as image:
    image.resize((256, 256), Image.Resampling.LANCZOS).save(ROOT / "assets/app/sannygit.png")
    image.save(ROOT / "assets/app/sannygit.ico", sizes=[(s, s) for s in (16, 20, 24, 32, 48, 64, 128, 256)])
    image.save(ROOT / "assets/app/sannygit.icns")
