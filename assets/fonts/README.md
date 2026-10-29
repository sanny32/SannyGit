# Source Sans 3

Source Sans 3 version 3.052, by Adobe.

Source: https://github.com/adobe-fonts/source-sans/tree/3.052R

The unmodified Regular and Semibold TTF files are embedded in the application.
They are distributed under the SIL Open Font License 1.1; see `OFL.txt`.

On Windows, the interface prefers Segoe UI Regular and Semibold loaded from the
system Fonts directory at startup. These system fonts are not redistributed.
The embedded Source Sans 3 faces remain fallbacks and are used on other platforms
or when the Windows fonts cannot be read. The diff keeps its monospace font.

Text uses egui's font hinting with fractional glyph binning disabled in both
themes. Interface fonts also enable horizontal grid fitting for sharper small
text; the compact layout and font sizes are independent of these settings.
