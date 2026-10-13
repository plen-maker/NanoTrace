//! Talks to GitHub (latest commit + commit count of the vault's `memory`
//! branch) and copies `claude/<name>/` into every matching local Claude Code
//! project memory dir. Same rules as `receive.sh`: a local file that differs
//! from the vault copy and is newer than it is kept, not overwritten.

use std::collections::HashMap;
use std::fs;
use std::path::{Component, Path, PathBuf};
use std::process::Command;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use anyhow::{anyhow, bail, Context, Result};
use serde::{Deserialize, Serialize};

const REPO: &str = "plen-maker/nanotrace-vault";
const BRANCH: &str = "memory";

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct Commit {
    pub sha: String,
    pub version: u32,
    pub message: String,
    pub date: String,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct State {
    pub synced: Option<Commit>,
    pub synced_at: Option<u64>,
    pub last_error: Option<String>,
}

#[derive(Clone, Debug, Default)]
pub struct Report {
    pub projects: Vec<String>,
    pub updated: Vec<String>,
    pub kept: Vec<String>,
}

pub fn home() -> PathBuf {
    std::env::var_os("HOME").map(PathBuf::from).unwrap_or_default()
}

fn state_path() -> PathBuf {
    home().join(".claude/nanotrace/state.json")
}

pub fn now_secs() -> u64 {
    SystemTime::now().duration_since(UNIX_EPOCH).map(|d| d.as_secs()).unwrap_or(0)
}

pub fn load_state() -> State {
    fs::read(state_path())
        .ok()
        .and_then(|b| serde_json::from_slice(&b).ok())
        .unwrap_or_default()
}

fn save_state(s: &State) -> Result<()> {
    let p = state_path();
    fs::create_dir_all(p.parent().unwrap())?;
    fs::write(&p, serde_json::to_vec_pretty(s)?)?;
    Ok(())
}

/// GITHUB_TOKEN, else `gh auth token` (also looked up outside PATH, since a
/// login item / LaunchAgent starts with a minimal PATH).
pub fn token() -> Result<String> {
    if let Ok(t) = std::env::var("GITHUB_TOKEN") {
        if !t.trim().is_empty() {
            return Ok(t.trim().to_string());
        }
    }
    let h = home();
    let candidates = [
        PathBuf::from("gh"),
        h.join(".local/bin/gh"),
        PathBuf::from("/opt/homebrew/bin/gh"),
        PathBuf::from("/opt/local/bin/gh"),
        PathBuf::from("/usr/local/bin/gh"),
        PathBuf::from("/usr/bin/gh"),
    ];
    for gh in candidates {
        if let Ok(out) = Command::new(&gh).args(["auth", "token"]).output() {
            let t = String::from_utf8_lossy(&out.stdout).trim().to_string();
            if out.status.success() && !t.is_empty() {
                return Ok(t);
            }
        }
    }
    bail!("nincs GitHub token - futtasd a `gh auth login`-t, vagy állítsd be a GITHUB_TOKEN-t")
}

fn get(url: &str, token: &str) -> Result<ureq::Response> {
    ureq::get(url)
        .set("Authorization", &format!("Bearer {token}"))
        .set("User-Agent", "nanotrace")
        .set("Accept", "application/vnd.github+json")
        .timeout(Duration::from_secs(30))
        .call()
        .map_err(|e| anyhow!("{url}: {e}"))
}

/// Number from `page=N` in the `rel="last"` entry of a GitHub Link header.
fn last_page(link: &str) -> Option<u32> {
    let part = link.split(',').find(|p| p.contains("rel=\"last\""))?;
    let url = part.split('<').nth(1)?.split('>').next()?;
    let query = url.split('?').nth(1)?;
    query.split('&').find_map(|kv| kv.strip_prefix("page=")?.parse().ok())
}

pub fn latest(token: &str) -> Result<Commit> {
    let v: serde_json::Value =
        get(&format!("https://api.github.com/repos/{REPO}/commits/{BRANCH}"), token)?.into_json()?;
    let sha = v["sha"].as_str().context("no sha in GitHub response")?.to_string();
    let message = v["commit"]["message"].as_str().unwrap_or("").to_string();
    let date = v["commit"]["committer"]["date"].as_str().unwrap_or("").to_string();
    // The branch's commit count doubles as a human-friendly version number.
    let resp = get(
        &format!("https://api.github.com/repos/{REPO}/commits?sha={BRANCH}&per_page=1"),
        token,
    )?;
    let version = resp.header("link").and_then(last_page).unwrap_or(1);
    Ok(Commit { sha, version, message, date })
}

fn sanitize(s: &str) -> String {
    s.chars().map(|c| if c.is_ascii_alphanumeric() { c } else { '-' }).collect()
}

fn download(token: &str, sha: &str, dest: &Path) -> Result<()> {
    let resp = get(&format!("https://codeload.github.com/{REPO}/tar.gz/{sha}"), token)?;
    let mut ar = tar::Archive::new(flate2::read::GzDecoder::new(resp.into_reader()));
    ar.set_preserve_mtime(true);
    for entry in ar.entries()? {
        let mut e = entry?;
        let path = e.path()?.into_owned();
        if path.components().any(|c| matches!(c, Component::ParentDir | Component::RootDir)) {
            continue;
        }
        let rel: PathBuf = path.components().skip(1).collect();
        if rel.as_os_str().is_empty() {
            continue;
        }
        let out = dest.join(&rel);
        if let Some(p) = out.parent() {
            fs::create_dir_all(p)?;
        }
        e.unpack(&out)?;
    }
    Ok(())
}

fn mtime(p: &Path) -> Option<SystemTime> {
    fs::metadata(p).and_then(|m| m.modified()).ok()
}

fn apply(src: &Path, dst: &Path, key: &str, report: &mut Report) -> Result<()> {
    let mut files = vec![PathBuf::from("MEMORY.md")];
    if let Ok(rd) = fs::read_dir(src.join("memory")) {
        for e in rd.flatten() {
            if e.path().extension().is_some_and(|x| x == "md") {
                files.push(PathBuf::from("memory").join(e.file_name()));
            }
        }
    }
    fs::create_dir_all(dst.join("memory"))?;
    for rel in files {
        let (from, to) = (src.join(&rel), dst.join(&rel));
        if !from.is_file() {
            continue;
        }
        if to.is_file() {
            if fs::read(&from)? == fs::read(&to)? {
                continue;
            }
            if let (Some(a), Some(b)) = (mtime(&to), mtime(&from)) {
                if a > b {
                    report.kept.push(format!("{key}/{}", rel.display()));
                    continue;
                }
            }
        }
        fs::copy(&from, &to)?;
        if let Some(t) = mtime(&from) {
            filetime::set_file_mtime(&to, filetime::FileTime::from_system_time(t))?;
        }
        report.updated.push(format!("{key}/{}", rel.display()));
    }
    Ok(())
}

/// Downloads `commit` and applies every `claude/<name>/` to each local
/// project dir whose key ends in `-<name>` (or `-<alias>` per `aliases`).
pub fn sync_to(token: &str, commit: &Commit) -> Result<Report> {
    let tmp = std::env::temp_dir().join(format!("nanotrace-{}", std::process::id()));
    let _ = fs::remove_dir_all(&tmp);
    fs::create_dir_all(&tmp)?;
    let result = (|| {
        download(token, &commit.sha, &tmp)?;

        let mut local_names: HashMap<String, Vec<String>> = HashMap::new();
        if let Ok(a) = fs::read_to_string(tmp.join("aliases")) {
            for line in a.lines().filter(|l| !l.trim_start().starts_with('#')) {
                let mut it = line.split_whitespace();
                if let (Some(local), Some(vault)) = (it.next(), it.next()) {
                    local_names.entry(vault.to_string()).or_default().push(local.to_string());
                }
            }
        }

        let projects_dir = home().join(".claude/projects");
        let keys: Vec<String> = fs::read_dir(&projects_dir)
            .map(|rd| rd.flatten().filter(|e| e.path().is_dir()).filter_map(|e| e.file_name().into_string().ok()).collect())
            .unwrap_or_default();

        let mut report = Report::default();
        for e in fs::read_dir(tmp.join("claude"))?.flatten() {
            let Ok(name) = e.file_name().into_string() else { continue };
            let mut suffixes = vec![format!("-{}", sanitize(&name))];
            for l in local_names.get(&name).into_iter().flatten() {
                suffixes.push(format!("-{}", sanitize(l)));
            }
            for key in keys.iter().filter(|k| suffixes.iter().any(|s| k.ends_with(s.as_str()))) {
                apply(&e.path(), &projects_dir.join(key), key, &mut report)?;
                report.projects.push(format!("{name} → {key}"));
            }
        }
        Ok(report)
    })();
    let _ = fs::remove_dir_all(&tmp);
    result
}

/// Checks GitHub and syncs. `force` re-applies even when already on the
/// latest commit (the manual "Frissítés" button). Records the outcome in
/// the state file either way.
pub fn run(force: bool) -> Result<(Commit, Option<Report>)> {
    let mut state = load_state();
    let result = (|| {
        let token = token()?;
        let latest = latest(&token)?;
        if !force && state.synced.as_ref().is_some_and(|c| c.sha == latest.sha) {
            return Ok((latest, None));
        }
        let report = sync_to(&token, &latest)?;
        Ok((latest, Some(report)))
    })();
    match &result {
        Ok((c, Some(_))) => {
            state.synced = Some(c.clone());
            state.synced_at = Some(now_secs());
            state.last_error = None;
        }
        Ok((_, None)) => state.last_error = None,
        Err(e) => state.last_error = Some(format!("{e:#}")),
    }
    save_state(&state)?;
    result
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_last_page_from_link_header() {
        let link = r#"<https://api.github.com/repositories/1/commits?sha=memory&per_page=1&page=2>; rel="next", <https://api.github.com/repositories/1/commits?sha=memory&per_page=1&page=14>; rel="last""#;
        assert_eq!(last_page(link), Some(14));
        assert_eq!(last_page(r#"<https://x/y?page=3>; rel="prev""#), None);
    }

    #[test]
    fn sanitizes_like_claude_code() {
        assert_eq!(sanitize("/home/ddnemet/benchpeek"), "-home-ddnemet-benchpeek");
        assert_eq!(sanitize("/Users/a.b/Log Peaker"), "-Users-a-b-Log-Peaker");
    }

    #[test]
    fn keeps_newer_local_edits_and_updates_stale_files() {
        let root = std::env::temp_dir().join(format!("nanotrace-test-{}", std::process::id()));
        let (src, dst) = (root.join("src"), root.join("dst"));
        fs::create_dir_all(src.join("memory")).unwrap();
        fs::create_dir_all(dst.join("memory")).unwrap();
        let old = filetime::FileTime::from_unix_time(1_000, 0);
        let new = filetime::FileTime::from_unix_time(2_000, 0);
        fs::write(src.join("MEMORY.md"), "vault index").unwrap();
        filetime::set_file_mtime(src.join("MEMORY.md"), new).unwrap();
        fs::write(dst.join("MEMORY.md"), "old index").unwrap();
        filetime::set_file_mtime(dst.join("MEMORY.md"), old).unwrap();
        fs::write(src.join("memory/a.md"), "vault a").unwrap();
        filetime::set_file_mtime(src.join("memory/a.md"), old).unwrap();
        fs::write(dst.join("memory/a.md"), "edited locally").unwrap();
        filetime::set_file_mtime(dst.join("memory/a.md"), new).unwrap();
        fs::write(src.join("memory/b.md"), "new file").unwrap();

        let mut r = Report::default();
        apply(&src, &dst, "k", &mut r).unwrap();
        assert_eq!(fs::read_to_string(dst.join("MEMORY.md")).unwrap(), "vault index");
        assert_eq!(fs::read_to_string(dst.join("memory/a.md")).unwrap(), "edited locally");
        assert_eq!(fs::read_to_string(dst.join("memory/b.md")).unwrap(), "new file");
        assert_eq!(r.kept, vec!["k/memory/a.md".to_string()]);
        assert_eq!(r.updated.len(), 2);
        fs::remove_dir_all(&root).unwrap();
    }
}
