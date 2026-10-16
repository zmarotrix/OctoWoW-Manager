# Client updates: OctoLauncher vs. Installation Manager v2.6

Both use the same source, `https://dl.octowow.st/download/client.torrent`, and both treat
**the SHA-1 of the .torrent file's bytes as the client "version"**. Both download through `aria2c`.
They differ in how they decide what's wrong and how much work they do to fix it.

Sources: `octolauncher/src/main/modules/updater.ts`, `aria2.ts`; `OctoWow_Install_Manager.py`
(`_check_updates_thread`, `_sync_client_thread`).

---

## How the launcher does it

### Check (runs on every launcher start, and after mod changes)
1. Refuse if WoW is running (`tasklist`), or if the Windows path is longer than 220 characters.
2. Repair `realmlist.wtf` if an interrupted sync left a 0-byte file.
3. Download the torrent and compute its SHA-1.
4. Reconcile the **HTTP side-files**: `Data/patch-O.mpq` (optional "raid visuals",
   checked against `<url>.sha256`) and an optional `patch-5.mpq` content patch (not
   configured in production).
5. Decide the state:
   - **Hash changed, but every file in the torrent exists with the right size** →
     "fast-forward": mark it up to date *without downloading*, record the new hash, cache a
     pristine `WoW.exe`, prune stale archives.
   - **Hash unchanged and all sizes match** → `upToDate`.
   - Otherwise → `updateAvailable`.
6. If up to date, **start seeding** in the background (see below).

> The check uses file **sizes only**, never content. It's fast (a few hundred `stat`
> calls), but a file that changed and kept its size is never detected.

### Update
1. Stop seeding, re-fetch the torrent hash, and try fast-forward again.
2. If the torrent or the client folder changed since the last run, clear aria2's resume
   state (`.aria2` control file).
3. **Build a selection** of the files that are missing or the wrong size. Oversized or
   mismatched files are deleted first. If any file was missing, resume state is cleared,
   because aria2 would otherwise think those pieces are already done.
4. Run aria2 with `--select-file=<only those indices>`, `--check-integrity=false`. Only the
   broken files are touched and nothing else is hashed.
   - The **"clean"/repair mode** instead runs a full `--check-integrity=true` pass with no
     selection.
5. aria2's `--dir` points at a **junction** (`userData/torrent-root/client → clientDir`),
   because the torrent's root folder is named `client`.
6. Progress is parsed from aria2's 1-second summary lines (`12MiB/5.9GiB(0%) … DL:4.2MiB`).
7. Afterwards it re-checks sizes. Then it caches a pristine `WoW.exe`, prunes stale archives
   (old `patch-6..a.mpq`, but **only when name *and* exact byte size match**, so player mods
   are safe) and unused locale folders, removes `d3d9.dll` if DXVK is off, and saves the
   hash and version.

### Seeding
- On by default. Once up to date it spawns a second aria2 that seeds for up to a year at
  2 MB/s upload on port 6881, and opens that port on your router through **UPnP**.
- Skipped when DXVK is off, because aria2 would recreate `d3d9.dll` as a 0-byte file.

### Weak spots
- Downloads and parses the .torrent **3–4 times per check** (hash, tree check, prune, selection).
- Size-only fast-forward can accept a corrupted or outdated file of the same size.
- The junction trick is Windows-only.
- Seeding and UPnP are opt-out, and most users never know they're on.

---

## How the Installation Manager v2.6 does it

### Check (manual, from the Game Updates tab)
1. If `WoW.exe` is missing, offer a full install (~6 GB).
2. Download the torrent and compute its SHA-1.
3. Compare with `client_hash` in the config. **That's the whole check.** No files are looked at.
4. **TLS verification is disabled** (`check_hostname = False`, `CERT_NONE`).

### Sync
1. Download the torrent again and decode it.
2. **Pre-scan**: list every missing or wrong-size file in the console. This is for display
   only and doesn't change what gets downloaded.
3. Write an aria2 **input file** with `index-out=N=<path>` for every file. This maps the
   torrent's paths straight into the game folder, with no junction needed.
4. Download `aria2c.exe` (32-bit, from GitHub, **unverified**) if it isn't next to the exe.
5. Run aria2 with **`--check-integrity=true` every time**. It SHA-1-hashes the entire
   ~6 GB client on every update, then downloads bad pieces.
6. Parse stdout per character for progress, plus "Download complete", "Checksum error"
   and other lines for the activity console.
7. Save `client_hash` on success.

### Weak spots
- "Up to date" only means the torrent didn't change. Deleted or corrupted files go
  unnoticed until someone runs a sync.
- Every sync re-hashes everything, which takes minutes even when one file changed.
- TLS verification is off, and aria2 is downloaded at runtime without a checksum.
- No "is WoW running" guard, no stale-archive cleanup, no resume-state handling.

