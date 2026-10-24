//! Addons in `Interface/AddOns`.
//!
//! Installs and updates without a git binary: the latest commit is read with
//! git's smart-HTTP ref advertisement (works on GitHub, GitLab, Gitea,
//! Codeberg and OctoWoW's own git), then the host's zip archive of that commit
//! is extracted and the folder(s) holding a `.toc` are swapped in atomically.
//! Each installed folder gets v2.6-compatible `.octowow_meta` (`url`, `sha`).

use super::util::{extract_zip, get_bytes, http, remove_dir_force, replace_dir, scratch_dir};
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};

pub const CATALOG_URL: &str = "https://octowow.st/api/addons.json";

#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AddonEntry {
    pub folder: String,
    pub title: String,
    pub version: String,
    pub author: String,
    pub notes: String,
    pub url: Option<String>,
    pub local_sha: Option<String>,
    pub remote_sha: Option<String>,
    pub update_available: bool,
    pub managed: bool,
    /// Tracked by URL but the folder is gone.
    pub missing: bool,
    /// Folder is a git checkout (e.g. installed by the OctoLauncher).
    pub is_git: bool,
}

#[derive(Default, Serialize, Deserialize)]
struct OctoMeta {
    url: Option<String>,
    sha: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    branch: Option<String>,
}

// ---------- repository URLs ----------

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Host {
    GitHub,
    GitLab,
    /// Gitea / Forgejo / Codeberg / OctoWoW git.
    Gitea,
}

#[derive(Clone, Debug)]
pub struct Repo {
    /// `https://host/path/name` (no `.git`).
    pub web: String,
    pub host: Host,
    pub owner: String,
    pub name: String,
    pub branch: Option<String>,
}

impl Repo {
    /// Where git requests go. OctoWoW's `octowow.st/git/` sits behind a bot
    /// check that git clients can't pass; the same repos answer git requests on
    /// `git.octowow.st/git/`.
    pub fn git_url(&self) -> String {
        let web = match self.web.strip_prefix(OCTOWOW_WEB) {
            Some(rest) => format!("{OCTOWOW_GIT}{rest}"),
            None => self.web.clone(),
        };
        format!("{web}.git")
    }

    pub fn archive_url(&self, sha: &str) -> String {
        match self.host {
            Host::GitHub => format!("https://codeload.github.com/{}/{}/zip/{sha}", self.owner, self.name),
            Host::GitLab => format!("{}/-/archive/{sha}/{}-{sha}.zip", self.web, self.name),
            Host::Gitea => format!("{}/archive/{sha}.zip", self.web),
        }
    }
}

/// Normalised key for comparing repo URLs typed in different ways.
const OCTOWOW_WEB: &str = "https://octowow.st/git/";
const OCTOWOW_GIT: &str = "https://git.octowow.st/git/";

/// Normalised key for comparing repo URLs typed in different ways. Both OctoWoW
/// git addresses name the same repos.
pub fn url_key(url: &str) -> String {
    let key = parse_repo(url).map(|r| r.web.to_ascii_lowercase()).unwrap_or_else(|_| url.trim().trim_end_matches('/').trim_end_matches(".git").to_ascii_lowercase());
    match key.strip_prefix(OCTOWOW_GIT) {
        Some(rest) => format!("{OCTOWOW_WEB}{rest}"),
        None => key,
    }
}

