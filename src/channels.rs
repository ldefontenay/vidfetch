//! The YouTube channels the user follows, kept in VidFetch, without an
//! account (log entry 3, plan 2): `%APPDATA%\VidFetch\channels.json`. What is
//! new is what came out since the user last looked, with Ctrl+L, at each
//! channel; a channel just followed starts from then. YouTube says only "5
//! days ago" now, so the videos seen at the last look are kept, and new is
//! any other.

use std::path::{Path, PathBuf};

use crate::config::Config;

#[derive(Clone, Debug, PartialEq)]
pub struct Channel {
    /// "UC...".
    pub id: String,
    pub name: String,
    /// When the user last looked at what is new, in seconds since 1970.
    pub looked: i64,
    /// The videos listed when the user last looked.
    pub seen: Vec<String>,
}

/// How many seen videos are kept for each channel: more than its Videos tab
/// lists.
const SEEN: usize = 60;

impl Channel {
    pub fn url(&self) -> String {
        format!("https://www.youtube.com/channel/{}", self.id)
    }

    /// Its videos as yt-dlp lists them, to download as a playlist.
    pub fn videos_url(&self) -> String {
        format!("https://www.youtube.com/channel/{}/videos", self.id)
    }

    /// A channel known by its id and name, not followed.
    pub fn named(id: &str, name: &str) -> Channel {
        Channel {
            id: id.to_string(),
            name: name.to_string(),
            looked: 0,
            seen: Vec::new(),
        }
    }

    /// Whether a video, out `published` (about), is new since the last look.
    pub fn is_new(&self, id: &str, published: i64) -> bool {
        if self.seen.is_empty() {
            published > self.looked
        } else {
            !self.seen.iter().any(|s| s == id)
        }
    }

    /// Looked at now, having listed `ids`.
    pub fn looked_at(&mut self, now: i64, ids: impl IntoIterator<Item = String>) {
        self.looked = now;
        let mut seen: Vec<String> = ids.into_iter().collect();
        seen.extend(
            self.seen
                .drain(..)
                .filter(|s| !seen.contains(s))
                .collect::<Vec<_>>(),
        );
        seen.truncate(SEEN);
        self.seen = seen;
    }
}

fn path() -> Option<PathBuf> {
    Some(Config::dir()?.join("channels.json"))
}

/// The channels followed, in the order they were followed.
pub fn load() -> Vec<Channel> {
    path().map_or_else(Vec::new, |p| read(&p))
}

pub fn save(channels: &[Channel]) {
    if let Some(p) = path() {
        write(&p, channels);
    }
}

fn read(path: &Path) -> Vec<Channel> {
    let text = std::fs::read_to_string(path).unwrap_or_default();
    let v: serde_json::Value =
        serde_json::from_str(text.trim_start_matches('\u{feff}')).unwrap_or_default();
    v["channels"]
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(|c| {
            Some(Channel {
                id: c["id"]
                    .as_str()
                    .filter(|id| id.starts_with("UC"))?
                    .to_string(),
                name: c["name"].as_str().unwrap_or_default().to_string(),
                looked: c["looked"].as_i64().unwrap_or(0),
                seen: c["seen"]
                    .as_array()
                    .into_iter()
                    .flatten()
                    .filter_map(|s| s.as_str().map(str::to_string))
                    .collect(),
            })
        })
        .collect()
}

fn write(path: &Path, channels: &[Channel]) {
    let v = serde_json::json!({
        "what": "The YouTube channels VidFetch follows for you. Remove one in VidFetch, with \
                 Delete in Your channels, or here while VidFetch is closed.",
        "channels": channels
            .iter()
            .map(|c| {
                serde_json::json!({ "id": c.id, "name": c.name, "looked": c.looked, "seen": c.seen })
            })
            .collect::<Vec<_>>(),
    });
    if let Some(dir) = path.parent() {
        let _ = std::fs::create_dir_all(dir);
    }
    let part = path.with_extension("part");
    if std::fs::write(&part, serde_json::to_string_pretty(&v).unwrap_or_default()).is_ok() {
        let _ = std::fs::rename(&part, path);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn channels_are_kept_and_read_back() {
        let d = std::env::temp_dir().join(format!("vf-channels-{}", std::process::id()));
        let p = d.join("channels.json");
        let list = vec![Channel {
            id: "UCz75RVbH8q2jdBJ4SnwuZZQ".into(),
            name: "Blender Studio".into(),
            looked: 1_800_000_000,
            seen: vec!["aaaaaaaaaaa".into()],
        }];
        write(&p, &list);
        assert_eq!(read(&p), list);
        assert!(!list[0].is_new("aaaaaaaaaaa", 1_900_000_000));
        assert!(list[0].is_new("bbbbbbbbbbb", 0));
        let mut c = Channel::named("UCx", "X");
        c.looked = 100;
        assert!(c.is_new("x", 200) && !c.is_new("x", 50));
        c.looked_at(300, ["b".to_string(), "a".to_string()]);
        assert!(!c.is_new("a", 999) && c.is_new("c", 0));
        assert_eq!(
            list[0].url(),
            "https://www.youtube.com/channel/UCz75RVbH8q2jdBJ4SnwuZZQ"
        );
        let _ = std::fs::remove_dir_all(&d);
    }
}
