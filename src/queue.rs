//! What waits its turn to download, and what runs now. Each download is
//! settled when it is asked for, so changing the options later does not
//! change it. The queue is kept in `%APPDATA%\VidFetch\queue.json`, so what
//! had not finished when VidFetch closed is offered again when it starts
//! (log entry 2, Q3). Titles are fetched in the background as downloads are
//! added, as only the address is known until then (log entry 2, Q2).

use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};

use windows::Win32::Foundation::{HWND, LPARAM, WPARAM};
use windows::Win32::UI::WindowsAndMessaging::{PostMessageW, WM_APP};

use crate::config::{Config, Format};
use crate::job;
use crate::ytdlp;

/// A title has come for a download: its LPARAM is a boxed `(id, title)`.
pub const WM_TITLE: u32 = WM_APP + 21;

/// Numbers each download, so a title finds its own.
static NEXT_ID: AtomicU64 = AtomicU64::new(1);

pub struct Queued {
    pub id: u64,
    pub settings: job::Settings,
    /// The playlist folder it updates, which must stay as it is until it is done.
    pub into: Option<PathBuf>,
    /// The video's or playlist's title, once known.
    pub title: Option<String>,
}

impl Queued {
    pub fn new(settings: job::Settings, into: Option<PathBuf>) -> Queued {
        Queued {
            id: NEXT_ID.fetch_add(1, Ordering::Relaxed),
            settings,
            into,
            title: None,
        }
    }

    pub fn same_as(&self, o: &Queued) -> bool {
        let (a, b) = (&self.settings, &o.settings);
        let lines = |s: &job::Settings| job::past_lines(s.format, s.lines);
        a.url == b.url
            && a.whole_list == b.whole_list
            && a.format == b.format
            && lines(a) == lines(b)
    }

    /// What it is, in words: its title once known, else where it is from.
    pub fn label(&self) -> String {
        let s = &self.settings;
        let how = match s.format {
            Format::Video => format!("video at {} lines", s.lines),
            Format::M4a => "sound, M4A".to_string(),
            Format::Mp3 => "sound, MP3".to_string(),
        };
        let folder = self
            .into
            .as_deref()
            .and_then(|p| p.file_name())
            .map(|n| n.to_string_lossy().into_owned());
        match (&self.title, folder) {
            (_, Some(name)) => format!("Update the playlist {name}, {how}"),
            (Some(t), None) if s.whole_list => format!("The playlist {t}, {how}"),
            (Some(t), None) => format!("{t}, {how}"),
            (None, None) if s.whole_list => format!(
                "The whole playlist from {}, {how}: {}",
                ytdlp::site(&s.url),
                s.url
            ),
            (None, None) => format!("From {}, {how}: {}", ytdlp::site(&s.url), s.url),
        }
    }

    /// The same download, for the job to take: the queue keeps its own.
    pub fn settings(&self) -> job::Settings {
        let s = &self.settings;
        job::Settings {
            url: s.url.clone(),
            output_folder: s.output_folder.clone(),
            ..*s
        }
    }
}

/// Fetches the title of `q` in the background, posting it to `hwnd`. Nothing
/// happens if yt-dlp is not there yet, or the title cannot be had: the label
/// keeps the address.
pub fn fetch_title(hwnd: HWND, q: &Queued) {
    if q.title.is_some() || q.into.is_some() {
        return;
    }
    let Some(exe) = ytdlp::find() else { return };
    let (id, url, whole) = (q.id, q.settings.url.clone(), q.settings.whole_list);
    // Sent as a number: a window handle cannot cross to another thread.
    let to = hwnd.0 as isize;
    std::thread::spawn(move || {
        let stop = AtomicBool::new(false);
        if let Some(title) = ytdlp::title(&exe, &url, whole, &stop) {
            let boxed = Box::new((id, title));
            unsafe {
                let raw = Box::into_raw(boxed);
                if PostMessageW(
                    Some(HWND(to as *mut _)),
                    WM_TITLE,
                    WPARAM(0),
                    LPARAM(raw as isize),
                )
                .is_err()
                {
                    drop(Box::from_raw(raw));
                }
            }
        }
    });
}

/// The `(id, title)` a WM_TITLE carries.
pub fn take_title(lparam: LPARAM) -> (u64, String) {
    *unsafe { Box::from_raw(lparam.0 as *mut (u64, String)) }
}