### Strengths
- `index-out` mapping is simple and cross-platform.
- The pre-scan and activity log give clear feedback on what's wrong.
- Nothing runs in the background and nothing is uploaded.

---

## Side by side

| | Launcher | Manager v2.6 |
|---|---|---|
| Version signal | SHA-1 of .torrent bytes | same |
| "Up to date" means | hash matches **and** all sizes match | hash matches |
| Normal update downloads | only missing/wrong-size files (`--select-file`) | aria2 decides after hashing everything |
| Full hash verify | only in "repair" mode | every time |
| Path mapping | Windows junction | `index-out` (portable) |
| Resume handling | clears `.aria2` state when it would be wrong | none |
| Stale archive cleanup | yes (name + exact size) | no |
| WoW-running guard | yes | no |
| Seeding / UPnP | on by default | none |
| TLS | verified | **disabled** |
| aria2 source | bundled | downloaded at runtime |
| Torrent downloads per check | 3–4 | 1 (check), 1 more (sync) |

---

## Proposal for v3

1. **One torrent fetch per operation.** Download once (with `If-None-Match` /
   `If-Modified-Since`), parse once in Rust, and pass the parsed file list around.
2. **Version = info-hash** (SHA-1 of the bencoded `info` dict), not the whole file. Edits to
   trackers or comments then won't show up as a fake update. Keep reading v2.6's
   `client_hash` for migration.
3. **Quick check = launcher-style size check, shown like your pre-scan.** A few hundred
   `stat` calls take milliseconds. The Updates page lists exactly which files are missing
   or changed, instead of a bare "update available".
4. **Update = download only what's broken** (`--select-file`), using your `index-out`
   mapping instead of a junction so it works on Linux and macOS.
5. **"Verify & repair" = native, parallel piece hashing in Rust**, with per-file progress.
   That's faster than aria2's single-threaded `--check-integrity`. Any file with a bad
   piece goes into the selection, and aria2 fetches only those.
6. **Safety:** TLS on; aria2 bundled per OS as a Tauri sidecar; refuse while WoW is running;
   resume-state handling like the launcher; stale-archive pruning only on exact
   name + size matches; never touch files the manager owns (`d3d9.dll`, our DLLs,
   disabled `*.mpq.disabled` files).
7. **Talk to aria2 over JSON-RPC** (`--enable-rpc`) instead of parsing console text. This
   gives exact bytes, speed, per-file completion and peer count, and lets us pause and resume.
8. **Seeding: opt-in, off by default**, with a clear toggle and upload cap. No UPnP unless
   enabled.

### Questions for you
- **Raid visuals** (`patch-O.mpq` from the HTTP side-channel): offer it as an optional
  download on the Game Mods tab, or ignore it?
- **Seeding:** include it as an opt-in toggle, or leave it out entirely?
- **Check on startup:** run the quick size check automatically on launch, or keep it
  manual like v2.6?

---

## Implemented (v3)

`src-tauri/src/core/{torrent,verify,webseed,updater,platform}.rs`, page `src/pages/GameUpdates.tsx`.

- **Check** runs on startup and on demand. It fetches the torrent once (ETag-cached) and stats every
  file (~2 ms for the 50-file client). It diffs piece hashes against the last synced
  release, so same-size changes are caught *without* hashing; only ambiguous pieces get
  hashed locally.
- **Every file gets a reason**: Missing, Incomplete (with exact size difference), Updated in
  this release, New, Corrupt (after Verify). Obsolete archives and files dropped from
  the release are listed separately.
- **Verify Files** hashes every piece on up to 6 threads.
- **Update** downloads only the files that need it, **without aria2 or any BitTorrent client**.
  It makes parallel HTTPS range requests (6 connections, 8 MiB each) to the torrent's own web
  seed (`dl.octowow.st/client/client/<path>`), and **every 1 MiB piece is SHA-1-checked against
  the torrent** before it's written. It shows live per-file progress, speed, ETA and connections,
  can be cancelled, and resumes from a record of verified pieces.
- **Client Tweaks DLLs are ignored forever**: root-level plugin DLLs, `d3d9.dll` and
  `VanillaFixes.exe` are never downloaded, checked or listed. Only the selected files'
  bytes are ever written, so neighbouring files that share a piece are never touched.
- `realmlist.wtf` is only restored if missing; your edits are kept.
- Updates are blocked while WoW is running (`tasklist` on Windows, `ps` under Wine).
- No seeding and no UPnP.
- Trade-off vs. BitTorrent: no peer-to-peer. The OctoWoW mirror serves everything (measured
  ~7 MB/s per connection, ~23 MB/s with 4), and v3 never seeded anyway.

Not done yet: raid-visuals `patch-O.mpq`; importing v2.6's `client_hash` as a baseline.