/// Accepts repo URLs as copied from a browser or a clone button, including
/// `/tree/<branch>` (GitHub), `/-/tree/<branch>` (GitLab), `/src/branch/<branch>` (Gitea).
pub fn parse_repo(url: &str) -> Result<Repo, String> {
    let url = url.trim();
    let rest = url.strip_prefix("https://").ok_or("Use an https:// repository URL")?;
    let (host, path) = rest.split_once('/').ok_or("That URL doesn't point to a repository")?;
    let host = host.to_ascii_lowercase();
    let mut parts: Vec<&str> = path.split(['?', '#']).next().unwrap_or("").split('/').filter(|p| !p.is_empty()).collect();

    let mut branch = None;
    let kind = if host == "github.com" || host == "www.github.com" {
        if let Some(i) = parts.iter().position(|p| *p == "tree") {
            branch = parts.get(i + 1).map(|b| b.to_string());
            parts.truncate(i);
        }
        parts.truncate(2);
        Host::GitHub
    } else if host == "gitlab.com" {
        if let Some(i) = parts.iter().position(|p| *p == "-") {
            if parts.get(i + 1) == Some(&"tree") {
                branch = parts.get(i + 2).map(|b| b.to_string());
            }
            parts.truncate(i);
        }
        Host::GitLab
    } else {
        if let Some(i) = parts.iter().position(|p| *p == "src") {
            if parts.get(i + 1) == Some(&"branch") {
                branch = parts.get(i + 2).map(|b| b.to_string());
            }
            parts.truncate(i);
        }
        Host::Gitea
    };
    if let Some(last) = parts.last_mut() {
        *last = last.strip_suffix(".git").unwrap_or(last);
    }
    if parts.len() < 2 || parts.iter().any(|p| *p == ".." || p.contains('\\')) {
        return Err("That URL doesn't look like a repository (expected https://host/owner/name)".into());
    }
    let name = parts[parts.len() - 1].to_string();
    let owner = parts[parts.len() - 2].to_string();
    let host_out = if kind == Host::GitHub { "github.com".to_string() } else { host };
    Ok(Repo { web: format!("https://{host_out}/{}", parts.join("/")), host: kind, owner, name, branch })
}

/// Latest commit of the branch (or the default branch) via git smart-HTTP.
/// Returns `(sha, branch)`.
pub async fn ls_remote(repo: &Repo) -> Result<(String, String), String> {
    let url = format!("{}/info/refs?service=git-upload-pack", repo.git_url());
    let res = http()
        .get(&url)
        .header(reqwest::header::USER_AGENT, "git/2.45.0 (octowow-manager)")
        .send()
        .await
        .map_err(|e| format!("Couldn't reach {}: {e}", repo.web))?;
    if res.status() == reqwest::StatusCode::NOT_FOUND || res.status() == reqwest::StatusCode::UNAUTHORIZED {
        return Err(format!("Repository not found (or private): {}", repo.web));
    }
    if !res.status().is_success() {
        return Err(format!("{} answered HTTP {}", repo.web, res.status()));
    }
    let body = res.bytes().await.map_err(|e| e.to_string())?;
    let (refs, head_target) = parse_ref_advertisement(&body).ok_or_else(|| {
        format!("{} didn't answer like a git server. It may be behind a bot check; download the .zip in your browser and drop it on the window instead.", repo.web)
    })?;
    let branch = repo.branch.clone().or(head_target).unwrap_or_else(|| "master".into());
    let sha = refs
        .get(&format!("refs/heads/{branch}"))
        .or_else(|| refs.get(&format!("refs/tags/{branch}")))
        .or_else(|| if repo.branch.is_none() { refs.get("HEAD") } else { None })
        .cloned()
        .ok_or_else(|| format!("Branch '{branch}' not found in {}", repo.web))?;
    Ok((sha, branch))
}

/// Parses pkt-lines from `info/refs`: ref name → sha, plus HEAD's symref target branch.
fn parse_ref_advertisement(body: &[u8]) -> Option<(HashMap<String, String>, Option<String>)> {
    let mut refs = HashMap::new();
    let mut head = None;
    let mut pos = 0;
    let mut saw_service = false;
    while pos + 4 <= body.len() {
        let len = usize::from_str_radix(std::str::from_utf8(&body[pos..pos + 4]).ok()?, 16).ok()?;
        if len == 0 {
            pos += 4;
            continue;
        }
        if len < 4 || pos + len > body.len() {
            return None;
        }
        let line = String::from_utf8_lossy(&body[pos + 4..pos + len]).trim_end_matches('\n').to_string();
        pos += len;
        if line.starts_with("# service=") {
            saw_service = true;
            continue;
        }
        let (refpart, caps) = line.split_once('\0').unwrap_or((&line, ""));
        let (sha, name) = refpart.split_once(' ')?;
        if sha.len() == 40 {
            refs.insert(name.to_string(), sha.to_string());
        }
        if let Some(t) = caps.split(' ').find_map(|c| c.strip_prefix("symref=HEAD:refs/heads/")) {
            head = Some(t.to_string());
        }
    }
    (saw_service && !refs.is_empty()).then_some((refs, head))
}

