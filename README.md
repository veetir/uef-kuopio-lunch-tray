# Lunch Plasmoid

Unofficial system tray KDE Plasma widget for UEF Kuopio lunch menus.

## Features

- Shows today's lunch in hover tooltip
- Supports
  - Compass: Snellmania, Snellari, Canthia, Tietoteknia, Mediteknia
  - Antell: Round, Highway
  - Hyvä Huomen: Bioteknia
  - Sorrento: Pranzeria
- Language switch (`fi` / `en`)
- Configurable set of restaurants, favorite restaurant
- Mouse-wheel cycling on tray icon to switch restaurant instantly
- Middle-click icon to open restaurant web page
- Automatic and manual refresh

## Screenshot

<p>
  <img src="plasma6/docs/image.png" alt="Compass Lunch widget" width="49%" />
  <img src="plasma6/docs/settings.png" alt="Compass Lunch settings" width="49%" />
</p>

## Install / Update / Remove

Check Plasma version:

```bash
plasmashell --version
```

Clone once:

```bash
git clone https://github.com/veetir/compass-lunch-plasmoid.git
cd compass-lunch-plasmoid
```

Install (Plasma 6):

```bash
kpackagetool6 -t Plasma/Applet -i "$PWD/plasma6"
kbuildsycoca6
systemctl --user restart plasma-plasmashell.service
```

Upgrade existing install:

```bash
git pull
kpackagetool6 -t Plasma/Applet -u "$PWD/plasma6"
kbuildsycoca6
systemctl --user restart plasma-plasmashell.service
```

Remove:

```bash
kpackagetool6 -t Plasma/Applet -r compass-lunch
```

On Plasma 5, run the same commands but use `kpackagetool5` and `"$PWD/plasma5"` instead.

## Windows version

See [Releases](https://github.com/veetir/compass-lunch-plasmoid/releases) for
exe downloads.

### Features

- Navigate restaurants with mouse wheel, header buttons, or `Left/Right` (`A`/`D`)
- Toggle allergens, diet highlights, and price groups
- Themes: dark, light, blue, green, ...
- Highlight favorite dishes
- Automatic/manual refresh
- Run at startup

![Compass Lunch screenshot](windows-tray/assets/windows.png)

---

This project is not affiliated with or endorsed by the University of Eastern Finland or any listed restaurant operators.
