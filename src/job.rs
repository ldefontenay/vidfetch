//! One download on a background thread, its news posted to the window, which
//! owns all state (Video Scribe's and Photo Scribe's shape). Stopping is a
//! flag the job checks between and inside its steps.
//!
//! A single video: yt-dlp downloads it into VidFetch's own folder, its
//! picture and sound are put together into one file (media.rs), the file is
//! tagged, and only then moved into the output folder, named after its
//! title. A playlist: the same for each of its videos in turn, into a folder
//! of its own, with a record of which are done, so that stopping, or fetching
//! the playlist again later, carries on with the ones still missing.

use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};

use windows::Win32::Foundation::{HWND, LPARAM, WPARAM};
use windows::Win32::UI::WindowsAndMessaging::{PostMessageW, WM_APP};

use crate::config::Format;
use crate::{library, media, tags, ytdlp};

/// The window hears from the job by this message; its LPARAM is a boxed Event.
pub const WM_JOB: u32 = WM_APP + 20;

/// After this many videos of a playlist fail one after the other, the
/// trouble is taken to be the connection or yt-dlp, not the videos.
const FAILURES_IN_A_ROW: usize = 4;

pub enum Event {
    /// A new step: said out loud and shown.
    Step(String),
    /// How far a step has got: shown, not said.
    Progress(String),
    /// Finished: what to say, the file or folder to choose in the list, and
    /// a report to show when there is more to read than can be said.
    Done {
        said: String,
        path: PathBuf,
        report: Option<String>,
    },
    /// A small job's result, said and shown.
    Said(String),
    Failed(String),
    /// Stopped by the user: what was kept.
    Stopped(String),
}

pub struct Settings {
    pub url: String,
    pub output_folder: PathBuf,
    pub format: Format,
    pub lines: u32,
    /// Start a playlist's files with their place in it.
    pub number: bool,
    /// The address names a video in a playlist, and the user asked for the
    /// whole playlist.
    pub whole_list: bool,
    /// The user agreed to fetch yt-dlp, which is not on this computer yet.
    pub fetch_ytdlp: bool,
}

type Post<'a> = &'a dyn Fn(Event);

/// Starts a download; set the returned flag to stop it.
pub fn start(hwnd: HWND, settings: Settings) -> Arc<AtomicBool> {
    spawn(hwnd, move |stop, post| run(&settings, stop, post))
}

/// Updates yt-dlp, on request from the File menu.
pub fn update_ytdlp(hwnd: HWND) -> Arc<AtomicBool> {
    spawn(hwnd, |stop, post| {
        let Some(exe) = ytdlp::find() else {
            return Ok(Event::Said(
                "yt-dlp is not on this computer yet. VidFetch asks to download it the first time \
                 you download a video."
                    .into(),
            ));
        };
        post(Event::Step("Updating yt-dlp.".into()));
        Ok(Event::Said(match ytdlp::update(&exe, stop) {
            Ok(()) => "yt-dlp is up to date.".into(),
            Err(why) => format!("yt-dlp could not update itself. It said: {why}"),
        }))
    })
}

/// Runs `work` on its own thread, posting its news and its end to the window.
fn spawn<F>(hwnd: HWND, work: F) -> Arc<AtomicBool>
where
    F: FnOnce(&AtomicBool, Post) -> Result<Event, String> + Send + 'static,
{
    let stop = Arc::new(AtomicBool::new(false));
    let flag = stop.clone();
    // A window handle is not Send; its number is.
    let window = hwnd.0 as isize;
    std::thread::spawn(move || {
        let post = |e: Event| unsafe {
            let raw = Box::into_raw(Box::new(e));
            if PostMessageW(
                Some(HWND(window as *mut _)),
                WM_JOB,
                WPARAM(0),
                LPARAM(raw as isize),
            )
            .is_err()
            {
                // The window has gone: nobody to tell.
                drop(Box::from_raw(raw));
            }
        };
        let end = work(&flag, &post).unwrap_or_else(Event::Failed);
        post(end);
    });
    stop
}

