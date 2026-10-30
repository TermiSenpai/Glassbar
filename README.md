# Glassbar

A glass second toolbar for Windows. Pin it to the taskbar, click it, and your scripts and programs appear on a translucent bar above the taskbar. Click one to launch it; the bar gets out of the way.

Native Rust on Win32, DirectComposition and Direct2D. A single 600 KB executable with no runtime to install. Nothing runs in the background unless you ask for it.

[Versión en español](README.es.md)

![Glassbar above the Windows taskbar](docs/images/bar.png)

## Install

1. Download `Glassbar.exe` from the [latest release](https://github.com/TermiSenpai/Glassbar/releases/latest).
2. Put it anywhere you like.
3. Right-click it and choose **Pin to taskbar**.

Windows 11 is the target. Windows 10 (1903 or later) works with a solid background in the settings window instead of Mica.

If you used BatLauncher before, your entries and icons are imported automatically the first time Glassbar starts. The old folder is left untouched.

## Use

| Action | How |
|---|---|
| Open the bar | Click the pinned icon. |
| Launch an entry | Click its icon, or focus it with the arrow keys and press Enter. |
| Close the bar | Press Escape, click anywhere else, or launch an entry. |
| Add an entry | Click **+**, or drop `.bat`, `.cmd`, `.exe` or `.lnk` files onto the bar. |
| Edit, reorder, remove | Right-click an icon. |
| Many entries | The bar caps at 80 % of the screen and becomes a carousel: drag it, use the wheel, or the arrows at both ends. |
| Settings | Click the gear. |

![Context menu](docs/images/menu.png)

![Edit entry](docs/images/edit.png)

### Keep it in the background

Off by default. Turn on **Keep in the background** in Settings to keep Glassbar in the system tray so the bar opens instantly, and to enable:

- **Start with Windows**
- **Global keyboard shortcut**, for example Win + Alt + Space

![Settings](docs/images/settings.png)

### Updates

Glassbar checks GitHub Releases once a day in the background (you can turn it off) and shows a dot on the gear when a new version exists. Nothing is replaced until you press **Update**; the download is verified against the published `SHA256SUMS` before the executable is swapped.

## Data

Everything lives in `%LOCALAPPDATA%\Glassbar`:

```
entries.json     your entries and their order
settings.json    settings
icons\           copied custom icons
icons.cache      pre-scaled icon bitmaps
log.txt          log (rotated at 1 MB)
```

The only registry value Glassbar ever writes is `HKCU\Software\Microsoft\Windows\CurrentVersion\Run\Glassbar`, and only while **Start with Windows** is on.

## Build

Requires a stable Rust toolchain and the Windows SDK (for the resource compiler).

```
cargo build --release
```

The executable is `target\release\Glassbar.exe`. `cargo test` runs the unit tests. Start the app with `--perf` to log the startup phases to `log.txt`; measurements for the reference machine are in [docs/perf.md](docs/perf.md).

## How it works

- The bar is a popup window without a redirection bitmap; everything on it is a DirectComposition visual drawn once with Direct2D. Animations run in the Windows compositor, so the app does no per-frame work.
- The glass is a snapshot: what is behind the bar is captured and blurred once per opening, which lets the whole plate fade and slide. Settings use the system Mica backdrop where available.
- Drawing uses the WARP software device on purpose: creating a hardware device loads the GPU driver and costs hundreds of milliseconds per process, while WARP costs about 30 ms. Composition still happens on the GPU.
- Icons are cached as pre-scaled bitmaps so a cold start has nothing to decode.

Design notes live in `docs/superpowers/`. The previous WPF application is kept on the `legacy-wpf` branch.

## License

MIT
