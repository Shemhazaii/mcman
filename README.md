# mcman

A Minecraft server mod/plugin manager for the CIFS-mounted instances at `/mnt/minecraft` (cookcraft, freesia, realcraft, realcraft-exploration, proxy). It runs as a local HTTP server on `127.0.0.1:8765` and also supports a native desktop window.

## Features

- Lists instances and auto-detects loader/MC version from the server jar.
- Lists mods/plugins; toggle enable/disable; trash/restore/purge, including config ownership.
- Modrinth + CurseForge search, install, dependency auto-download, cart batch install.
- Update check, version picker (install specific versions/downgrades), update-all/selected.
- Per-mod config file lookup/opening.
- Diff instances.
- UI settings: language (Indonesian/English), theme (Dark/Light/Follow system), instances folder, CurseForge API key.
- Plugin-only instances (Velocity proxy) install into `plugins/` instead of `mods/`.

## Artifacts

Build artifacts are listed with checksums in `dist/SHA256SUMS`.

GitHub: https://github.com/Shemhazaii/mcman
Release v0.2.0: https://github.com/Shemhazaii/mcman/releases/tag/v0.2.0

### Linux AppImage
`dist/mcman.AppImage` — portable AppImage (x86-64), built in a `rust:1-bookworm` Docker container so it links an older glibc and runs on Ubuntu/Debian.

Run:
```bash
chmod +x mcman.AppImage
./mcman.AppImage --server --port 8765
# or native desktop:
./mcman.AppImage --desktop
```
If FUSE (`libfuse2`) is not installed:
```bash
./mcman.AppImage --appimage-extract-and-run --server --port 8765
```

### Linux tar.gz
`dist/mcman-linux-x64.tar.gz` — contains the portable Linux binary `mcman` plus `README.md`.

```bash
tar -xzf mcman-linux-x64.tar.gz
./mcman --server --port 8765
# or
./mcman --desktop
```

### Windows exe
`dist/mcman-windows-x64.exe` — single PE32+ x86-64 executable. Desktop mode needs WebView2 (present on Windows 10/11). This artifact is cross-compiled and compile-verified; it was not runtime-tested on Windows in this build environment.

```powershell
.\mcman-windows-x64.exe --server --port 8765
# or
.\mcman-windows-x64.exe --desktop
```

## Run from source

```bash
cd mcman-rs
cargo build --release
./target/release/mcman --server --port 8765 --root /mnt/minecraft
# native desktop:
./target/release/mcman --desktop --port 8765
```

Environment overrides:
```bash
MCMAN_ROOT=/mnt/minecraft MCMAN_STATE=~/.local/share/mcman MCMAN_PORT=8765 ./mcman
```

## UI build

The frontend is a Next.js app in `web/`:

```bash
cd web
npm install
npm run build
# output: web/out/
cp -a web/out/. ../static/
cd ..
cargo build --release
```

`static/index.html` is embedded into the Rust binary via `include_str!`, so UI changes require `npm run build`, copying `web/out/` to `static/`, and a Rust rebuild.

## Packaging builds

The full Docker recipe is `dist/build-in-docker.sh`:

```bash
docker run --rm \
  -v ~/scripts/mcman-rs:/app \
  -v mcman_cargo:/usr/local/cargo \
  rust:1-bookworm bash /app/dist/build-in-docker.sh
```

This produces:
- `target/release/bundle/appimage/mcman_0.2.0_amd64.AppImage`
- `target/x86_64-pc-windows-gnu/release/mcman.exe`
- `target/release/mcman` (Linux binary)

Important: linuxdeploy patches `target/release/mcman` in place, so the production server must not be running from that exact path during bundling (Text file busy). Run production from a copy, or stop it first.

## Settings

The gear button is in the sidebar footer (bottom-left). Settings include:
- Language: Indonesia / English
- Theme: Dark / Light / Follow system
- Instances folder: current path + change folder
- CurseForge API key: status + set key

Preferences persist in browser localStorage: `mcman-lang`, `mcman-theme`.

## Notes

- Backend API error messages remain Indonesian even when UI language is English.
- Mods load at server startup only; toggling files while the server runs does not hot-load mods.
- Installing plugins on a Velocity proxy writes to `plugins/`, not `mods/`.
