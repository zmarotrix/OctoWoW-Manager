//! "Is there a newer manager?" via the GitHub releases of this project (as v2.6 did).

use super::util::http;
use serde::Serialize;

const RELEASES_API: &str = "https://api.github.com/repos/zmarotrix/OctoWoW-Manager/releases/latest";

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AppUpdate {
    pub current: String,
    pub latest: String,
    pub url: String,
    pub newer: bool,
}

fn parse_version(v: &str) -> Vec<u64> {
    v.trim().trim_start_matches(['v', 'V']).split(['.', '-', '+']).map_while(|p| p.parse().ok()).collect()
}

pub fn is_newer(latest: &str, current: &str) -> bool {
    let (mut a, mut b) = (parse_version(latest), parse_version(current));
    let n = a.len().max(b.len());
    a.resize(n, 0);
    b.resize(n, 0);
    !a.is_empty() && a > b
}

pub async fn check() -> Result<AppUpdate, String> {
    let res = http().get(RELEASES_API).header("Accept", "application/vnd.github+json").send().await.map_err(|e| e.to_string())?;
    if !res.status().is_success() {
        return Err(format!("GitHub answered HTTP {}", res.status()));
    }
    let v: serde_json::Value = res.json().await.map_err(|e| e.to_string())?;
    let latest = v["tag_name"].as_str().unwrap_or_default().to_string();
    let current = env!("CARGO_PKG_VERSION").to_string();
    Ok(AppUpdate { newer: is_newer(&latest, &current), url: v["html_url"].as_str().unwrap_or_default().to_string(), latest, current })
}

#[cfg(test)]
mod tests {
    #[test]
    fn version_compare() {
        assert!(super::is_newer("v3.1.0", "3.0.0"));
        assert!(super::is_newer("3.0.1", "3.0"));
        assert!(!super::is_newer("2.6.0", "3.0.0"));
        assert!(!super::is_newer("3.0.0", "3.0.0"));
        assert!(!super::is_newer("", "3.0.0"));
    }
}