/// Takes back an event the job posted.
///
/// # Safety
/// `lparam` must come from a WM_JOB message, and be taken only once.
pub unsafe fn take(lparam: LPARAM) -> Box<Event> {
    Box::from_raw(lparam.0 as *mut Event)
}

/// Shows progress at most twice a second, so the Status box is not rewritten
/// thousands of times.
struct Every {
    last: Option<Instant>,
}

impl Every {
    fn new() -> Every {
        Every { last: None }
    }

    fn due(&mut self) -> bool {
        let now = Instant::now();
        if self
            .last
            .is_some_and(|l| now.duration_since(l) < Duration::from_millis(500))
        {
            return false;
        }
        self.last = Some(now);
        true
    }
}

/// yt-dlp, fetched first if the user agreed, and kept up to date.
fn ytdlp_exe(s: &Settings, stop: &AtomicBool, post: Post) -> Result<Option<PathBuf>, String> {
    let exe = match ytdlp::find() {
        Some(exe) => exe,
        None if s.fetch_ytdlp => {
            post(Event::Step("Downloading yt-dlp from GitHub.".into()));
            let mut every = Every::new();
            let got = ytdlp::fetch(stop, &mut |done, total| {
                if total > 0 && every.due() {
                    post(Event::Progress(format!(
                        "Downloading yt-dlp: {:.0}%.",
                        done as f64 * 100.0 / total as f64
                    )));
                }
            })?;
            match got {
                Some(exe) => exe,
                None => return Ok(None),
            }
        }
        None => {
            return Err(
                "VidFetch needs yt-dlp to download videos, and it is not on this computer.".into(),
            )
        }
    };
    if ytdlp::stale(&exe) {
        post(Event::Step("Updating yt-dlp.".into()));
        // An old copy may still work, so a failed update is not a reason to stop.
        let _ = ytdlp::update(&exe, stop);
    }
    Ok(Some(exe))
}

fn run(s: &Settings, stop: &AtomicBool, post: Post) -> Result<Event, String> {
    let stopped = || Event::Stopped("Stopped. Nothing was saved.".into());
    let Some(exe) = ytdlp_exe(s, stop, post)? else {
        return Ok(stopped());
    };
    let _media = media::Session::start()?;
    let work = ytdlp::local_dir().join("downloads");
    std::fs::create_dir_all(&work).map_err(|e| format!("{}: {e}", work.display()))?;
    ytdlp::tidy(&work);
    std::fs::create_dir_all(&s.output_folder).map_err(|e| {
        format!(
            "The output folder {} could not be made: {e}. Choose another in the File menu.",
            s.output_folder.display()
        )
    })?;
    let site = ytdlp::site(&s.url);
    // A YouTube video's address needs no asking what it is, unless the user
    // wants the whole playlist it is in.
    let one_video = ytdlp::is_youtube(&s.url) && ytdlp::video_id(&s.url).is_some();
    let list = if one_video && !s.whole_list {
        None
    } else {
        post(Event::Step(format!(
            "Asking {site} what is at the address."
        )));
        match ytdlp::probe(&exe, &s.url, stop)? {
            Some(list) => list,
            None => return Ok(stopped()),
        }
    };
    match list {
        Some(list) if !list.videos.is_empty() => playlist(s, &exe, &work, list, stop, post),
        Some(list) => Err(format!(
            "The playlist {} has no videos that can be downloaded: they may all be private or \
             removed.",
            list.title
        )),
        None => single(s, &exe, &work, stop, post),
    }
}

/// A video downloaded and made into its final file in the work folder, with
/// what is known about it.
struct Fetched {
    file: PathBuf,
    title: String,
    uploader: Option<String>,
    year: Option<u32>,
    /// What to tell the user about how it was saved, when it is not what
    /// they asked for.
    note: Option<String>,
}

