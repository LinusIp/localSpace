# localSpace brand files

Place this folder at `web/public/brand/` in the repository.

| File | Use |
|---|---|
| `localspace-mark.svg` | **Source for every icon.** Square, 256×256 viewBox, transparent, ink `#1C1E20`. Generate `.ico`, `.icns`, the PNG set and the favicon from this. |
| `localspace-mark-light.svg` | The same mark in `#FAFAFA`, for dark backgrounds. |
| `localspace-mark-1024.png`, `-512.png` | Raster fallbacks if a tool cannot read SVG. Transparent. |
| `localspace-lockup.png` | Mark + wordmark, transparent, 552×162. Login page, About dialog, documents. |
| `localspace-lockup-light.png` | The same lockup in near-white, for dark backgrounds. |

## Notes

The mark is a traced vector rebuild of the original raster logo: pointy-top rounded hexagon, circumradius 92, stroke 23.3, corner radius 16.8; three nodes of radius 15.5 joined by 9.1-wide connectors. It matches the original at 96% pixel overlap, the remainder being antialiasing.

The **icon is the mark alone, never the lockup** — the wordmark is unreadable below about 64 px.

The **lockup is still raster**, cropped from the original artwork at its native resolution. It is sharp enough for the login page and About at any size up to about 260 px wide, which covers every current use. If it ever needs to be printed or shown larger, the wordmark must be re-set from the original type — identify the typeface first rather than tracing it.

Ink `#1C1E20`. On dark, `#FAFAFA`. Do not recolour the mark, place it on a busy background, or stretch it — the hexagon is 1.106 times taller than it is wide and must stay so.
