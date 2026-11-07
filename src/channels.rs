//! The YouTube channels the user follows, kept in VidFetch, without an
//! account (log entry 3, plan 2): `%APPDATA%\VidFetch\channels.json`. What is
//! new is what came out since the user last looked, with Ctrl+L, at each
//! channel; a channel just followed starts from then.

use std::path::{Path, PathBuf};

use crate::config::Config;

#[derive(Clone, Debug, PartialEq)]
pub struct Channel {
    /// "UC...".
    pub id: String,
    pub name: String,
    /// When the user last looked at what is new, in seconds since 1970.
    pub looked: i64,
}

impl Channel {
    pub fn url(&self) -> String {
        format!("https://www.youtube.com/channel/{}", self.id)
    }

    /// Its videos as yt-dlp lists them, to download as a playlist.
    pub fn videos_url(&self) -> String {
        format!("https://www.youtube.com/channel/{}/videos", self.id)
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
            .map(|c| serde_json::json!({ "id": c.id, "name": c.name, "looked": c.looked }))
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
        }];
        write(&p, &list);
        assert_eq!(read(&p), list);
        assert_eq!(
            list[0].url(),
            "https://www.youtube.com/channel/UCz75RVbH8q2jdBJ4SnwuZZQ"
        );
        let _ = std::fs::remove_dir_all(&d);
    }
}
