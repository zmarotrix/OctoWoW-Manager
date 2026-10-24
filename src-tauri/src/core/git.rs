//! Built-in git client (gitoxide), used where plain HTTP isn't enough:
//!
//! - a depth-1 clone for hosts that don't serve zip archives;
//! - updating addons that are git checkouts (OctoLauncher, GitAddonsManager)
//!   in place, keeping `.git`, the same way GitAddonsManager does: fetch, then
//!   force-checkout the branch's new commit;
//! - reading which subfolders of a multi-addon checkout are addons, so
//!   GitAddonsManager's `<Name>.repo` layout can be recognised.
//!
//! Everything here blocks; call it from a blocking thread.

use std::collections::HashSet;
use std::num::NonZeroU32;
use std::path::{Path, PathBuf};
use std::sync::atomic::AtomicBool;

fn err(e: impl std::fmt::Display) -> String {
    e.to_string()
}

/// Clones the tip of `branch` (or the default branch) of `url` into `dest`
/// without history. Returns the commit sha.
pub fn clone_shallow(url: &str, dest: &Path, branch: Option<&str>) -> Result<String, String> {
    let stop = AtomicBool::new(false);
    let mut prep = gix::prepare_clone(url, dest)
        .map_err(err)?
        .with_shallow(gix::remote::fetch::Shallow::DepthAtRemote(NonZeroU32::MIN));
    if let Some(b) = branch {
        prep = prep.with_ref_name(Some(b)).map_err(err)?;
    }
    let (mut checkout, _) = prep.fetch_then_checkout(gix::progress::Discard, &stop).map_err(|e| format!("git clone of {url} failed: {e}"))?;
    let (repo, _) = checkout.main_worktree(gix::progress::Discard, &stop).map_err(err)?;
    Ok(repo.head_id().map_err(err)?.to_string())
}

/// The branch checked out in `dir` (`None` when HEAD is detached).
pub fn current_branch(dir: &Path) -> Option<String> {
    let repo = gix::open(dir).ok()?;
    let name = repo.head_name().ok()??;
    Some(name.shorten().to_string())
}

/// Fetches from the checkout's remote and force-checks-out the new commit of
/// `branch` (or the branch already checked out). Files the new commit no
/// longer has are deleted; untracked files are left alone; `.git` is kept.
/// Returns the new commit sha.
pub fn update_checkout(dir: &Path, branch: Option<&str>) -> Result<String, String> {
    let stop = AtomicBool::new(false);
    let repo = gix::open(dir).map_err(|e| format!("{} isn't a usable git checkout: {e}", dir.display()))?;
    let head_branch = repo.head_name().ok().flatten().map(|n| n.shorten().to_string());
    let branch = branch.map(str::to_string).or(head_branch.clone());

    let remote = repo.find_default_remote(gix::remote::Direction::Fetch).ok_or("The checkout has no remote to update from")?.map_err(err)?;
    let remote_name = remote.name().map(|n| n.as_bstr().to_string()).unwrap_or_else(|| "origin".into());
    let outcome = remote
        .connect(gix::remote::Direction::Fetch)
        .map_err(|e| format!("Couldn't reach the repository: {e}"))?
        .prepare_fetch(gix::progress::Discard, Default::default())
        .map_err(|e| format!("Couldn't reach the repository: {e}"))?
        .receive(gix::progress::Discard, &stop)
        .map_err(|e| format!("git fetch failed: {e}"))?;

    // The branch's tip as the remote advertised it, falling back to the tracking ref.
    let wanted = branch.as_ref().map(|b| format!("refs/heads/{b}"));
    let advertised = outcome.ref_map.remote_refs.iter().find_map(|r| {
        let (name, target, _) = r.unpack();
        let hit = match &wanted {
            Some(w) => name == w.as_bytes(),
            None => name == "HEAD".as_bytes(),
        };
        hit.then(|| target.map(|t| t.to_owned())).flatten()
    });
    let new = match advertised {
        Some(id) => id,
        None => {
            let b = branch.as_deref().ok_or("The checkout isn't on a branch")?;
            repo.find_reference(format!("refs/remotes/{remote_name}/{b}").as_str())
                .map_err(|_| format!("Branch '{b}' wasn't found on the remote"))?
                .peel_to_id()
                .map_err(err)?
                .detach()
        }
    };
    if !repo.has_object(new) {
        return Err(format!("The remote didn't send commit {new}"));
    }
    // Move the branch (or the detached HEAD) to the new commit.
    let target = match &head_branch {
        Some(b) if Some(b) == branch.as_ref() => format!("refs/heads/{b}"),
        _ => "HEAD".to_string(),
    };
    force_checkout(&repo, new, &target)?;
    Ok(new.to_string())
}