/// Keeps `items`, the one running first, for next time. Failing to save only
/// means they are not offered again.
pub fn save<'a>(items: impl Iterator<Item = &'a Queued>) {
    let Some(path) = Config::queue_path() else {
        return;
    };
    let v: Vec<serde_json::Value> = items.map(to_json).collect();
    write(&path, &v);
}

fn to_json(q: &Queued) -> serde_json::Value {
    let s = &q.settings;
    serde_json::json!({
        "url": s.url,
        "output folder": s.output_folder.display().to_string(),
        "format": s.format.word(),
        "lines": s.lines,
        "number": s.number,
        "captions": s.captions,
        "whole playlist": s.whole_list,
        "into": q.into.as_ref().map(|p| p.display().to_string()),
        "title": q.title,
    })
}

fn write(path: &Path, v: &[serde_json::Value]) {
    if v.is_empty() {
        let _ = std::fs::remove_file(path);
        return;
    }
    if let Some(dir) = path.parent() {
        let _ = std::fs::create_dir_all(dir);
    }
    let part = path.with_extension("part");
    let text = serde_json::to_string_pretty(v).unwrap_or_default();
    if std::fs::write(&part, text).is_ok() {
        let _ = std::fs::rename(&part, path);
    }
}

/// What was kept last time, in order.
pub fn load() -> Vec<Queued> {
    Config::queue_path().map_or_else(Vec::new, |p| read(&p))
}

fn read(path: &Path) -> Vec<Queued> {
    let text = std::fs::read_to_string(path).unwrap_or_default();
    // Notepad may have saved it with a byte order mark.
    let v: serde_json::Value =
        serde_json::from_str(text.trim_start_matches('\u{feff}')).unwrap_or_default();
    v.as_array()
        .into_iter()
        .flatten()
        .filter_map(|q| {
            let text = |k: &str| q[k].as_str().map(str::to_string);
            let yes = |k: &str| q[k].as_bool().unwrap_or(false);
            let settings = job::Settings {
                url: text("url").filter(|u| !u.is_empty())?,
                output_folder: PathBuf::from(text("output folder")?),
                format: Format::from_word(&text("format").unwrap_or_default()),
                lines: q["lines"].as_u64().unwrap_or(720) as u32,
                number: yes("number"),
                captions: yes("captions"),
                whole_list: yes("whole playlist"),
                fetch_ytdlp: false,
            };
            let mut item = Queued::new(settings, text("into").map(PathBuf::from));
            item.title = text("title");
            Some(item)
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn settings(url: &str, whole: bool) -> job::Settings {
        job::Settings {
            url: url.into(),
            output_folder: PathBuf::from(r"C:\Out"),
            format: Format::Video,
            lines: 480,
            number: true,
            captions: true,
            whole_list: whole,
            fetch_ytdlp: false,
        }
    }

    #[test]
    fn labels_say_the_title_once_known() {
        let mut q = Queued::new(settings("https://www.youtube.com/watch?v=x", false), None);
        assert!(q
            .label()
            .starts_with("From YouTube, video at 480 lines: https://"));
        q.title = Some("Me at the zoo".into());
        assert_eq!(q.label(), "Me at the zoo, video at 480 lines");
        let mut p = Queued::new(
            settings("https://www.youtube.com/playlist?list=PL", true),
            None,
        );
        p.title = Some("Open Movies".into());
        assert_eq!(p.label(), "The playlist Open Movies, video at 480 lines");
    }

    #[test]
    fn the_queue_is_kept_and_read_back() {
        let d = std::env::temp_dir().join(format!("vf-queue-{}", std::process::id()));
        let path = d.join("queue.json");
        let mut a = Queued::new(settings("https://a", false), None);
        a.title = Some("A".into());
        let b = Queued::new(
            settings("https://b", true),
            Some(PathBuf::from(r"C:\Out\B")),
        );
        write(&path, &[to_json(&a), to_json(&b)]);
        let back = read(&path);
        assert_eq!(back.len(), 2);
        assert!(back[0].same_as(&a) && back[1].same_as(&b));
        assert_eq!(back[0].title.as_deref(), Some("A"));
        assert_eq!(back[1].into, b.into);
        assert!(back[0].settings.captions && back[1].settings.whole_list);
        // Nothing waiting: the file goes.
        write(&path, &[]);
        assert!(!path.exists());
        let _ = std::fs::remove_dir_all(&d);
    }
}
