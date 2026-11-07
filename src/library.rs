//! What has been downloaded, in the output folder. A single video is a file
//! there, named after its title. A playlist is a folder named after the
//! playlist, holding its videos and `VidFetch.json`, which says which
//! playlist it is and which of its videos are already there, so fetching it
//! again brings only the new ones.
//!
//! A playlist's folder is found by that file, never by its name, so a folder
//! renamed in File Explorer is still found. `clean` and `shorten` are
//! NoteFetch's (`naming.rs`, MIT, same author), by way of Video Scribe.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::time::SystemTime;

const MARKER: &str = "VidFetch.json";
const UNTITLED: &str = "Untitled";
/// Longest name made from a title: room is left for a number and for the
/// output folder itself, under Windows' usual 260.
const LONGEST: usize = 80;
/// Characters Windows forbids in a name.
const NOT_IN_A_NAME: &[char] = &['<', '>', ':', '"', '/', '\\', '|', '?', '*'];
/// Files the list shows: what VidFetch saves, and what a fallback download
/// may be.
const MEDIA: &[&str] = &[
    "mp4", "m4a", "mp3", "webm", "mkv", "mov", "m4v", "ogg", "opus",
];

/// Make text safe as a Windows file name. Characters Windows forbids are
/// replaced with readable ones, runs of spaces are collapsed, trailing dots
/// and spaces are removed, and a reserved device name gets a leading
/// underscore. Empty text becomes "Untitled".
pub fn clean(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    for c in text.chars() {
        let replacement = match c {
            ':' => Some('.'),
            '/' | '\\' | '|' => Some('-'),
            '"' => Some('\''),
            '<' | '>' | '?' | '*' => None,
            c if c.is_control() => Some(' '),
            c => Some(c),
        };
        if let Some(r) = replacement {
            if r.is_whitespace() {
                if !out.is_empty() && !out.ends_with(' ') {
                    out.push(' ');
                }
            } else {
                out.push(r);
            }
        }
    }
    let trimmed = out.trim_matches(' ').trim_end_matches(['.', ' ']);
    if trimmed.is_empty() {
        return UNTITLED.to_string();
    }
    if is_reserved(trimmed) {
        return format!("_{trimmed}");
    }
    trimmed.to_string()
}

fn is_reserved(name: &str) -> bool {
    let stem = name
        .split('.')
        .next()
        .unwrap_or(name)
        .trim_end()
        .to_ascii_uppercase();
    matches!(stem.as_str(), "CON" | "PRN" | "AUX" | "NUL")
        || ((stem.starts_with("COM") || stem.starts_with("LPT"))
            && stem.chars().count() == 4
            && stem.chars().nth(3).is_some_and(|c| c.is_ascii_digit()))
}

/// Cut text to at most `max` characters, at a word boundary where one is
/// near, without leaving trailing dots or spaces.
pub fn shorten(text: &str, max: usize) -> String {
    if text.chars().count() <= max {
        return text.to_string();
    }
    let cut: String = text.chars().take(max).collect();
    let at_word = match cut.rfind(' ') {
        Some(i) if cut[..i].chars().count() * 3 >= max * 2 => &cut[..i],
        _ => &cut[..],
    };
    let tidy = at_word.trim_end_matches(['.', ' ', ',', '-']);
    if tidy.is_empty() {
        cut
    } else {
        tidy.to_string()
    }
}

/// The name a title gives a file or a folder.
pub fn name_for(title: &str) -> String {
    shorten(&clean(title), LONGEST)
}

/// A file in `dir` named `stem.ext`, or `stem 2.ext` and so on when that is
/// taken.
pub fn free_name(dir: &Path, stem: &str, ext: &str) -> PathBuf {
    (1..)
        .map(|n| {
            if n == 1 {
                dir.join(format!("{stem}.{ext}"))
            } else {
                dir.join(format!("{stem} {n}.{ext}"))
            }
        })
        .find(|p| !p.exists())
        .expect("a free name")
}

/// Why a name typed for a download cannot be used, in words that say how to
/// write one that can; None when it is fine.
pub fn bad_name(name: &str) -> Option<String> {
    let name = name.trim();
    if name.is_empty() {
        return Some("Type a name, such as Sprite Fright.".into());
    }
    if name.chars().count() > LONGEST {
        return Some(format!("The name is too long: {LONGEST} letters at most."));
    }
    if let Some(c) = name
        .chars()
        .find(|c| NOT_IN_A_NAME.contains(c) || c.is_control())
    {
        return Some(format!(
            "A name cannot hold the character {c}, because it becomes a file or folder name. \
             Letters, numbers, spaces and most punctuation are fine."
        ));
    }
    if name.ends_with('.') {
        return Some("A name cannot end with a full stop.".into());
    }
    if is_reserved(name) {
        return Some(format!(
            "Windows keeps the name {name} for itself. Add a word to it."
        ));
    }
    None
}