// ---------- finding addon folders ----------

/// TOC suffixes for other game flavours; the 1.12 client only reads `<Folder>.toc`.
const FLAVOR_SUFFIXES: [&str; 12] = ["classic", "bcc", "tbc", "wotlkc", "wrath", "mainline", "cata", "mists", "vanilla", "era", "retail", "sod"];

fn pick_toc(dir: &Path) -> Option<String> {
    let mut tocs: Vec<String> = std::fs::read_dir(dir)
        .ok()?
        .flatten()
        .filter_map(|e| {
            let n = e.file_name().to_string_lossy().into_owned();
            (n.to_ascii_lowercase().ends_with(".toc") && e.path().is_file()).then(|| n[..n.len() - 4].to_string())
        })
        .collect();
    let flavoured = |s: &str| {
        let l = s.to_ascii_lowercase();
        FLAVOR_SUFFIXES.iter().any(|f| l.ends_with(&format!("-{f}")) || l.ends_with(&format!("_{f}")))
    };
    tocs.sort_by_key(|s| (flavoured(s), s.len()));
    tocs.into_iter().next()
}

fn subdirs(dir: &Path) -> Vec<PathBuf> {
    let mut v: Vec<PathBuf> = std::fs::read_dir(dir).into_iter().flatten().flatten().map(|e| e.path()).filter(|p| p.is_dir() && !p.file_name().is_some_and(|n| n.to_string_lossy().starts_with('.'))).collect();
    v.sort();
    v
}

/// Folders inside an extracted archive that are addons, with the name each
/// must be installed under (its `.toc` stem).
pub fn find_addon_dirs(extracted: &Path) -> Vec<(PathBuf, String)> {
    // Archives usually wrap everything in one top folder (e.g. `pfUI-<sha>/`).
    let mut base = extracted.to_path_buf();
    loop {
        let entries: Vec<_> = std::fs::read_dir(&base).into_iter().flatten().flatten().collect();
        match entries.as_slice() {
            [only] if only.path().is_dir() && pick_toc(&base).is_none() => base = only.path(),
            _ => break,
        }
    }
    if let Some(toc) = pick_toc(&base) {
        return vec![(base, toc)];
    }
    let mut found: Vec<(PathBuf, String)> = subdirs(&base).into_iter().filter_map(|d| pick_toc(&d).map(|t| (d, t))).collect();
    if found.is_empty() {
        // e.g. repo/AddOns/<Addon>/<Addon>.toc
        for d in subdirs(&base) {
            found.extend(subdirs(&d).into_iter().filter_map(|s| pick_toc(&s).map(|t| (s, t))));
        }
    }
    found
}

fn safe_folder(name: &str) -> bool {
    !name.is_empty() && name != "." && name != ".." && !name.contains(['/', '\\', ':']) && !name.starts_with('.')
}

/// Moves each found addon into `addons_dir`, tagging it with `.octowow_meta` when from a repo.
fn install_found(addons_dir: &Path, extracted: &Path, meta: Option<&OctoMeta>, what: &str) -> Result<Vec<String>, String> {
    let found = find_addon_dirs(extracted);
    if found.is_empty() {
        return Err(format!("No addon (.toc file) found in {what}."));
    }
    let mut installed = Vec::new();
    for (src, name) in found {
        if !safe_folder(&name) {
            continue;
        }
        let _ = remove_dir_force(&src.join(".git"));
        if let Some(m) = meta {
            std::fs::write(src.join(".octowow_meta"), serde_json::to_vec(m).unwrap()).map_err(|e| e.to_string())?;
        }
        replace_dir(&src, &addons_dir.join(&name))?;
        installed.push(name);
    }
    Ok(installed)
}