/// Downloads `url` and makes its file, as the user chose. `before` starts
/// every progress line, such as "Video 3 of 12. ". None if stopped.
fn fetch(
    s: &Settings,
    exe: &Path,
    work: &Path,
    url: &str,
    before: &str,
    stop: &AtomicBool,
    post: Post,
) -> Result<Option<Fetched>, String> {
    let site = ytdlp::site(url);
    let mut every = Every::new();
    let Some(saved) = ytdlp::download(exe, url, work, s.format, s.lines, stop, &mut |p| {
        if every.due() {
            let what = p.what;
            post(Event::Progress(match p.total {
                Some(t) if t > 0 => format!(
                    "{before}Downloading the {what} from {site}: {:.0}%.",
                    p.done as f64 * 100.0 / t as f64
                ),
                _ => format!(
                    "{before}Downloading the {what} from {site}: {:.1} MB.",
                    p.done as f64 / 1e6
                ),
            }));
        }
    })?
    else {
        return Ok(None);
    };
    let key = ytdlp::key(url).unwrap_or_else(|| "video".into());
    let picture = saved.iter().find(|f| f.picture && !f.sound);
    let sound = saved.iter().find(|f| f.sound && !f.picture);
    let both = saved.iter().find(|f| f.picture && f.sound);
    let first = saved.first().expect("download returns a file with sound");
    let ext = |p: &Path| {
        p.extension()
            .map(|e| e.to_string_lossy().to_ascii_lowercase())
            .unwrap_or_default()
    };
    let mut note = None;
    let step = |what: &'static str| progress_of(post, before, what);
    let file = match s.format {
        Format::Video => match (picture, sound, both) {
            (Some(pic), Some(snd), _) => {
                let shift = if ext(&pic.path) == "mp4" {
                    ytdlp::drop_edit_list(&pic.path)?
                } else {
                    0.0
                };
                let out = work.join(format!("{key}.joined.mp4"));
                if !media::join(
                    Some((&pic.path, shift)),
                    &snd.path,
                    &out,
                    stop,
                    &mut step("Putting the picture and sound together"),
                )? {
                    return Ok(None);
                }
                out
            }
            (_, _, Some(f)) => f.path.clone(),
            (_, Some(snd), None) => {
                note = Some(format!("{site} offered only the sound, so that was saved."));
                snd.path.clone()
            }
            _ => first.path.clone(),
        },
        Format::M4a => {
            let src = sound.or(both).unwrap_or(first);
            if ext(&src.path) == "m4a" {
                src.path.clone()
            } else if media::sound_is_aac(&src.path) {
                let out = work.join(format!("{key}.sound.m4a"));
                if !media::join(
                    None,
                    &src.path,
                    &out,
                    stop,
                    &mut step("Taking out the sound"),
                )? {
                    return Ok(None);
                }
                out
            } else {
                note = Some(format!(
                    "{site} offered the sound only as {}, which an M4A cannot hold as it is, so it \
                     was saved as it came. Choose MP3 in Download as for a file any player takes.",
                    ext(&src.path).to_uppercase()
                ));
                src.path.clone()
            }
        }
        Format::Mp3 => {
            let src = sound.or(both).unwrap_or(first);
            let out = work.join(format!("{key}.sound.mp3"));
            if !media::mp3(&src.path, &out, stop, &mut step("Making the MP3"))? {
                return Ok(None);
            }
            out
        }
    };
    Ok(Some(Fetched {
        file,
        title: first.title.clone(),
        uploader: first.uploader.clone(),
        year: first.year,
        note,
    }))
}

/// Shows how far `what` has got, as a fraction, at most twice a second.
fn progress_of<'a>(post: Post<'a>, before: &'a str, what: &'a str) -> impl FnMut(f64) + 'a {
    let mut every = Every::new();
    move |f| {
        if every.due() {
            post(Event::Progress(format!(
                "{before}{what}: {:.0}%.",
                f * 100.0
            )));
        }
    }
}