#[derive(Clone, Debug, PartialEq)]
pub enum Kind {
    Video,
    Sound,
    /// A playlist's folder: how many videos are in it, and its address.
    Playlist {
        videos: usize,
        url: String,
    },
}

/// One thing in the list: a downloaded file, or a playlist's folder.
#[derive(Clone, Debug)]
pub struct Entry {
    pub path: PathBuf,
    /// The file's name without its ending, or the folder's name.
    pub name: String,
    pub kind: Kind,
    /// When something in it was last saved.
    pub made: SystemTime,
}

impl Entry {
    /// What it is, in words.
    pub fn holds(&self) -> String {
        let ext = self
            .path
            .extension()
            .map(|e| e.to_string_lossy().to_uppercase())
            .unwrap_or_default();
        match &self.kind {
            Kind::Video => format!("video, {ext}"),
            Kind::Sound => format!("sound, {ext}"),
            // Videos, not files: the folder also holds their captions
            // (milestone 2 test, 5.5).
            Kind::Playlist { videos: 1, .. } => "playlist, 1 video".into(),
            Kind::Playlist { videos, .. } => format!("playlist, {videos} videos"),
        }
    }

    pub fn is_playlist(&self) -> bool {
        matches!(self.kind, Kind::Playlist { .. })
    }
}

fn is_media(path: &Path) -> bool {
    path.extension()
        .map(|e| e.to_string_lossy().to_ascii_lowercase())
        .is_some_and(|e| MEDIA.contains(&e.as_str()))
}

fn modified(path: &Path) -> SystemTime {
    std::fs::metadata(path)
        .and_then(|m| m.modified())
        .unwrap_or(SystemTime::UNIX_EPOCH)
}

/// Everything downloaded into `output`, most recent first.
pub fn list(output: &Path) -> Vec<Entry> {
    let Ok(dir) = std::fs::read_dir(output) else {
        return Vec::new();
    };
    let mut out: Vec<Entry> = dir
        .flatten()
        .filter_map(|e| {
            let path = e.path();
            let kind = e.file_type().ok()?;
            if kind.is_dir() {
                let p = read_record(&path)?;
                let mut made = modified(&path);
                let mut videos = 0;
                for f in std::fs::read_dir(&path).into_iter().flatten().flatten() {
                    if is_media(&f.path()) {
                        videos += 1;
                        made = made.max(modified(&f.path()));
                    }
                }
                Some(Entry {
                    name: e.file_name().to_string_lossy().into_owned(),
                    path,
                    kind: Kind::Playlist { videos, url: p.url },
                    made,
                })
            } else if is_media(&path) {
                let ext = path
                    .extension()
                    .unwrap_or_default()
                    .to_string_lossy()
                    .to_ascii_lowercase();
                Some(Entry {
                    name: path
                        .file_stem()
                        .map(|n| n.to_string_lossy().into_owned())
                        .unwrap_or_default(),
                    made: modified(&path),
                    kind: if matches!(ext.as_str(), "m4a" | "mp3" | "ogg" | "opus") {
                        Kind::Sound
                    } else {
                        Kind::Video
                    },
                    path,
                })
            } else {
                None
            }
        })
        .collect();
    out.sort_by_key(|e| std::cmp::Reverse(e.made));
    out
}

/// The videos in a playlist's folder, in the playlist's order: by name, as
/// each starts with its place in it, when numbered.
pub fn videos_in(folder: &Path) -> Vec<Entry> {
    let mut out: Vec<Entry> = list(folder)
        .into_iter()
        .filter(|e| !e.is_playlist())
        .collect();
    out.sort_by_key(|e| e.name.to_lowercase());
    out
}

/// A playlist's folder's record of itself.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Record {
    pub url: String,
    /// The site's id for the playlist.
    pub id: String,
    pub title: String,
    /// Each video already there, by its id, with the file it was saved as.
    pub done: BTreeMap<String, String>,
}

pub fn read_record(folder: &Path) -> Option<Record> {
    let text = std::fs::read_to_string(folder.join(MARKER)).ok()?;
    let v: serde_json::Value = serde_json::from_str(&text).ok()?;
    let text = |k: &str| v[k].as_str().unwrap_or_default().to_string();
    Some(Record {
        url: text("url"),
        id: text("id"),
        title: text("title"),
        done: v["done"]
            .as_object()
            .map(|o| {
                o.iter()
                    .map(|(k, f)| (k.clone(), f.as_str().unwrap_or_default().to_string()))
                    .collect()
            })
            .unwrap_or_default(),
    })
}

