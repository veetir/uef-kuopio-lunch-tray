# Compass Lunch for Windows

Small system tray app for viewing UEF Kuopio campus lunch menus on Windows.

## What this app does

- Shows today's menu in a popup from the Windows tray
- Lets you switch restaurant and language
- Supports manual refresh and auto-refresh

## Download and start

1. Open the project's **GitHub Releases** page:  
   `https://github.com/veetir/compass-lunch-plasmoid/releases`
2. Download the latest Windows `.exe` asset.
3. Place the `.exe` in any folder (for example `Downloads` or `Apps`).
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
- Right-click tray icon: open settings, refresh, and quit

## First-time setup

Open settings from the tray menu and set:

- Language (`fi` or `en`)
- Enabled restaurants
- Favorite/default restaurant
- Run at startup (optional)

## Where your data is stored

- Settings: `%LOCALAPPDATA%\compass-lunch\settings.json`
- Cache: `%LOCALAPPDATA%\compass-lunch\cache\`

To reset the app:

1. Quit the app from the tray menu.
2. Delete `%LOCALAPPDATA%\compass-lunch\settings.json`.
3. Start the app again.

## Troubleshooting

- Tray icon not visible: click the hidden-icons arrow near the clock and pin the app.
- Menus look outdated: use refresh from the tray menu.
- Data looks broken or stale: quit the app, clear `%LOCALAPPDATA%\compass-lunch\cache\`, and start again.

## Privacy and network

- The app fetches menu data from restaurant provider endpoints.
- No login or account is required.

## Uninstall

1. Quit the app from the tray menu.
2. Delete the `.exe`.
3. (Optional) Delete `%LOCALAPPDATA%\compass-lunch\` to remove settings and cache.
