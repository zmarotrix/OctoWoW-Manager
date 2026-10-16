# OctoWoW Manager: Design

> **Status:** every v2.6 feature is implemented. See `docs/overnight-summary.md` for what
> changed and what's untested, and `README.md` for running, building and testing.

A ground-up rebuild of **the OctoWoW Installation Manager** (the Python/customtkinter app):
the same five tabs and the same feature set, but fast and cross-platform.

**Scope:** this recreates the Installation Manager, not the official OctoLauncher. The
launcher source (`Claude Workspace/octolauncher`) is only a *reference*. Where it does
something better (mainly addon handling and the torrent updater), we compare the two
approaches and borrow the parts that hold up.

---

## 1. Goals

| Goal | Concretely |
|---|---|
| **Snappy** | Opens in under a second. Every click responds within one frame. The UI thread never does file or network I/O. Lists of 300+ addons scroll at 60 fps. |
| **Cross-platform** | Windows, Linux, macOS. The game itself is still the 32-bit Windows `WoW.exe`, so Linux and macOS launch it through Wine, Proton or CrossOver. |
| **Small and clean** | One download of about 10–15 MB. No Python runtime and no PyInstaller, so fewer antivirus false positives. |
| **No external patcher** | `WoW_Tweaked.exe` is built in-process by `core/patcher.rs`, a native port of vanilla-tweaks 1.6.0, verified byte-identical on 28 setting combinations. No PowerShell or elevation either (see README → Antivirus). |

## 2. Stack: Tauri 2 (Rust core + React UI)

```
┌──────────────────────────── UI (system webview) ────────────────────────────┐
│  React 18 + TypeScript + Vite + Tailwind                                     │
│  TanStack Query (cache of core state)   TanStack Virtual (long lists)        │
└───────────────▲──────────────────────────────────────────┬──────────────────┘
                │ events (job progress, state changed)      │ invoke(command)
┌───────────────┴──────────────────────────────────────────▼──────────────────┐
│  Rust core (tokio async)                                                     │
│  config · client · patcher · mods · mpq · addons · updater · launch · jobs   │
│  platform/{windows,unix}                                                     │
└──────────────────────────────────────────────────────────────────────────────┘
        │ sidecar                         │ HTTPS
     (no helper binaries)            OctoWoW mirror / GitHub / GitLab / Codeberg
```

**Why Tauri rather than Electron?** The official OctoLauncher is Electron. It works, but every
copy ships its own Chromium (≈150 MB, slower cold start, 200+ MB RAM). Tauri uses the OS
webview (WebView2 / WebKitGTK / WKWebView), so the app is about 10 MB and starts close to
instantly. Rust is also a good fit for the heavy parts: hashing, zip and tar extraction,
binary patching and parallel downloads.

**Why a web UI rather than native widgets (Qt, Avalonia, Flutter)?** We can borrow the
launcher's React components, Tailwind styling and UX patterns, since it's MIT-licensed. The
UI also looks the same on every OS.

**Trade-off:** the launcher's TypeScript *backend* logic (patcher offsets, dlls.txt handling,
mod catalog, updater) has to be ported to Rust, not copied. It's well-factored, so the port is
mechanical.

## 3. Responsiveness rules

These rules are what make it feel snappy, so every module follows them:

1. **Commands return immediately.** Anything slower than ~50 ms (network, hashing,
   extraction, patching) starts a **Job** and returns a `job_id`.
2. **Jobs stream progress** as `job://progress { id, label, done, total, phase }` events. A
   global **activity tray** at the bottom shows all running jobs. Nothing ever shows a
   blocking modal while it waits.
3. **Optimistic UI.** Toggles flip at once and roll back with a toast if the core reports
   an error.
4. **Core is the source of truth.** The core emits `state://changed { area }`. The UI
   invalidates that TanStack Query key and never re-scans on its own.
5. **Scans are cached.** Addon `.toc` data, MPQ lists and file hashes are cached by
   `(path, mtime, size)`, so opening a tab never re-reads the disk unless something changed.
   A file-system watcher (`notify` crate) on `Interface/AddOns` and `Data` pushes changes in.
6. **Virtualized lists** for addons and mods.
7. **Network is concurrent but polite.** Update checks run in parallel (bounded to about 8),
   use ETags or If-None-Match, and an optional GitHub token avoids the 60/hour rate limit.