/// One single download VidFetch remembers, to notice it being asked for again.
/// A playlist needs none of this: its folder has its own record.
#[derive(Clone, Debug, PartialEq)]
pub struct Past {
    /// `ytdlp::key` of the address, so every form of a YouTube address is one.
    pub key: String,
    pub file: PathBuf,
    /// `Format::word`.
    pub format: String,
    /// Lines of picture; 0 for sound, where they make no difference.
    pub lines: u32,
}

fn read_past(record: &Path) -> Vec<Past> {
    let text = std::fs::read_to_string(record).unwrap_or_default();
    let v: serde_json::Value = serde_json::from_str(&text).unwrap_or_default();
    v.as_array()
        .into_iter()
        .flatten()
        .filter_map(|p| {
            Some(Past {
                key: p["key"].as_str()?.to_string(),
                file: PathBuf::from(p["file"].as_str()?),
                format: p["format"].as_str()?.to_string(),
                lines: p["lines"].as_u64()? as u32,
            })
        })
        .collect()
}

/// Saved under a temporary name first; files gone since are left out. Failing
/// to save only means a later download is not noticed as a repeat.
fn write_past(record: &Path, past: &[Past]) {
    let v: Vec<serde_json::Value> = past
        .iter()
        .filter(|p| p.file.exists())
        .map(|p| {
            serde_json::json!({
                "key": p.key,
                "file": p.file.display().to_string(),
                "format": p.format,
                "lines": p.lines,
            })
        })
        .collect();
    if let Some(dir) = record.parent() {
        let _ = std::fs::create_dir_all(dir);
    }
    let part = record.with_extension("part");
    let text = serde_json::to_string_pretty(&v).unwrap_or_default();
    if std::fs::write(&part, text).is_ok() {
        let _ = std::fs::rename(&part, record);
    }
}

/// The file `key` was already saved as, at this format and quality, if it is
/// still there. Lines are 0 for sound.
pub fn already(record: &Path, key: &str, format: &str, lines: u32) -> Option<PathBuf> {
    read_past(record)
        .into_iter()
        .find(|p| p.key == key && p.format == format && p.lines == lines && p.file.exists())
        .map(|p| p.file)
}

/// Remembers a single download just saved. Lines are 0 for sound.
pub fn remember(record: &Path, key: &str, file: &Path, format: &str, lines: u32) {
    let mut past = read_past(record);
    past.retain(|p| !(p.key == key && p.format == format && p.lines == lines));
    past.push(Past {
        key: key.to_string(),
        file: file.to_path_buf(),
        format: format.to_string(),
        lines,
    });
    write_past(record, &past);
}

/// A remembered download was renamed.
pub fn moved(record: &Path, from: &Path, to: &Path) {
    let mut past = read_past(record);
    if !past.iter().any(|p| p.file == from) {
        return;
    }
    for p in past.iter_mut().filter(|p| p.file == from) {
        p.file = to.to_path_buf();
    }
    write_past(record, &past);
}

/// The captions beside a download: `Title.en.srt` for `Title.mp4`. None are
/// counted as its own while another download has the same name, such as
/// `Title.mp3`, as they serve that one too.
pub fn captions_of(file: &Path) -> Vec<PathBuf> {
    let (Some(stem), Some(dir)) = (file.file_stem(), file.parent()) else {
        return Vec::new();
    };
    let stem = stem.to_string_lossy().to_lowercase();
    let mut found = Vec::new();
    for e in std::fs::read_dir(dir).into_iter().flatten().flatten() {
        let path = e.path();
        if path != file
            && is_media(&path)
            && path
                .file_stem()
                .is_some_and(|s| s.to_string_lossy().to_lowercase() == stem)
        {
            return Vec::new();
        }
        let name = e.file_name().to_string_lossy().to_lowercase();
        let lang = name
            .strip_prefix(stem.as_str())
            .and_then(|r| r.strip_prefix('.'))
            .and_then(|r| r.strip_suffix(".srt"));
        if lang.is_some_and(|l| !l.is_empty() && l.chars().all(|c| c.is_alphanumeric() || c == '-'))
        {
            found.push(path);
        }
    }
    found
}