/// Installs (or updates) every addon in a repository. Returns `(folders, sha)`.
pub async fn install_from_url(addons_dir: &Path, url: &str) -> Result<(Vec<String>, String), String> {
    let repo = parse_repo(url)?;
    let (sha, branch) = ls_remote(&repo).await?;
    let bytes = get_bytes(&repo.archive_url(&sha)).await?;
    std::fs::create_dir_all(addons_dir).map_err(|e| e.to_string())?;
    let tmp = scratch_dir(addons_dir, "dl")?;
    let result = (|| {
        extract_zip(std::io::Cursor::new(bytes), &tmp)?;
        let meta = OctoMeta { url: Some(url.trim().to_string()), sha: Some(sha.clone()), branch: repo.branch.as_ref().map(|_| branch.clone()) };
        install_found(addons_dir, &tmp, Some(&meta), &repo.web)
    })();
    let _ = remove_dir_force(&tmp);
    result.map(|folders| (folders, sha))
}

pub fn install_from_zip(addons_dir: &Path, zip: &Path) -> Result<Vec<String>, String> {
    let file = std::fs::File::open(zip).map_err(|e| format!("Couldn't open {}: {e}", zip.display()))?;
    std::fs::create_dir_all(addons_dir).map_err(|e| e.to_string())?;
    let tmp = scratch_dir(addons_dir, "zip")?;
    let name = zip.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
    let result = extract_zip(file, &tmp).and_then(|_| install_found(addons_dir, &tmp, None, &name));
    let _ = remove_dir_force(&tmp);
    result
}

pub fn remove(addons_dir: &Path, folder: &str) -> Result<(), String> {
    if !safe_folder(folder) {
        return Err(format!("Invalid addon folder '{folder}'"));
    }
    remove_dir_force(&addons_dir.join(folder)).map_err(|e| format!("Couldn't remove {folder} (is the game running?): {e}"))
}

// ---------- scanning ----------

/// Removes WoW color codes (`|cAARRGGBB` ... `|r`) from TOC strings.
fn strip_colors(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    let mut chars = s.chars().peekable();
    while let Some(c) = chars.next() {
        if c == '|' {
            match chars.peek() {
                Some('c') | Some('C') => {
                    chars.next();
                    for _ in 0..8 {
                        chars.next();
                    }
                    continue;
                }
                Some('r') | Some('R') => {
                    chars.next();
                    continue;
                }
                _ => {}
            }
        }
        out.push(c);
    }
    out.trim().to_string()
}

#[derive(Default)]
struct Toc {
    title: String,
    version: String,
    author: String,
    notes: String,
}

fn read_toc(dir: &Path, folder: &str) -> Toc {
    let mut toc = Toc::default();
    // TOC files are often Latin-1; lossy decoding is fine for display.
    let Ok(bytes) = std::fs::read(dir.join(format!("{folder}.toc"))) else { return toc };
    for line in String::from_utf8_lossy(&bytes).trim_start_matches('\u{feff}').lines() {
        let Some(rest) = line.strip_prefix("##") else { continue };
        let Some((key, value)) = rest.split_once(':') else { continue };
        let value = strip_colors(value);
        match key.trim() {
            "Title" => toc.title = value,
            "Version" => toc.version = value,
            "Author" => toc.author = value,
            "Notes" => toc.notes = value,
            _ => {}
        }
    }
    toc
}