## 4. Feature map

| Current app (Python) | v2 module | Borrow from |
|---|---|---|
| Game Settings: FoV, far clip, frill, nameplate range, sound channels, camera distance, quickloot, background sound, LAA, camera-skip fix (via `vanilla-tweaks.exe` → `WoW_Tweaked.exe`) | `patcher` writes `WoW_Tweaked.exe` natively from a pristine `WoW.exe` using a table of offsets | `octolauncher/src/main/modules/patcher.ts` (already has farClip, frill, nameplate, camera distance, LAA, background sound…). Fill any gaps from vanilla-tweaks' published offsets. |
| Aspect-ratio FoV calc, "safety limits" sliders | UI only (`TweaksPage`) | current app |
| Config.wtf tweaks, interface-corruption bypass | `client::config_wtf` | launcher `patchConfig` |
| DLL mods: VanillaFixes, UnitXP_SP3, WeirdUtils, SuperWoW, nampower… with "pull latest" | `mods` with a **data-driven catalog** (JSON: id, source, extract map, `requires`, dlls.txt entry). The catalog can be fetched remotely, so adding a mod doesn't need a new release. | launcher `common/mods.ts`, `modules/mods.ts` |
| Custom DLL import and toggle | `mods::custom` | current app |
| `dlls.txt` management that preserves unmanaged lines | `mods::dlls_txt` | launcher `dllsTxt.ts` |
| DXVK config | `mods` (dxvk entry) + `dxvk.conf` writer | launcher `ensureDxvkConf` |
| MPQ manager: detect, toggle via `.disabled` rename, custom titles and descriptions | `mpq` | current app |
| Addons: scan, read `.toc`, install from GitHub or zip, batch update | `addons` (see §6) | both |
| BitTorrent client updater with pre-scan | `updater` + `webseed`: torrent metadata for the file list and piece hashes; downloads over HTTPS from the torrent's web seed with every piece SHA-1-verified (no aria2) | launcher `updater.ts` |
| Drag & drop `.mpq` / `.dll` / `.zip` anywhere | Tauri native file-drop event → router by extension → opens the right page with a confirm sheet | current app |
| Launch game | `launch` (see §7) | launcher `routers/launcher.ts` |
| App self-update check | `tauri-plugin-updater` against GitHub Releases | — |
| Desktop shortcut, DEP / process mitigations | `platform::windows` (hidden on other OSes) | current app |

## 5. UI layout

Same structure as the current app: sidebar with the five tabs, with Apply Changes and Play
pinned at the bottom. New: a status bar for background jobs.

```
┌──────────────────┬───────────────────────────────────────────────────────┐
│ OCTOWOW          │                                                       │
│ Install Mgr v3   │                 page content                          │
│                  │   (cards, switches, sliders, virtualized lists)       │
│ ⚙ Game Settings  │                                                       │
│ 🔌 Client Tweaks │                                                       │
│ 🎨 Game Mods     │                                                       │
│ 📦 Addon Manager │                                                       │
│ 🚀 Game Updates  │                                                       │
│                  │                                                       │
│ [💾 Apply Changes]├───────────────────────────────────────────────────────┤
│ [▶ PLAY GAME]    │ ⟳ Checking 42 addons… 17/42 ▓▓▓▓░░░             (jobs)│
└──────────────────┴───────────────────────────────────────────────────────┘
```

- **Game Settings:** installation directory; environment (DXVK Standard / AMD) and
  quality-of-life switches (auto login, auto-loot, background sound, LAA, camera-skip fix,
  corrupt-interface bypass, DEP); engine adjustments (aspect ratio → FoV, render distance,
  ground clutter, nameplate distance, camera zoom, audio channels) with "Disable Safety
  Limits".
- **Client Tweaks:** Recommended Core DLLs (with Recommended / Latest-from-GitHub source
  pickers), Optional WeirdUtils, Custom User Plugins, credits.
- **Game Mods:** custom MPQs in `Data/` with toggles and editable title and description.
- **Addon Manager:** search, installed list with update badges, Update All, add from URL or zip.
- **Game Updates:** torrent sync with a pre-scan and live stats.
- **Apply Changes** stays an explicit, staged step (as today). The button shows a dot when
  there are unapplied changes.