/// Saves the record, under a temporary name first, so it is never half
/// written.
pub fn write_record(folder: &Path, r: &Record) -> Result<(), String> {
    let v = serde_json::json!({
        "what": "VidFetch keeps this file to know which playlist this folder is for, and \
                 which of its videos are already here. Rename the folder as you like, but keep \
                 this file in it. Delete a line under done to download that video again.",
        "url": r.url,
        "id": r.id,
        "title": r.title,
        "done": r.done,
    });
    let part = folder.join(format!("{MARKER}.part"));
    let text = serde_json::to_string_pretty(&v).unwrap_or_default();
    std::fs::write(&part, text)
        .and_then(|_| std::fs::rename(&part, folder.join(MARKER)))
        .map_err(|e| format!("{}: {e}", folder.display()))
}

/// The folder for the playlist `id`: the one already made for it, whatever
/// it is called now, else a new one named after `title`.
pub fn playlist_folder(output: &Path, id: &str, url: &str, title: &str) -> Result<PathBuf, String> {
    if let Some(found) = std::fs::read_dir(output)
        .into_iter()
        .flatten()
        .flatten()
        .map(|e| e.path())
        .find(|p| read_record(p).is_some_and(|r| r.id == id && !id.is_empty()))
    {
        return Ok(found);
    }
    let name = name_for(title);
    let folder = (1..)
        .map(|n| {
            if n == 1 {
                output.join(&name)
            } else {
                output.join(format!("{name} {n}"))
            }
        })
        .find(|f| !f.exists())
        .expect("a free name");
    std::fs::create_dir_all(&folder).map_err(|e| {
        format!(
            "The folder {} could not be made: {e}. Choose another output folder in the File menu.",
            folder.display()
        )
    })?;
    write_record(
        &folder,
        &Record {
            url: url.into(),
            id: id.into(),
            title: title.into(),
            done: BTreeMap::new(),
        },
    )?;
    Ok(folder)
}