/// `git checkout --force` of `commit`, then points `target` (a branch or HEAD) at it.
pub(crate) fn force_checkout(repo: &gix::Repository, commit: gix::ObjectId, target: &str) -> Result<(), String> {
    let stop = AtomicBool::new(false);
    let workdir = repo.workdir().ok_or("Bare repositories can't be updated")?.to_path_buf();
    // Build the new index, delete what the new commit drops, then write files.
    let tree = repo.find_object(commit).map_err(err)?.peel_to_tree().map_err(err)?.id;
    let mut index = repo.index_from_tree(&tree).map_err(err)?;
    if let Ok(prev) = repo.open_index() {
        let keep: HashSet<&[u8]> = index.entries().iter().map(|e| -> &[u8] { e.path(&index) }).collect();
        for e in prev.entries() {
            let rel = e.path(&prev);
            if !keep.contains::<[u8]>(rel.as_ref()) {
                remove_tracked(&workdir, &String::from_utf8_lossy(rel));
            }
        }
    }
    let mut opts = repo.checkout_options(gix::worktree::stack::state::attributes::Source::IdMapping).map_err(err)?;
    opts.overwrite_existing = true;
    let objects = repo.objects.clone().into_arc().map_err(err)?;
    let result = gix::worktree::state::checkout(&mut index, &workdir, objects, &gix::progress::Discard, &gix::progress::Discard, &stop, opts).map_err(err)?;
    if let Some(e) = result.errors.first() {
        return Err(format!("Couldn't write {} (is the game running?): {}", e.path, e.error));
    }
    index.write(Default::default()).map_err(err)?;

    repo.reference(target, commit, gix::refs::transaction::PreviousValue::Any, "octowow-manager: update").map_err(err)?;
    Ok(())
}

/// Deletes a file the new commit no longer tracks, plus parent folders it leaves empty.
fn remove_tracked(workdir: &Path, rel: &str) {
    if rel.split('/').any(|p| p == ".." || p == ".git") {
        return;
    }
    let path = workdir.join(rel);
    if std::fs::remove_file(&path).is_err() {
        return;
    }
    let mut dir = path.parent().map(PathBuf::from);
    while let Some(d) = dir {
        if d == workdir || std::fs::remove_dir(&d).is_err() {
            break;
        }
        dir = d.parent().map(PathBuf::from);
    }
}

/// Top-level folders of the checked-out commit that are addons of their own
/// (`<Folder>/<Folder>.toc`, flavour suffixes allowed). GitAddonsManager moves
/// these out of the checkout into AddOns.
pub fn addon_subfolders(dir: &Path, is_flavoured_toc: impl Fn(&str, &str) -> bool) -> Vec<String> {
    let Ok(repo) = gix::open(dir) else { return Vec::new() };
    let Ok(tree) = repo.head_commit().and_then(|c| c.tree().map_err(Into::into)) else { return Vec::new() };
    let mut out = Vec::new();
    for entry in tree.iter().flatten() {
        if !entry.mode().is_tree() {
            continue;
        }
        let name = entry.filename().to_string();
        let Ok(sub) = repo.find_tree(entry.oid()) else { continue };
        let is_addon = sub.iter().flatten().any(|f| {
            let file = f.filename().to_string();
            f.mode().is_blob() && file.to_ascii_lowercase().ends_with(".toc") && is_flavoured_toc(&name, &file[..file.len() - 4])
        });
        if is_addon {
            out.push(name);
        }
    }
    out
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;
    use crate::core::util::remove_dir_force;

    /// Full clone like GitAddonsManager makes, rewound, then fetched and updated in place.
    pub(crate) fn full_clone(url: &str, dest: &Path) -> gix::Repository {
        let stop = AtomicBool::new(false);
        let (mut co, _) = gix::prepare_clone(url, dest).unwrap().fetch_then_checkout(gix::progress::Discard, &stop).unwrap();
        co.main_worktree(gix::progress::Discard, &stop).unwrap().0
    }

    pub(crate) fn rewind(repo: &gix::Repository, by: usize) -> gix::ObjectId {
        let old = repo.rev_parse_single(format!("HEAD~{by}").as_str()).unwrap().detach();
        let head = repo.head_name().unwrap().unwrap();
        force_checkout(repo, old, &head.as_bstr().to_string()).unwrap();
        old
    }

    #[test]
    #[ignore]
    fn live_update_in_place() {
        let root = std::env::temp_dir().join(format!("octo-git-live-{}", std::process::id()));
        let _ = remove_dir_force(&root);
        std::fs::create_dir_all(&root).unwrap();
        let dir = root.join("ShaguTweaks");
        let repo = full_clone("https://github.com/shagu/ShaguTweaks.git", &dir);
        let tip = repo.head_id().unwrap().detach();
        let old = rewind(&repo, 25);
        std::fs::write(dir.join("untracked.txt"), "keep me").unwrap();
        assert_eq!(gix::open(&dir).unwrap().head_id().unwrap().detach(), old);
        assert!(gix::open(&dir).unwrap().is_dirty().is_ok_and(|d| !d), "rewound checkout should be clean");

        assert_eq!(update_checkout(&dir, None).unwrap(), tip.to_string());
        let repo = gix::open(&dir).unwrap();
        assert_eq!(repo.head_id().unwrap().detach(), tip);
        assert!(repo.head_name().unwrap().is_some(), "still on its branch");
        assert!(!repo.is_dirty().unwrap(), "worktree matches the new commit");
        assert!(dir.join("untracked.txt").exists() && dir.join(".git").is_dir());
        // A second update is a no-op.
        assert_eq!(update_checkout(&dir, None).unwrap(), tip.to_string());

        // Shallow clone from OctoWoW's git host.
        let sha = clone_shallow("https://git.octowow.st/git/shaga/NampowerSettings.git", &root.join("np"), None).unwrap();
        assert_eq!(sha.len(), 40);
        assert!(root.join("np/.git/shallow").exists());
        remove_dir_force(&root).unwrap();
    }
}
