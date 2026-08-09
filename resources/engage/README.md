# Engage UI textures

Extracted with UnityPy from a personal Fire Emblem Engage dump
(`Data/StreamingAssets/aa/Switch/fe_assets_ui/...`), for use in the installer's
Engage-themed UI. Most are white-on-transparent — view the contact sheets
(`cursor_sheet.png`, `hub_icons_sheet.png`) or composite on a dark background.

- `menucursor/` — menu pointers (» chevrons) and the glowing selection frames
  (`Cursor*` per menu type, `*Bg*` are solid tint silhouettes)
- `minimap/` — Somniel facility pictograms (bed, forge, market, arena, …)
- `icon_system/` — item/food/animal/weapon-type/ring glyphs from the system set
- `sactx-*.png` — the raw sprite atlases the singles were unpacked from

Nothing here ships automatically: only files referenced via `asset!()` (copied
into `assets/`) end up in builds. Currently in use: the » pointer and the
RefineShop/Market/Bed sidebar icons. White icons are tinted at runtime via CSS
`mask` + `background-color: currentColor` (see `.nav_icon.game` in main.css and
the injected style block in main.rs).