/// Renames a download: a file keeps its ending; a playlist's folder is
/// renamed as it is. The new path.
pub fn rename(e: &Entry, new: &str) -> Result<PathBuf, String> {
    let new = new.trim();
    if let Some(why) = bad_name(new) {
        return Err(why);
    }
    let parent = e.path.parent().ok_or("That has no folder.")?;
    let target = match e.path.extension().filter(|_| !e.is_playlist()) {
        Some(ext) => parent.join(format!("{new}.{}", ext.to_string_lossy())),
        None => parent.join(new),
    };
    if target == e.path {
        return Ok(target);
    }
    // A change of capitals alone is the same name to Windows.
    let same = e.name.eq_ignore_ascii_case(new);
    if target.exists() && !same {
        return Err(format!(
            "There is already something called {new} in the output folder. Choose another name."
        ));
    }
    let captions = if e.is_playlist() {
        Vec::new()
    } else {
        captions_of(&e.path)
    };
    std::fs::rename(&e.path, &target).map_err(|err| {
        if err.raw_os_error() == Some(32) || err.raw_os_error() == Some(5) {
            format!(
                "{} could not be renamed, because it is open in another program, such as a \
                 media player. Close it and try again.",
                e.name
            )
        } else {
            format!("{} could not be renamed: {err}", e.name)
        }
    })?;
    // The captions follow, keeping their language: Old.en.srt to New.en.srt.
    for c in captions {
        let name = c
            .file_name()
            .unwrap_or_default()
            .to_string_lossy()
            .into_owned();
        let lang = name.get(e.name.len()..).unwrap_or(".srt");
        let _ = std::fs::rename(&c, parent.join(format!("{new}{lang}")));
    }
    Ok(target)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn scratch(name: &str) -> PathBuf {
        let d = std::env::temp_dir().join(format!("vf-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&d);
        std::fs::create_dir_all(&d).unwrap();
        d
    }

    #[test]
    fn titles_become_safe_names() {
        assert_eq!(clean("Q&A: what? <now>"), "Q&A. what now");
        assert_eq!(clean("AC/DC | Live"), "AC-DC - Live");
        assert_eq!(clean("  "), "Untitled");
        assert_eq!(clean("con"), "_con");
        assert_eq!(shorten("one two three", 9), "one two");
        assert!(bad_name("a/b").unwrap().contains('/'));
        assert_eq!(bad_name("Sprite Fright"), None);
    }

    #[test]
    fn a_taken_name_gets_a_number() {
        let d = scratch("free");
        assert_eq!(free_name(&d, "A", "mp4"), d.join("A.mp4"));
        std::fs::write(d.join("A.mp4"), b"x").unwrap();
        assert_eq!(free_name(&d, "A", "mp4"), d.join("A 2.mp4"));
        let _ = std::fs::remove_dir_all(&d);
    }

    #[test]
    fn files_and_playlists_are_listed_and_renamed() {
        let d = scratch("list");
        std::fs::write(d.join("Sprite Fright.mp4"), b"x").unwrap();
        std::fs::write(d.join("A song.mp3"), b"x").unwrap();
        std::fs::write(d.join("notes.txt"), b"x").unwrap();
        std::fs::create_dir_all(d.join("Not ours")).unwrap();
        let f = playlist_folder(
            &d,
            "PLx",
            "https://youtube.com/playlist?list=PLx",
            "Open: films",
        )
        .unwrap();
        assert_eq!(f, d.join("Open. films"));
        std::fs::write(f.join("01 - One.mp4"), b"x").unwrap();
        // The same playlist again: the same folder, even renamed.
        std::fs::rename(&f, d.join("Films")).unwrap();
        assert_eq!(
            playlist_folder(&d, "PLx", "", "Open: films").unwrap(),
            d.join("Films")
        );
        let all = list(&d);
        assert_eq!(all.len(), 3, "{all:?}");
        let p = all.iter().find(|e| e.is_playlist()).unwrap();
        assert_eq!(p.holds(), "playlist, 1 video");
        std::fs::write(d.join("Films").join("02 - Two.en.srt"), b"x").unwrap();
        std::fs::write(d.join("Films").join("10 - Ten.m4a"), b"x").unwrap();
        std::fs::write(d.join("Films").join("02 - Two.m4a"), b"x").unwrap();
        let names: Vec<String> = videos_in(&d.join("Films"))
            .into_iter()
            .map(|e| e.name)
            .collect();
        assert_eq!(names, ["01 - One", "02 - Two", "10 - Ten"]);
        let song = all.iter().find(|e| e.name == "A song").unwrap();
        assert_eq!(song.holds(), "sound, MP3");
        let to = rename(song, "Another song").unwrap();
        assert_eq!(to, d.join("Another song.mp3"));
        // A name already taken in the output folder is refused.
        assert!(rename(p, "Another song.mp3").is_err());
        let renamed = rename(p, "Open films").unwrap();
        assert!(read_record(&renamed).is_some_and(|r| r.id == "PLx"));
        let _ = std::fs::remove_dir_all(&d);
    }

    #[test]
    fn a_download_is_noticed_only_at_the_same_quality() {
        let d = scratch("past");
        let record = d.join("downloaded.json");
        let file = d.join("Zoo.mp4");
        std::fs::write(&file, b"x").unwrap();
        assert_eq!(already(&record, "k", "video", 720), None);
        remember(&record, "k", &file, "video", 720);
        assert_eq!(already(&record, "k", "video", 720), Some(file.clone()));
        assert_eq!(already(&record, "k", "video", 1080), None);
        assert_eq!(already(&record, "k", "mp3", 0), None);
        assert_eq!(already(&record, "other", "video", 720), None);
        // Renamed in VidFetch: still known.
        let to = d.join("Zebra.mp4");
        std::fs::rename(&file, &to).unwrap();
        moved(&record, &file, &to);
        assert_eq!(already(&record, "k", "video", 720), Some(to.clone()));
        // Deleted: free to download again.
        std::fs::remove_file(&to).unwrap();
        assert_eq!(already(&record, "k", "video", 720), None);
        let _ = std::fs::remove_dir_all(&d);
    }

    #[test]
    fn captions_follow_their_download() {
        let d = scratch("captions");
        let file = d.join("Zoo.mp4");
        std::fs::write(&file, b"x").unwrap();
        std::fs::write(d.join("Zoo.en.srt"), b"x").unwrap();
        std::fs::write(d.join("Zoo two.en.srt"), b"x").unwrap();
        assert_eq!(captions_of(&file), vec![d.join("Zoo.en.srt")]);
        let e = list(&d).remove(0);
        let to = rename(&e, "Zebra").unwrap();
        assert!(d.join("Zebra.en.srt").exists() && !d.join("Zoo.en.srt").exists());
        // The sound of the same title shares them, so they stay with it.
        std::fs::write(d.join("Zebra.mp3"), b"x").unwrap();
        assert!(captions_of(&to).is_empty());
        let _ = std::fs::remove_dir_all(&d);
    }

    #[test]
    fn the_record_keeps_what_is_done() {
        let d = scratch("record");
        let mut r = Record {
            url: "u".into(),
            id: "PLx".into(),
            title: "T".into(),
            done: BTreeMap::new(),
        };
        r.done.insert("abc".into(), "01 - One.mp4".into());
        write_record(&d, &r).unwrap();
        assert_eq!(read_record(&d), Some(r));
        let _ = std::fs::remove_dir_all(&d);
    }
}
