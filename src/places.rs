//! Where the user stopped listening, so the Play window carries on from
//! there next time (log entry 3, idea 1): `%APPDATA%\VidFetch\places.json`.
//! A download is known by its file, a video online by its YouTube id.
//! Playing to the end, or stopping in the first or last few seconds, forgets
//! the place.

use std::path::{Path, PathBuf};

use crate::config::Config;

/// Places are kept for this many things, the most recent.
const KEPT: usize = 300;
/// Closer than this to the start or the end, there is nothing to remember.
const EDGE: f64 = 10.0;

fn path() -> Option<PathBuf> {
    Some(Config::dir()?.join("places.json"))
}

/// The key for a file, or for a YouTube video by its id.
pub fn of_file(file: &Path) -> String {
    file.display().to_string()
}

pub fn of_video(id: &str) -> String {
    format!("youtube:{id}")
}

/// Where `key` was left, in seconds.
pub fn get(key: &str) -> Option<f64> {
    read(&path()?)
        .into_iter()
        .find(|(k, _)| k == key)
        .map(|(_, at)| at)
}

/// Remembers `at` for `key`, of `length` seconds (0 when not known); forgets
/// it near either end.
pub fn set(key: &str, at: f64, length: f64) {
    if let Some(p) = path() {
        put(&p, key, at, length);
    }
}

/// A file or folder was moved or renamed: the places of what is in it go
/// with it.
pub fn moved(from: &Path, to: &Path) {
    let Some(p) = path() else { return };
    let mut all = read(&p);
    let mut changed = false;
    for e in all.iter_mut() {
        if let Ok(rest) = Path::new(&e.0).strip_prefix(from) {
            e.0 = of_file(&if rest.as_os_str().is_empty() {
                to.to_path_buf()
            } else {
                to.join(rest)
            });
            changed = true;
        }
    }
    if changed {
        write(&p, &all);
    }
    crate::history::moved(from, to);
}

fn put(p: &Path, key: &str, at: f64, length: f64) {
    let mut all = read(p);
    all.retain(|(k, _)| k != key);
    let near_end = length > 0.0 && at > length - EDGE;
    if at > EDGE && !near_end {
        // Most recent first.
        all.insert(0, (key.to_string(), (at * 10.0).round() / 10.0));
        all.truncate(KEPT);
    }
    write(p, &all);
}

fn read(p: &Path) -> Vec<(String, f64)> {
    let text = std::fs::read_to_string(p).unwrap_or_default();
    let v: serde_json::Value = serde_json::from_str(&text).unwrap_or_default();
    v.as_array()
        .into_iter()
        .flatten()
        .filter_map(|e| Some((e["key"].as_str()?.to_string(), e["at"].as_f64()?)))
        .collect()
}

fn write(p: &Path, all: &[(String, f64)]) {
    let v: Vec<serde_json::Value> = all
        .iter()
        .map(|(k, at)| serde_json::json!({ "key": k, "at": at }))
        .collect();
    if let Some(dir) = p.parent() {
        let _ = std::fs::create_dir_all(dir);
    }
    let part = p.with_extension("part");
    if std::fs::write(&part, serde_json::to_string_pretty(&v).unwrap_or_default()).is_ok() {
        let _ = std::fs::rename(&part, p);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn places_are_kept_but_not_near_either_end() {
        let d = std::env::temp_dir().join(format!("vf-places-{}", std::process::id()));
        let p = d.join("places.json");
        let find = |k: &str| read(&p).into_iter().find(|(x, _)| x == k).map(|(_, a)| a);
        put(&p, "a", 125.34, 600.0);
        assert_eq!(find("a"), Some(125.3));
        put(&p, "a", 595.0, 600.0);
        assert_eq!(find("a"), None, "the end forgets it");
        put(&p, "b", 5.0, 0.0);
        assert_eq!(find("b"), None, "the start is not worth keeping");
        put(&p, "c", 50.0, 0.0);
        put(&p, "d", 60.0, 0.0);
        assert_eq!(read(&p)[0].0, "d");
        let _ = std::fs::remove_dir_all(&d);
    }
}
