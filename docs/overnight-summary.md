# Overnight work: everything from v2.6, ported

Nothing was committed and nothing touched your real game folder. All testing used temp
folders and the live OctoWoW release. Run `run-dev.bat` to try it.

## What's new, tab by tab

### Game Settings
- **Layout:** matches v2.6 again (Installation Directory → Environment & QoL switch grid →
  Engine Adjustments), in the new style.
- **Import v2.6 settings:** a button in the Installation card. Pick your old
  `octowow_config.json` (yours is in `dist/`). It brings over the game folder, tweaks,
  plugin choices and sources, custom DLLs, MPQ titles/descriptions and tracked addons. If
  its `client_hash` matches the current release, it also becomes the updater's baseline.
- **Linux/macOS:** a "Wine command" field appears (empty = `wine`), and the DEP switch is
  hidden.

### Client Tweaks
- **Add Custom DLL / remove:** works (file picker or drag & drop). Removal deletes the file
  after a confirm.
- **Credits:** the grid is v2.6's (Mod ↗ / Addon ↗).

### Game Mods
- **Add Mod:** shows v2.6's "New Game Mod Detected" dialog (title + description) for each
  file in turn, then copies it into `Data/`.
- **Toggle / edit / delete:** toggle renames `.mpq` ↔ `.mpq.disabled`; edit and delete (with
  confirm) work. Stock archives are protected.

### Addon Manager
- **Layout:** v2.6's URL bar + search, "Updates Available" (Update All), then
  "Installed Addons".
- **New:** a **Browse** view of the curated OctoWoW catalog (126 addons, the same list the
  launcher uses).
- **No MinGit anymore:** works on every OS. See `docs/addons-comparison.md` for the
  launcher / v2.6 / v3 comparison.
- **Install sources:** from a URL, a `.zip` (button or drop), or the catalog. Repair for
  missing tracked addons.
- **Launcher-installed addons:** git checkouts get update checks too.

### Game Updates
Unchanged from yesterday, except that after a successful update it **re-applies your
tweaks automatically**, like v2.6 did.

### Sidebar
- **Apply Changes:** runs the real v2.6 pipeline (below). The orange dot clears when it succeeds.
- **PLAY:** runs `VanillaFixes.exe WoW_Tweaked.exe` on Windows (as v2.6 did), or the
  configured Wine command elsewhere.
- **Manager update notice:** appears when GitHub has a newer release (v2.6 had this too).

## Apply Changes pipeline (`src-tauri/src/core/apply.rs`)
The same steps as v2.6's `run_installation`:
1. Remove what's switched off.
2. Copy base files.
3. DXVK.
4. Plugins + `dlls.txt` (unmanaged lines kept).
5. `vanilla-tweaks` → `WoW_Tweaked.exe`.
6. Corrupt-interface bypass.
7. DEP exemption (Windows, admin prompt).
8. "Play Modernized WoW" shortcut.

**v2.6 bugs fixed along the way:**
- `transmogfix.dll` and `weirdperformance.dll` were never copied (they sit in
  `Payload/WeirdUtils/`, which v2.6 never looked in for core plugins).
- Companion addons of *disabled* plugins were copied straight back after being removed.
- `payload/DXVK_AMD` and `DXVK_Standard` are empty, so v2.6 never installed DXVK.
  v3 downloads the official DXVK build you describe (v2.6.2 Standard / v2.5.3 AMD) once,
  caches it, and installs its `d3d9.dll`.
- "Latest (GitHub)" now also works when a release ships a `.zip` (SuperWoW does), not only
  a bare `.dll`.
- A plugin `dlls.txt` line is only written if the DLL is actually present.

**Smaller improvements:**
- Companion addons you installed yourself (from git/the catalog) are no longer overwritten.
- The DEP admin prompt is skipped when an exemption is already registered, so it doesn't
  pop up on every Apply.
- The shortcut is created via PowerShell instead of a temp VBS file.

## Tests run
- **18 offline unit tests:** torrent parsing/diff, piece verification, MPQ ops, addon URL
  parsing / ref parsing / `.toc` detection, `dlls.txt` merge, vanilla-tweaks args, limits,
  v2.6 import, version compare, folder swap.
- **Live, end to end (temp folders only):**
  - Built a mini client from the real release.
  - Ran a full Apply: DXVK download, ClassicAPI from GitHub, vanilla-tweaks, bypass,
    `dlls.txt` with custom + unmanaged lines, AutoLogin, companion addons, shortcut.
  - Then turned a plugin off and re-applied to check cleanup.
- **Live addons:** installed from GitHub and OctoWoW git with no git binary, loaded the
  catalog, uninstalled.
- **Live updater:** small real download and a read-only check of your client folder (as
  before).
- **Preview:** every page checked visually in the browser preview.

## Not done / needs your call
- **PLAY is untested** (it would launch WoW). Its command mirrors v2.6 exactly.
- **The DEP exemption was not executed** (it changes Windows security settings). The code
  mirrors v2.6's command.
- **Your real `WoW.exe` is modified (4.7 MB vs 6.7 MB in the release).** Run a Game Update
  first so Apply starts from the stock exe.
- **Update (next day):** vanilla-tweaks is now built in (`core/patcher.rs`), so it works on
  Linux/macOS with no extra binary. PowerShell/elevation is gone. DEP is handled by the
  exe's NX flag, with advice shown if Windows forces DEP system-wide.
- ~~No Linux/macOS binaries are bundled for aria2/vanilla-tweaks~~ No longer applicable: both
  are now built in. CI/release packaging isn't set up.
- **v2.6's payload dumps everything in `Payload/Interface/Addons`** into AddOns. v3 does the
  same, minus disabled companions.
- **Nothing is committed.** The new project is untracked inside the v2.6 repo.
