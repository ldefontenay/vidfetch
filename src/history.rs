//! What the user played lately, downloads and videos online alike (log
//! entry 3, idea 9): `%APPDATA%\VidFetch\history.json`, newest first. Each is
//! known by the same key as its place (places.rs), so the list can say where
//! the user stopped, and Enter carries on from there.

use std::path::PathBuf;

use crate::config::Config;

/// How many are kept.
const KEPT: usize = 100;

#[derive(Clone, Debug, PartialEq)]
pub struct Played {
    /// A file's path, or "youtube:" and the video's id.
    pub key: String,
    pub name: String,
    /// Seconds since 1970.
    pub when: i64,
}

impl Played {
    /// The YouTube video, for one played online.
    pub fn video(&self) -> Option<&str> {
        self.key.strip_prefix("youtube:")
    }
}

fn path() -> Option<PathBuf> {
    Some(Config::dir()?.join("history.json"))
}

pub fn load() -> Vec<Played> {
    let Some(p) = path() else { return Vec::new() };
    let text = std::fs::read_to_string(p).unwrap_or_default();
    let v: serde_json::Value = serde_json::from_str(&text).unwrap_or_default();
    v.as_array()
        .into_iter()
        .flatten()
        .filter_map(|e| {
            Some(Played {
                key: e["key"].as_str()?.to_string(),
                name: e["name"].as_str().unwrap_or_default().to_string(),
                when: e["when"].as_i64().unwrap_or(0),
            })
        })
        .collect()
}

fn save(all: &[Played]) {
    let Some(p) = path() else { return };
    let v: Vec<serde_json::Value> = all
        .iter()
        .map(|e| serde_json::json!({ "key": e.key, "name": e.name, "when": e.when }))
        .collect();
    if let Some(dir) = p.parent() {
        let _ = std::fs::create_dir_all(dir);
    }
    let part = p.with_extension("part");
    if std::fs::write(&part, serde_json::to_string_pretty(&v).unwrap_or_default()).is_ok() {
        let _ = std::fs::rename(&part, &p);
    }
}

/// Puts `key`, called `name`, first in the list.
pub fn record(key: &str, name: &str) {
    let mut all = load();
    all.retain(|e| e.key != key);
    all.insert(
        0,
        Played {
            key: key.to_string(),
            name: name.to_string(),
            when: crate::youtube::now(),
        },
    );
    all.truncate(KEPT);
    save(&all);
}

pub fn remove(key: &str) {
    let mut all = load();
    all.retain(|e| e.key != key);
    save(&all);
}

/// A file or folder was moved or renamed: its entries follow.
pub fn moved(from: &std::path::Path, to: &std::path::Path) {
    let mut all = load();
    let mut changed = false;
    for e in all.iter_mut() {
        if let Ok(rest) = std::path::Path::new(&e.key).strip_prefix(from) {
            let to = if rest.as_os_str().is_empty() {
                to.to_path_buf()
            } else {
                to.join(rest)
            };
            e.key = to.display().to_string();
            changed = true;
        }
    }
    if changed {
        save(&all);
    }
}
