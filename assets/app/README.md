# Application icon

The crab adapts `assets/icons/crab.svg`. Edit `sannygit.svg`, then regenerate the PNG, ICO and ICNS files:

```sh
python3 -m pip install Pillow
python3 scripts/build-icons.py
```

The 256 px PNG is embedded in the application. Windows builds embed the ICO in the executable; macOS bundles use ICNS; Linux packaging includes PNG, SVG and a desktop entry.