/// Origin URL and checked-out commit of a git working copy, read straight from `.git`.
fn read_git_checkout(dir: &Path) -> Option<(String, String)> {
    let git = dir.join(".git");
    let config = std::fs::read_to_string(git.join("config")).ok()?;
    let mut in_origin = false;
    let mut url = None;
    for line in config.lines().map(str::trim) {
        if line.starts_with('[') {
            in_origin = line == "[remote \"origin\"]";
        } else if in_origin {
            if let Some(v) = line.strip_prefix("url").map(|r| r.trim_start()).and_then(|r| r.strip_prefix('=')) {
                url = Some(v.trim().to_string());
            }
        }
    }
    let head = std::fs::read_to_string(git.join("HEAD")).ok()?;
    let sha = match head.trim().strip_prefix("ref: ") {
        Some(r) => std::fs::read_to_string(git.join(r)).ok().map(|s| s.trim().to_string()).or_else(|| {
            std::fs::read_to_string(git.join("packed-refs")).ok()?.lines().find_map(|l| l.strip_suffix(r).map(|s| s.trim().to_string()))
        })?,
        None => head.trim().to_string(),
    };
    Some((url?, sha))
}

/// Local scan merged with remote commit info from the last update check
/// (`remote`: url key → sha).
pub fn scan(wow_dir: &Path, tracked: &[String], remote: &HashMap<String, String>) -> std::io::Result<Vec<AddonEntry>> {
    let addons_dir = wow_dir.join("Interface").join("AddOns");
    let mut out = Vec::new();
    let mut found_keys = HashSet::new();

    if addons_dir.is_dir() {
        for entry in std::fs::read_dir(&addons_dir)? {
            let entry = entry?;
            if !entry.file_type()?.is_dir() {
                continue;
            }
            let folder = entry.file_name().to_string_lossy().into_owned();
            if folder.starts_with("Blizzard_") || folder.starts_with("Turtle_") || folder.starts_with('.') {
                continue;
            }
            let path = entry.path();
            let meta: OctoMeta = std::fs::read(path.join(".octowow_meta")).ok().and_then(|raw| serde_json::from_slice(&raw).ok()).unwrap_or_default();
            let is_git = path.join(".git").is_dir();
            let git = if is_git && meta.url.is_none() { read_git_checkout(&path) } else { None };
            let url = meta
                .url
                .or_else(|| git.as_ref().map(|g| g.0.clone()))
                .or_else(|| tracked.iter().find(|u| parse_repo(u).is_ok_and(|r| r.name.eq_ignore_ascii_case(&folder))).cloned());
            let local_sha = meta.sha.or_else(|| git.map(|g| g.1));
            let remote_sha = url.as_ref().and_then(|u| remote.get(&url_key(u))).cloned();
            if let Some(u) = &url {
                found_keys.insert(url_key(u));
            }
            let toc = read_toc(&path, &folder);
            out.push(AddonEntry {
                title: if toc.title.is_empty() { folder.clone() } else { toc.title },
                version: toc.version,
                author: toc.author,
                notes: toc.notes,
                managed: url.is_some(),
                update_available: matches!((&local_sha, &remote_sha), (Some(l), Some(r)) if l != r) || (url.is_some() && local_sha.is_none() && remote_sha.is_some()),
                url,
                local_sha,
                remote_sha,
                missing: false,
                is_git,
                folder,
            });
        }
    }

    for url in tracked.iter().filter(|u| !found_keys.contains(&url_key(u))) {
        let folder = parse_repo(url).map(|r| r.name).unwrap_or_else(|_| url.clone());
        out.push(AddonEntry {
            title: folder.clone(),
            folder,
            version: String::new(),
            author: String::new(),
            notes: "Managed by OctoWoW, but the files are missing from your drive.".into(),
            remote_sha: remote.get(&url_key(url)).cloned(),
            url: Some(url.clone()),
            local_sha: None,
            update_available: true,
            managed: true,
            missing: true,
            is_git: false,
        });
    }

    out.sort_by_key(|a| a.title.to_ascii_lowercase());
    Ok(out)
}

// ---------- catalog ----------