/// Tags the fetched file, then moves it to `to`, which may be on another
/// drive. The work folder's files for `url` go once it is there.
fn keep(f: &Fetched, to: &Path, tags: &tags::Tags, work: &Path, url: &str) -> Result<(), String> {
    // A file the property system cannot tag, such as a WebM, is kept untagged.
    let _ = tags::write(&f.file, tags);
    if std::fs::rename(&f.file, to).is_err() {
        let part = to.with_extension("part");
        std::fs::copy(&f.file, &part)
            .and_then(|_| std::fs::rename(&part, to))
            .map_err(|e| {
                let _ = std::fs::remove_file(&part);
                format!(
                    "{} could not be saved in {}: {e}",
                    to.file_name().unwrap_or_default().to_string_lossy(),
                    to.parent().unwrap_or(to).display()
                )
            })?;
    }
    if let Some(key) = ytdlp::key(url) {
        ytdlp::forget(work, &key);
    }
    Ok(())
}

fn ext_of(p: &Path) -> String {
    p.extension()
        .map(|e| e.to_string_lossy().to_ascii_lowercase())
        .unwrap_or_else(|| "mp4".into())
}

fn single(
    s: &Settings,
    exe: &Path,
    work: &Path,
    stop: &AtomicBool,
    post: Post,
) -> Result<Event, String> {
    post(Event::Step(format!(
        "Downloading from {}.",
        ytdlp::site(&s.url)
    )));
    let Some(f) = fetch(s, exe, work, &s.url, "", stop, post)? else {
        return Ok(Event::Stopped(
            "Stopped. Nothing was saved; downloading the same address again today carries on \
             where it was."
                .into(),
        ));
    };
    let to = library::free_name(
        &s.output_folder,
        &library::name_for(&f.title),
        &ext_of(&f.file),
    );
    let t = tags::Tags {
        title: f.title.clone(),
        artist: f.uploader.clone(),
        year: f.year,
        comment: s.url.clone(),
        ..Default::default()
    };
    keep(&f, &to, &t, work, &s.url)?;
    let name = to
        .file_name()
        .unwrap_or_default()
        .to_string_lossy()
        .into_owned();
    let mut said = format!("Downloaded: {name}. Press Enter to open it.");
    if let Some(n) = &f.note {
        said = format!("{n} {said}");
    }
    Ok(Event::Done {
        said,
        path: to,
        report: None,
    })
}

/// A playlist file's name: its place, when numbered, then its title.
fn item_name(number: bool, at: usize, of: usize, title: &str) -> String {
    let name = library::name_for(title);
    if !number {
        return name;
    }
    let width = of.to_string().len().max(2);
    format!("{at:0width$} - {name}")
}