- Theme: the current palette (`#0B0F19` background, `#6366F1` accent).

## 6. Addons approach

Two options:

- **A. Archive-based (recommended).** Resolve the repo → download the zipball for the
  default branch or latest release → find the folder(s) containing `.toc` files → install
  them under the right names. Record `{source, ref, sha}` in a manifest. An update check is
  one API call per addon ("latest commit sha on branch"). No git needed, fast, and it works
  for every host.
- **B. Git clone (what the launcher does with isomorphic-git).** Exact history and cheap
  pulls, but slower first install, `.git` folders in AddOns, and more edge cases.

Proposal: A by default. If an addon folder already has a `.git` (for example from the official
launcher), treat it as git-managed and only report its status, without overwriting it.

## 7. Launching and cross-platform

| | Windows | Linux | macOS |
|---|---|---|---|
| Run game | `WoW_Tweaked.exe` directly | configurable runner: `wine`, Proton, or Lutris/Bottles command; `WINEPREFIX` setting | CrossOver bottle or wine (best-effort: 32-bit Windows apps on Apple Silicon are fragile) |
| DLL chainloading (VanillaFixes) | native | works under Wine | works under CrossOver/Wine |
| Client downloads | built in (HTTPS web seed) | built in | built in |
| DEP / mitigations, shortcuts | yes | n/a (hidden) | n/a (hidden) |

All OS-specific code lives in `platform/`. Pages don't show features the current OS can't use.

## 8. Data and persistence

- **App settings** (window, theme, runner, GitHub token, last game folder):
  OS app-config dir, `settings.json`.
- **Per-install state** (MPQ titles, custom DLL list, addon sources and shas, applied tweak
  values): `<game folder>/.octomanager/state.json`, so the state moves with the client.
- **Migration:** on first run, if `octowow_config.json` from the Python app is found next to
  the old exe or in the game folder, import MPQ metadata, custom DLLs and addon URLs.

## 9. Project layout

```
octowow-manager/
  package.json, vite.config.ts, tailwind.config.ts
  src/                    React UI
    pages/  components/  lib/ipc.ts (typed invoke + event hooks)
  src-tauri/
    tauri.conf.json, Cargo.toml
    src/
      main.rs  commands.rs  jobs.rs  state.rs
      core/{config,client,patcher,mods,dlls_txt,mpq,addons,updater,launch}.rs
      platform/{mod,windows,unix}.rs
```

Typed IPC: Rust types are exported to TypeScript (`specta` / `tauri-specta`), so the UI and
core can't drift apart.

## 10. Build and release

GitHub Actions with `tauri-action`, as a matrix:
- Windows: NSIS installer + portable exe
- Linux: AppImage + .deb
- macOS: .dmg (universal)

Unsigned builds still trigger SmartScreen and Gatekeeper warnings, but much less often than a PyInstaller exe that patches other exes.

## 11. Implementation order (after design sign-off)

1. **Skeleton (done first):** Tauri + React shell, all five tabs with their controls wired
   to persisted state, job system + status bar, game-folder validation, read-only scans of
   MPQs and addons, drag & drop routing. Apply, Play, update checks and downloads are stubs.
2. Game Mods (MPQ toggle, rename, metadata) + drag & drop install.
3. Addon Manager: install, update and zip import. Compare with the launcher first.
4. Client Tweaks: DLL catalog, dlls.txt, custom DLLs, GitHub "Latest" sources.
5. Game Settings: native patcher (verify byte-for-byte against `vanilla-tweaks.exe`),
   Config.wtf, DXVK, Apply pipeline.
6. Play (Windows, then Wine runner).
7. Game Updates: torrent metadata + verified HTTPS web-seed downloads. Compare with the launcher first.
8. Self-update, migration from v2.6 config, CI release matrix.

## 12. Open questions

1. **Addons and updates vs. the launcher:** write a side-by-side comparison when we fill
   out those tabs.
2. **Drop `vanilla-tweaks.exe`?** Proposed yes (native patcher), keeping `WoW_Tweaked.exe` as
   the output so `WoW.exe` stays pristine.
3. **Addons:** archive-based (A) by default?
4. **macOS:** officially supported, or best-effort?
5. **Name:** keep "OctoWoW Installation Manager", or something new?