#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CatalogEntry {
    pub name: String,
    pub owner: String,
    pub git: String,
    pub title: String,
    pub description: String,
    pub version: String,
    pub author: String,
    pub last_updated: Option<String>,
    pub stars: Option<u32>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct RawCatalogEntry {
    name: String,
    #[serde(default)]
    owner: String,
    git: String,
    #[serde(default)]
    branch: Option<String>,
    #[serde(default)]
    toc: HashMap<String, String>,
    #[serde(default)]
    description: Option<String>,
    #[serde(default)]
    last_updated: Option<String>,
    #[serde(default)]
    stars: Option<u32>,
}

/// The curated OctoWoW addon list (the same one the OctoLauncher shows).
pub async fn fetch_catalog() -> Result<Vec<CatalogEntry>, String> {
    let raw: Vec<RawCatalogEntry> = serde_json::from_slice(&get_bytes(CATALOG_URL).await?).map_err(|e| format!("Unexpected catalog format: {e}"))?;
    let mut list: Vec<CatalogEntry> = raw
        .into_iter()
        .filter(|r| parse_repo(&r.git).is_ok())
        .map(|r| {
            // Keep the branch in the URL so installs track the same branch the catalog does.
            let git = match &r.branch {
                Some(b) if r.git.contains("github.com") => format!("{}/tree/{b}", r.git.trim_end_matches(".git")),
                _ => r.git.clone(),
            };
            let toc = |k: &str| r.toc.get(k).map(|v| strip_colors(v)).unwrap_or_default();
            CatalogEntry {
                title: Some(toc("Title")).filter(|t| !t.is_empty()).unwrap_or_else(|| r.name.clone()),
                description: r.description.clone().filter(|d| !d.is_empty()).unwrap_or_else(|| toc("Notes")),
                version: toc("Version"),
                author: toc("Author"),
                name: r.name,
                owner: r.owner,
                git,
                last_updated: r.last_updated,
                stars: r.stars,
            }
        })
        .collect();
    list.sort_by_key(|c| c.title.to_ascii_lowercase());
    Ok(list)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn strips_color_codes() {
        assert_eq!(strip_colors(" |cff33ffccpf|rUI "), "pfUI");
    }

    #[test]
    fn parses_repo_urls() {
        let r = parse_repo("https://github.com/shagu/pfUI.git").unwrap();
        assert_eq!((r.host, r.owner.as_str(), r.name.as_str(), r.web.as_str()), (Host::GitHub, "shagu", "pfUI", "https://github.com/shagu/pfUI"));
        assert_eq!(parse_repo("https://github.com/shagu/pfUI/tree/dev/").unwrap().branch.as_deref(), Some("dev"));
        let g = parse_repo("https://git.octowow.st/git/shaga/AtlasLoot.git").unwrap();
        assert_eq!((g.host, g.web.as_str()), (Host::Gitea, "https://git.octowow.st/git/shaga/AtlasLoot"));
        assert_eq!(g.archive_url("abc"), "https://git.octowow.st/git/shaga/AtlasLoot/archive/abc.zip");
        let c = parse_repo("https://codeberg.org/konaka/UnitXP_SP3_Addon/src/branch/main").unwrap();
        assert_eq!((c.name.as_str(), c.branch.as_deref()), ("UnitXP_SP3_Addon", Some("main")));
        let l = parse_repo("https://gitlab.com/grp/sub/Addon/-/tree/main").unwrap();
        assert_eq!((l.host, l.owner.as_str(), l.branch.as_deref()), (Host::GitLab, "sub", Some("main")));
        assert!(parse_repo("http://github.com/a/b").is_err());
        assert!(parse_repo("https://github.com/onlyowner").is_err());
        assert_eq!(url_key("https://github.com/Shagu/pfUI/"), url_key("https://github.com/shagu/pfui.git"));
        // octowow.st/git is behind a bot check for git; git.octowow.st serves the same repos.
        let o = parse_repo("https://octowow.st/git/Dusk/LevelRange-Octo").unwrap();
        assert_eq!(o.git_url(), "https://git.octowow.st/git/Dusk/LevelRange-Octo.git");
        assert_eq!(o.archive_url("main"), "https://octowow.st/git/Dusk/LevelRange-Octo/archive/main.zip");
        assert_eq!(url_key("https://git.octowow.st/git/Dusk/LevelRange-Octo.git"), url_key("https://octowow.st/git/dusk/levelrange-octo/"));
    }

    #[test]
    fn parses_ref_advertisement() {
        let pkt = |s: &str| format!("{:04x}{s}", s.len() + 4);
        let sha = "d057918a1dc3b04169da533b5e14f43604b18309";
        let body = format!(
            "{}0000{}{}0000",
            pkt("# service=git-upload-pack\n"),
            pkt(&format!("{sha} HEAD\0multi_ack symref=HEAD:refs/heads/main agent=x\n")),
            pkt(&format!("{sha} refs/heads/main\n"))
        );
        let (refs, head) = parse_ref_advertisement(body.as_bytes()).unwrap();
        assert_eq!(head.as_deref(), Some("main"));
        assert_eq!(refs["refs/heads/main"], "d057918a1dc3b04169da533b5e14f43604b18309");
        assert!(parse_ref_advertisement(b"<html>blocked</html>").is_none());
    }

    #[test]
    fn finds_addons_in_archives() {
        let root = std::env::temp_dir().join(format!("octo-addon-find-{}", std::process::id()));
        let _ = remove_dir_force(&root);
        // Single addon wrapped in a top folder, with flavoured TOCs alongside.
        std::fs::create_dir_all(root.join("a/pfUI-abc/libs/Lib")).unwrap();
        for f in ["pfUI.toc", "pfUI-tbc.toc", "pfUI-wotlk.toc"] {
            std::fs::write(root.join("a/pfUI-abc").join(f), "").unwrap();
        }
        std::fs::write(root.join("a/pfUI-abc/libs/Lib/Lib.toc"), "").unwrap();
        let found = find_addon_dirs(&root.join("a"));
        assert_eq!(found.len(), 1);
        assert_eq!(found[0].1, "pfUI");
        // Repo bundling several addons one level down.
        for d in ["b/repo-sha/Atlas", "b/repo-sha/AtlasLoot"] {
            std::fs::create_dir_all(root.join(d)).unwrap();
        }
        std::fs::write(root.join("b/repo-sha/Atlas/Atlas.toc"), "").unwrap();
        std::fs::write(root.join("b/repo-sha/AtlasLoot/AtlasLoot.toc"), "").unwrap();
        std::fs::write(root.join("b/repo-sha/README.md"), "").unwrap();
        let names: Vec<String> = find_addon_dirs(&root.join("b")).into_iter().map(|f| f.1).collect();
        assert_eq!(names, vec!["Atlas", "AtlasLoot"]);
        remove_dir_force(&root).unwrap();
    }

    #[tokio::test]
    #[ignore]
    async fn live_install_update_remove() {
        let root = std::env::temp_dir().join(format!("octo-addon-live-{}", std::process::id()));
        let _ = remove_dir_force(&root);
        let addons = root.join("Interface/AddOns");
        for url in ["https://github.com/shagu/ShaguTweaks", "https://git.octowow.st/git/shaga/NampowerSettings.git", "https://octowow.st/git/Dusk/LevelRange-Octo"] {
            let (folders, sha) = install_from_url(&addons, url).await.unwrap();
            println!("{url} -> {folders:?} @ {sha}");
            assert!(!folders.is_empty());
        }
        let list = scan(&root, &[], &HashMap::new()).unwrap();
        assert!(list.iter().all(|a| a.managed && a.local_sha.is_some()));
        let cat = fetch_catalog().await.unwrap();
        println!("catalog: {} addons", cat.len());
        assert!(cat.len() > 50);
        remove(&addons, &list[0].folder).unwrap();
        remove_dir_force(&root).unwrap();
    }
}