fn playlist(
    s: &Settings,
    exe: &Path,
    work: &Path,
    list: ytdlp::Playlist,
    stop: &AtomicBool,
    post: Post,
) -> Result<Event, String> {
    let title = if list.title.is_empty() {
        "Playlist".to_string()
    } else {
        list.title.clone()
    };
    let folder = library::playlist_folder(&s.output_folder, &list.id, &s.url, &title)?;
    let mut record = library::read_record(&folder).unwrap_or_default();
    let of = list.videos.len();
    let todo: Vec<(usize, &ytdlp::Item)> = list
        .videos
        .iter()
        .enumerate()
        .map(|(i, v)| (i + 1, v))
        .filter(|(_, v)| !record.done.contains_key(&v.id))
        .collect();
    let already = of - todo.len();
    let folder_name = folder
        .file_name()
        .unwrap_or_default()
        .to_string_lossy()
        .into_owned();
    if todo.is_empty() {
        return Ok(Event::Done {
            said: format!(
                "The playlist {title} has nothing new: all its {of} videos are already in the \
                 folder {folder_name}."
            ),
            path: folder,
            report: None,
        });
    }
    let videos = |n: usize| {
        if n == 1 {
            "1 video".to_string()
        } else {
            format!("{n} videos")
        }
    };
    post(Event::Step(if already == 0 {
        format!(
            "Downloading the playlist {title}: {}. Press Escape to stop; what is done is kept.",
            videos(of)
        )
    } else {
        format!(
            "Downloading the playlist {title}: {} new of {of}, {already} already there. Press \
             Escape to stop; what is done is kept.",
            videos(todo.len())
        )
    }));
    let mut saved = 0;
    let mut failed: Vec<String> = Vec::new();
    let mut in_a_row = 0;
    let mut notes: Vec<String> = Vec::new();
    for (n, (at, item)) in todo.iter().enumerate() {
        if stop.load(Ordering::Relaxed) {
            break;
        }
        let before = format!("Video {} of {}, {}. ", n + 1, todo.len(), item.title);
        post(Event::Progress(format!("{before}Starting.")));
        match fetch(s, exe, work, &item.url, &before, stop, post) {
            Ok(Some(f)) => {
                let title = if f.title.is_empty() {
                    &item.title
                } else {
                    &f.title
                };
                let to = library::free_name(
                    &folder,
                    &item_name(s.number, *at, of, title),
                    &ext_of(&f.file),
                );
                let t = tags::Tags {
                    title: title.clone(),
                    artist: f.uploader.clone(),
                    album: Some(title_or(&list.title, title)),
                    track: Some(*at as u32),
                    year: f.year,
                    comment: item.url.clone(),
                };
                keep(&f, &to, &t, work, &item.url)?;
                record.done.insert(
                    item.id.clone(),
                    to.file_name()
                        .unwrap_or_default()
                        .to_string_lossy()
                        .into_owned(),
                );
                library::write_record(&folder, &record)?;
                if let Some(note) = f.note {
                    if !notes.contains(&note) {
                        notes.push(note);
                    }
                }
                saved += 1;
                in_a_row = 0;
            }
            Ok(None) => break,
            Err(why) => {
                failed.push(format!("{}: {why}", item.title));
                in_a_row += 1;
                if in_a_row >= FAILURES_IN_A_ROW && saved == 0 {
                    return Err(format!(
                        "The first {in_a_row} videos of the playlist {title} could not be \
                         downloaded, so VidFetch stopped. The last said:\n\n{why}"
                    ));
                }
            }
        }
    }
    let kept = format!(
        "{} saved in the folder {folder_name}",
        if saved == 1 {
            "1 video was".to_string()
        } else {
            format!("{saved} videos were")
        }
    );
    if stop.load(Ordering::Relaxed) {
        return Ok(Event::Stopped(format!(
            "Stopped. {kept}. Update the playlist, Ctrl+U, carries on with the rest."
        )));
    }
    let mut said = format!("Finished the playlist {title}. {kept}.");
    if already > 0 {
        said += &format!(" {already} were already there.");
    }
    let report = if failed.is_empty() && notes.is_empty() {
        None
    } else {
        let mut r = said.clone();
        if !failed.is_empty() {
            said += &format!(
                " {} could not be downloaded; the report says why.",
                if failed.len() == 1 {
                    "1 video".to_string()
                } else {
                    format!("{} videos", failed.len())
                }
            );
            r += &format!(
                "\n\nThese could not be downloaded. Update the playlist, Ctrl+U, tries them \
                 again:\n\n{}",
                failed.join("\n\n")
            );
        }
        for n in &notes {
            r += &format!("\n\n{n}");
        }
        Some(r)
    };
    Ok(Event::Done {
        said,
        path: folder,
        report,
    })
}

fn title_or(list: &str, video: &str) -> String {
    if list.is_empty() {
        video.to_string()
    } else {
        list.to_string()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn playlist_files_are_numbered_to_the_longest_place() {
        assert_eq!(item_name(true, 3, 12, "A: b"), "03 - A. b");
        assert_eq!(item_name(true, 7, 150, "Seven"), "007 - Seven");
        assert_eq!(item_name(false, 7, 150, "Seven"), "Seven");
    }
}
