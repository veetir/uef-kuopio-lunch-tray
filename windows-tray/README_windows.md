# Compass Lunch for Windows

Small system tray app for viewing UEF Kuopio campus lunch menus on Windows.

## What this app does

- Shows today's menu in a popup from the Windows tray
- Lets you switch restaurant and language
- Supports manual refresh and auto-refresh

## Download and start

1. Open the project's **GitHub Releases** page:  
   `https://github.com/veetir/uef-kuopio-lunch-tray/releases`
2. Download and extract the latest `compass-lunch-windows-x64` ZIP.
3. Place `compass-lunch.exe` in any folder (for example `Downloads` or `Apps`).
4. Double-click the `.exe` to start the app.

After launch, the app runs in the system tray (notification area), usually near the clock.

## If Windows shows a warning

Because the app is currently unsigned, Windows SmartScreen may show a warning on first run.

1. Click **More info**
2. Click **Run anyway**

Only do this for binaries downloaded from the official GitHub Releases page of this project.

## How to use it

- Left-click tray icon: open/close the menu popup
- Mouse wheel on tray icon: switch restaurant
- Drag-select text on a dish row: toggle it as a favorite highlight
- Right-click tray icon: open settings, refresh, and quit
- Right-click tray icon > Theme > Layout > Lunch items: choose Classic, Standard, or Compact menu rows

## First-time setup

Open settings from the tray menu and set:

- Language (`fi` or `en`)
- Enabled restaurants
- Favorite/default restaurant
- Run at startup (optional)

## Where your data is stored

- Settings: `%LOCALAPPDATA%\compass-lunch\settings.json`
- Cache: `%LOCALAPPDATA%\compass-lunch\cache\`
- Favorites: `%LOCALAPPDATA%\compass-lunch\favorites.json`
- Custom themes: `%LOCALAPPDATA%\compass-lunch\themes.json`

The `settings.json` key for the menu row layout is `lunch_item_display_mode`.
Supported values are `"classic"`, `"standard"`, and `"compact"`. New installs
default to `"classic"` with prices shown; upgraded installs without this key keep
the classic layout until changed from the tray menu.

The app creates `themes.json` on first run. Add custom themes there to show them
in the tray menu. Supported font presets are `"default"` (Segoe UI), `"classic"` (Tahoma),
`"web"` (Trebuchet MS), `"terminal"` (Consolas), `"rounded"` (Verdana), and
`"serif"` (Georgia). Bullet shapes are `"triangle"` (default),
`"square"`, `"diamond"`, `"bevel"`, and `"none"`.

Each theme also chooses how its edges are drawn:

- `"border"`: `"none"` (default), `"flat"`, `"raised"`, or `"sunken"`. The style
  applies to the popup frame and the header buttons together. Panels such as the
  ingredient details keep an outline under every style.
- `"border_color"`: hex color for `"flat"` borders. Defaults to `divider_color`.
- `"button_text_color"`: hex color for the header arrows and close mark.
  Defaults to `body_text_color`.
- `"shadow"`: `true` (default) or `false`.

To reset the app:

1. Quit the app from the tray menu.
2. Delete `%LOCALAPPDATA%\compass-lunch\settings.json`.
3. Start the app again.

## Troubleshooting

- Tray icon not visible: click the hidden-icons arrow near the clock and pin the app.
- Menus look outdated: use refresh from the tray menu.
- Data looks broken or stale: quit the app, clear `%LOCALAPPDATA%\compass-lunch\cache\`, and start again.

## Privacy and network

- The app fetches menu data from `lunch.veeti.dev`.
- No login or account is required.

## Uninstall

1. Quit the app from the tray menu.
2. Delete the `.exe`.
3. (Optional) Delete `%LOCALAPPDATA%\compass-lunch\` to remove settings and cache.
