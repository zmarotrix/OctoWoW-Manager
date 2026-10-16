# Addons: OctoLauncher vs. Installation Manager v2.6 vs. v3

## OctoLauncher (`src/main/modules/addons.ts`)
- Shows a **curated catalog** from `https://octowow.st/api/addons.json`: 126 addons with
  TOC info, description, stars and last update.
- Installs by **cloning with isomorphic-git** into `Interface/AddOns/<name>`, so each folder
  keeps a full `.git`. Updates use `git fetch` + pull. It detects local edits
  (`statusMatrix`).
- Only allows 5 git hosts (github, gitlab, gitea, codeberg, octowow.st).
- **Weak spots:** heavy first install (full git objects), `.git` folders in AddOns, slow
  checks (a fetch per addon), and repos with several addons or nested `.toc` files aren't
  handled.

## v2.6 manager
- Downloads **portable MinGit** (Windows only) at runtime, then `git ls-remote` to check for
  updates and `git clone --depth 1` to install. `.git` is removed afterwards and
  `.octowow_meta` (`url`, `sha`) is written.
- Finds the `.toc` in the repo root or one level down, picks the shortest name, and renames
  the folder to match.
- Installs from local `.zip` files (drag & drop).
- **Weak spots:** Windows-only git download, no catalog, and a failed clone can leave a
  half-replaced folder.

## v3 (implemented)
- **No git binary at all, on any OS.**
  - Update checks use git's smart-HTTP ref advertisement (`info/refs?service=git-upload-pack`,
    a few KB). That's what `ls-remote` does, done with plain HTTPS.
  - Installs download the host's zip of that exact commit (GitHub codeload, GitLab
    `/-/archive`, Gitea/Codeberg/OctoWoW `/archive/<sha>.zip`).
  - Tested live against GitHub, OctoWoW git and Codeberg.
- **Atomic replace:** the new folder is fully extracted next to the old one and swapped in.
  If anything fails, the old copy is put back.
- **Smarter `.toc` detection:**
  - unwraps the archive's top folder
  - handles repos containing several addons, including nested ones (`repo/AddOns/X/X.toc`)
  - ignores flavoured TOCs (`-Classic`, `_TBC`, `-Wrath`, …), since 1.12 only reads `<Folder>.toc`
- **Catalog "Browse" view** using the same OctoWoW list as the launcher. It shows what's
  already installed, and one-click installs are tracked like any other managed addon.
- **Launcher-installed addons are recognised:** folders with a `.git` have their origin URL
  and commit read straight from `.git/config` / `HEAD`, so they get update checks too.
  Updating one converts it to a normal managed folder.
- **Branch-aware URLs:** `/tree/<branch>` (GitHub), `/-/tree/<branch>` (GitLab) and
  `/src/branch/<branch>` (Gitea) are honoured, and the branch is stored in `.octowow_meta`.
- **Fully v2.6-compatible:** reads and writes the same `.octowow_meta`, and `tracked_addons`
  imports from the old config. A tracked repo whose folder is gone shows as "Missing" with
  a Repair button, as before.
- **Fast:** update checks run 8 repositories at a time in the background, once per session
  and on demand.
