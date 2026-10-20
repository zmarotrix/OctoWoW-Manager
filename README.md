# OctoWoW Manager

A cross-platform manager for the OctoWoW (Vanilla 1.12.1) client: engine tweaks, client
plugins, MPQ patches, addons and client updates, in one fast app for **Windows, Linux and
macOS**. It's the successor to the
[OctoWoW Installation Manager](https://github.com/zmarotrix/OctoWoW-Installation-Manager) (v2.6),
rebuilt from scratch in Rust + Tauri.

<img width="1109" height="775" alt="image" src="https://github.com/user-attachments/assets/47aeac75-9496-4d62-aebf-d3f091a9c72b" />
<img width="1112" height="774" alt="image" src="https://github.com/user-attachments/assets/4ec53676-fb46-482d-acf0-3458155e8553" />


## Download

Grab the latest build from **[Releases](../../releases)**. Each one is a single standalone
program with everything (plugin DLLs, companion addons, patcher) built in. There's no installer.

| Platform | File | Run it |
|---|---|---|
| Windows 10/11 | `OctoWoW_Manager_v…_windows_x64.exe` | double-click |
| Linux (x64) | `OctoWoW_Manager_v…_linux_x64.AppImage` | `chmod +x` it, then run. Play uses Wine; set the command in Game Settings |
| macOS (Intel + Apple Silicon) | `OctoWoW_Manager_v…_macos_universal.zip` | unzip, then right-click → Open the first time. Play uses Wine/CrossOver |

Coming from v2.6? Use **Game Settings → Import v2.6 settings** and pick your old
`octowow_config.json`.

## Features

- **Game Settings:** aspect-ratio-correct FoV, render distance, ground clutter, nameplate
  range, camera zoom, audio channels, auto-loot, background sound, Large Address Aware,
  camera-skip fix, corrupt-interface bypass and DEP. Written into `WoW_Tweaked.exe` by a
  built-in port of vanilla-tweaks; your `WoW.exe` is never modified.
- **Client Tweaks:** VanillaFixes, DXVK (Standard 2.6.1, AMD 2.5.3, older-GPU 1.10.3, or off), SuperWoW, nampower, UnitXP_SP3,
  ClassicAPI, VanillaHelpers and the WeirdUtils suite, with "Latest (GitHub)" sources, plus
  your own custom DLLs. `dlls.txt` is managed safely: unknown lines are always kept.
- **Game Mods:** toggle, describe and remove custom `.mpq` patches without deleting them.
- **Addon Manager:** install from any GitHub/GitLab/Gitea/Codeberg URL or `.zip`, browse the
  curated OctoWoW catalog, and update everything with one click. No git install needed.
- **Game Updates:** checks your client against the official release and lists exactly which
  files are missing, outdated or corrupt. It downloads only those, with every piece verified
  against the release checksums, and can resume.
- **Drag & drop** `.mpq`, `.dll` or `.zip` files anywhere on the window.

## Development

### Run it

```bash
run-dev.bat
```

That's `npm install` (first time only) plus `npm run tauri dev`: the real app with hot reload.

**UI-only preview:** `npm run dev`, then open http://localhost:1420 in a browser. A mock
backend (`src/lib/devMock.ts`) supplies sample data.

### Build

Every platform ships as **one standalone program, no installer**. The plugin payload, icon
and notices are compiled into it (`core/payload.rs`) and unpacked to the app data folder
on first Apply.

| Platform | File | How to build |
|---|---|---|
| Windows | `OctoWoW_Manager_v<ver>_windows_x64.exe` | here: `npx tauri build --no-bundle`, then copy `src-tauri/target/release/octowow-manager.exe` |
| Linux | `…_linux_x64.AppImage` (self-contained: `chmod +x` and run) | GitHub Actions |
| macOS | `…_macos_universal.zip` (Intel + Apple Silicon `.app`) | GitHub Actions |

Linux and macOS builds need those operating systems, so they're built by
`.github/workflows/build.yml` on every push to `main`:
- **Each push to `main`, or Actions → "Build" → Run workflow:** builds all three and attaches
  them to the run as downloadable artifacts.
- **Push a tag like `v3.0.0`:** builds all three and also attaches them to a *draft* GitHub
  release.

Local builds go to `releases/` (git-ignored).

**macOS:** the app is ad-hoc signed, not notarized. The first time, right-click it and
choose Open (or run `xattr -dr com.apple.quarantine "OctoWoW Manager.app"`).

### Test

```bash
cd src-tauri && cargo test --lib
```

These are offline unit tests. The live tests hit the real OctoWoW release, GitHub and
OctoWoW git, and only write to temp folders:

```bash
cd src-tauri && cargo test --lib live_ -- --ignored --nocapture --test-threads=1
```

(`OCTOWOW_TEST_CLIENT=<client dir>` additionally runs a read-only check of a real client.
`identical_to_vanilla_tweaks`, with `OCTOWOW_VANILLA_TWEAKS=<vanilla-tweaks 1.6.0 exe>`, proves the
built-in patcher writes byte-identical output to the original tool.)

## Antivirus

v2.6 got flagged mostly because it was a PyInstaller bundle that shelled out to an
exe-patcher and ran elevated PowerShell. v3 avoids those patterns:
- `WoW_Tweaked.exe` is built **in-process** by a native port of vanilla-tweaks (`core/patcher.rs`).
- **No PowerShell, VBScript, `reg`, `tasklist` or UAC elevation.** DEP is handled by building
  the exe without the NX-compat flag, the shortcut is written directly as a `.lnk`, and
  process checks use Win32 APIs.
- **No bundled helper executables** (v2.6 shipped vanilla-tweaks and downloaded aria2 and MinGit
  at runtime). Client downloads are plain HTTPS from the OctoWoW mirror; no torrent client.
- **No packers (UPX).** The PE carries normal version info (publisher, description, copyright).

The remaining risk is reputation: Defender/SmartScreen distrust *new, unsigned* downloads.
In order of impact:
1. **Code-sign releases.** Azure Trusted Signing (~$10/month), or SignPath.io, which is free
   for open-source projects. Signed builds build reputation instead of starting from zero
   with every release.
2. Build releases in CI (GitHub Actions) so every release is reproducible from public source.
3. If a release does get flagged, submit it as a false positive at
   https://www.microsoft.com/wdsi/filesubmission.

## Project layout

```
src/                     React UI
  pages/                 one file per tab
  lib/                   ipc (typed commands), settings, jobs, dialogs, toasts, dev mock
src-tauri/src/
  commands/              IPC commands, grouped by tab
  core/                  the actual work, no UI
    apply.rs             Apply Changes pipeline (v2.6 run_installation)
    patcher.rs           WoW_Tweaked.exe builder (native vanilla-tweaks port, MIT)
    updater.rs           client check / verify / update
    webseed.rs           verified parallel HTTPS downloader (replaces aria2)
    torrent.rs verify.rs
    addons.rs            git-less addon install/update + catalog
    mpq.rs plugins.rs launch.rs migrate.rs app_update.rs platform.rs util.rs
```

Design notes: `DESIGN.md`, `docs/updater-comparison.md`, `docs/addons-comparison.md`.

## Credits

OctoWoW Manager packages and builds on the work of:
[VanillaFixes](https://github.com/hannesmann/vanillafixes) ·
[vanilla-tweaks](https://github.com/brndd/vanilla-tweaks) (patcher ported under MIT, see `THIRD_PARTY_NOTICES.md`) ·
[DXVK](https://github.com/doitsujin/dxvk) ·
[SuperWoW](https://github.com/balakethelock/SuperWoW) ·
[ClassicAPI](https://github.com/brues-code/ClassicAPI) ·
[VanillaHelpers](https://github.com/isfir/VanillaHelpers) ·
[UnitXP_SP3](https://codeberg.org/konaka/UnitXP_SP3) ·
[nampower](https://gitea.com/avitasia/nampower) ·
[PerfBoost](https://gitea.com/avitasia/perf_boost) ·
[WeirdUtils](https://codeberg.org/MarcelineVQ/WeirdUtils) ·
[turtle-autologin](https://github.com/MarcelineVQ/turtle-autologin).

## License

GPL-3.0, like v2.6. Bundled third-party plugins keep their own licenses.
