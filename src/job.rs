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

use crate::config::{Config, Format};
use crate::{captions, library, media, tags, ytdlp};

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
    /// Save each video's captions beside it.
    pub captions: bool,
    /// The address names a video in a playlist, and the user asked for the
    /// whole playlist.
    pub whole_list: bool,
    /// The user agreed to fetch yt-dlp, which is not on this computer yet.
    pub fetch_ytdlp: bool,
}

type Post<'a> = &'a (dyn Fn(Event) + Sync);

/// Starts a download; set the returned flag to stop it.
pub fn start(hwnd: HWND, settings: Settings) -> Arc<AtomicBool> {
    spawn(hwnd, move |stop, post| run(&settings, stop, post))
}

/// Fetches Deno, on request from the File menu.
pub fn fetch_deno(hwnd: HWND) -> Arc<AtomicBool> {
    spawn(hwnd, |stop, post| {
        post(Event::Step("Fetching Deno.".into()));
        let mut last = 0;
        let mut progress = |got: u64, all: u64| {
            let percent = (got * 100).checked_div(all).unwrap_or(0);
            if percent >= last + 10 {
                last = percent;
                post(Event::Step(format!("Fetching Deno: {percent} percent.")));
            }
        };
        Ok(Event::Said(match ytdlp::fetch_deno(stop, &mut progress) {
            Ok(Some(())) => "Deno is fetched, and yt-dlp will use it for YouTube.".into(),
            Ok(None) => "Stopped: Deno was not fetched.".into(),
            Err(why) => format!("Deno could not be fetched: {why}"),
        }))
    })
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
    match download(s, exe, work, url, before, stop, post)? {
        Some(saved) => finish(s, work, url, saved, before, stop, post),
        None => Ok(None),
    }
}

/// The network half of `fetch`: yt-dlp brings the files into `work`.
fn download(
    s: &Settings,
    exe: &Path,
    work: &Path,
    url: &str,
    before: &str,
    stop: &AtomicBool,
    post: Post,
) -> Result<Option<Vec<ytdlp::Saved>>, String> {
    let site = ytdlp::site(url);
    let mut every = Every::new();
    ytdlp::download(exe, url, work, s.format, s.lines, stop, &mut |p| {
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
    })
}

/// The second half of `fetch`: makes the file the user chose from what yt-dlp
/// brought. Needs a `media::Session` on its thread. Every file it touches is
/// named after this video, so another video can be downloading meanwhile.
fn finish(
    s: &Settings,
    work: &Path,
    url: &str,
    saved: Vec<ytdlp::Saved>,
    before: &str,
    stop: &AtomicBool,
    post: Post,
) -> Result<Option<Fetched>, String> {
    let site = ytdlp::site(url);
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
    // Some of YouTube's sound files carry an edit list too, and Media
    // Foundation then reads no sound from them at all (milestone 4 test,
    // section 3: "no samples were processed by the sink"). Dropping it costs
    // the first few hundredths of a second of the encoder's lead-in.
    if let Some(snd) = sound.or(both).or(Some(first)) {
        if matches!(ext(&snd.path).as_str(), "m4a" | "mp4") && !snd.picture {
            ytdlp::drop_edit_list(&snd.path)?;
        }
    }
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
            // Copied even when it is an M4A already: YouTube's is cut up for
            // streaming, which Windows cannot tag and some players cannot
            // wind through.
            if media::sound_is_aac(&src.path) {
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
                    "{site} offered the sound only in a form Windows cannot copy into a new M4A, \
                     so it was saved as it came. Choose MP3 in Download as for a file any player \
                     takes."
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

/// Saves `url`'s captions beside the download at `to`, as an SRT file. What to
/// tell the user when there are none to save; a video is never lost to its
/// captions, so a failure here is only that.
fn save_captions(
    exe: &Path,
    work: &Path,
    url: &str,
    to: &Path,
    title: &str,
    stop: &AtomicBool,
) -> Option<String> {
    let saved = ytdlp::captions(exe, url, work, stop);
    if stop.load(Ordering::Relaxed) {
        if let Some(key) = ytdlp::key(url) {
            ytdlp::forget(work, &key);
        }
        return None;
    }
    let result = match saved {
        Ok(None) => Some(format!("{title} has no captions, so none were saved.")),
        Ok(Some((file, lang))) => write_srt(&file, to, &lang)
            .err()
            .map(|why| format!("The captions of {title} could not be saved: {why}")),
        Err(why) => Some(format!(
            "The captions of {title} could not be fetched, though the download itself is saved. \
             yt-dlp said: {why}"
        )),
    };
    if let Some(key) = ytdlp::key(url) {
        ytdlp::forget(work, &key);
    }
    result
}

/// The captions in `from` as `Title.en.srt` beside `to`, under a temporary
/// name until they are whole.
fn write_srt(from: &Path, to: &Path, lang: &str) -> Result<(), String> {
    let text = std::fs::read(from).map_err(|e| e.to_string())?;
    let cues = captions::parse(&String::from_utf8_lossy(&text));
    if cues.is_empty() {
        return Err("they came in a form VidFetch cannot read.".into());
    }
    let stem = to.file_stem().unwrap_or_default().to_string_lossy();
    let srt = to.with_file_name(format!("{stem}.{lang}.srt"));
    let part = srt.with_extension("srt.part");
    std::fs::write(&part, captions::to_srt(&cues))
        .and_then(|_| std::fs::rename(&part, &srt))
        .map_err(|e| {
            let _ = std::fs::remove_file(&part);
            e.to_string()
        })
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
    if let (Some(record), Some(key)) = (Config::past_path(), ytdlp::key(&s.url)) {
        library::remember(
            &record,
            &key,
            &to,
            s.format.word(),
            past_lines(s.format, s.lines),
            &s.url,
        );
    }
    let name = to
        .file_name()
        .unwrap_or_default()
        .to_string_lossy()
        .into_owned();
    let mut said = format!("Downloaded: {name}. Press Enter to play it.");
    if let Some(n) = &f.note {
        said = format!("{n} {said}");
    }
    if s.captions {
        post(Event::Progress(format!("Fetching the captions of {name}.")));
        if let Some(n) = save_captions(exe, work, &s.url, &to, &f.title, stop) {
            said += &format!(" {n}");
        }
    }
    Ok(Event::Done {
        said,
        path: to,
        report: None,
    })
}

/// The lines of picture a download is remembered by: none for sound, where
/// the setting makes no difference.
pub fn past_lines(format: Format, lines: u32) -> u32 {
    match format {
        Format::Video => lines,
        Format::M4a | Format::Mp3 => 0,
    }
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
    // Takes in one video's result, in playlist order. False if it was stopped.
    // `shown`: whether Status may follow its captions, which it may not while
    // the next video is downloading (milestone 8 test, 1.1).
    let mut settle = |at: usize,
                      item: &ytdlp::Item,
                      before: &str,
                      result: Result<Option<Fetched>, String>,
                      shown: bool|
     -> Result<bool, String> {
        match result {
            Ok(Some(f)) => {
                let title = if f.title.is_empty() {
                    &item.title
                } else {
                    &f.title
                };
                let to = library::free_name(
                    &folder,
                    &item_name(s.number, at, of, title),
                    &ext_of(&f.file),
                );
                let t = tags::Tags {
                    title: title.clone(),
                    artist: f.uploader.clone(),
                    album: Some(title_or(&list.title, title)),
                    track: Some(at as u32),
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
                if s.captions {
                    if shown {
                        post(Event::Progress(format!("{before}Fetching the captions.")));
                    }
                    notes.extend(save_captions(exe, work, &item.url, &to, title, stop));
                }
                saved += 1;
                in_a_row = 0;
                Ok(true)
            }
            Ok(None) => Ok(false),
            Err(why) => {
                failed.push(format!("{}: {why}", item.title));
                in_a_row += 1;
                if in_a_row >= FAILURES_IN_A_ROW && saved == 0 {
                    return Err(format!(
                        "The first {in_a_row} videos of the playlist {title} could not be \
                         downloaded, so VidFetch stopped. The last said:\n\n{why}"
                    ));
                }
                Ok(true)
            }
        }
    };
    let wait = |h: std::thread::ScopedJoinHandle<'_, Result<Option<Fetched>, String>>| {
        h.join()
            .unwrap_or_else(|_| Err("Making this video's file failed unexpectedly.".to_string()))
    };
    // The picture and sound of one video are put together on a second thread
    // while the next one downloads, which saves the wait on a long playlist.
    // Never more than one: Status follows the download, and what is saved
    // stays in playlist order because `settle` runs here, in turn.
    std::thread::scope(|scope| -> Result<(), String> {
        let mut pending = None;
        for (n, (at, item)) in todo.iter().enumerate() {
            if stop.load(Ordering::Relaxed) {
                break;
            }
            let before = format!("Video {} of {}, {}. ", n + 1, todo.len(), item.title);
            post(Event::Progress(format!("{before}Starting.")));
            // The same video twice in a list: `keep` clears its files, so the
            // first must be done with before the second is fetched.
            if pending
                .as_ref()
                .is_some_and(|(_, i, _, _): &(usize, &ytdlp::Item, String, _)| i.url == item.url)
            {
                let (a, i, b, h) = pending.take().expect("just looked");
                if !settle(a, i, &b, wait(h), false)? {
                    break;
                }
            }
            let got = match download(s, exe, work, &item.url, &before, stop, post) {
                Ok(Some(got)) => got,
                Ok(None) => break,
                Err(why) => {
                    if let Some((a, i, b, h)) = pending.take() {
                        if !settle(a, i, &b, wait(h), false)? {
                            break;
                        }
                    }
                    settle(*at, item, &before, Err(why), true)?;
                    continue;
                }
            };
            if let Some((a, i, b, h)) = pending.take() {
                post(Event::Progress(format!(
                    "{before}Downloaded; finishing the video before it."
                )));
                if !settle(a, i, &b, wait(h), false)? {
                    break;
                }
            }
            if stop.load(Ordering::Relaxed) {
                break;
            }
            if n + 1 == todo.len() {
                // Nothing left to overlap with, so Status can follow this one.
                let done = finish(s, work, &item.url, got, &before, stop, post);
                settle(*at, item, &before, done, true)?;
            } else {
                let line = before.clone();
                let url = item.url.as_str();
                let handle = scope.spawn(move || {
                    let _media = media::Session::start()?;
                    let quiet = |_: Event| {};
                    finish(s, work, url, got, &line, stop, &quiet)
                });
                pending = Some((*at, item, before, handle));
            }
        }
        if let Some((a, i, b, h)) = pending.take() {
            post(Event::Progress(format!("{b}Finishing.")));
            settle(a, i, &b, wait(h), true)?;
        }
        Ok(())
    })?;
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

    /// A real download, printing what the window would hear. Needs VF_URL
    /// and VF_OUT; VF_FORMAT is video, m4a or mp3, and VF_WHOLE takes the
    /// whole playlist a video is in:
    /// `cargo test end_to_end -- --ignored --nocapture`.
    #[test]
    #[ignore]
    fn end_to_end() {
        let var = |n: &str| std::env::var(n).ok();
        let s = Settings {
            url: var("VF_URL").expect("VF_URL"),
            output_folder: PathBuf::from(var("VF_OUT").expect("VF_OUT")),
            format: match var("VF_FORMAT").as_deref() {
                Some("m4a") => Format::M4a,
                Some("mp3") => Format::Mp3,
                _ => Format::Video,
            },
            captions: var("VF_CAPTIONS").is_some(),
            lines: var("VF_LINES").and_then(|l| l.parse().ok()).unwrap_or(720),
            number: true,
            whole_list: var("VF_WHOLE").is_some(),
            fetch_ytdlp: true,
        };
        let stop = AtomicBool::new(false);
        // VF_STOP_AT=3 presses Escape as the third video of a playlist starts.
        let stop_at = var("VF_STOP_AT").map(|n| format!("Video {n} of"));
        let last = std::sync::Mutex::new(String::new());
        let post = |e: Event| match e {
            Event::Step(t) => println!("STEP {t}"),
            Event::Progress(t) => {
                if stop_at.as_ref().is_some_and(|s| t.starts_with(s.as_str())) {
                    stop.store(true, Ordering::Relaxed);
                }
                // The first of each kind of line, not every percent.
                let kind: String = t.chars().take_while(|c| *c != ':').collect();
                let mut last = last.lock().unwrap();
                if *last != kind {
                    println!("PROGRESS {t}");
                    *last = kind;
                }
            }
            _ => {}
        };
        let started = Instant::now();
        match run(&s, &stop, &post) {
            Ok(Event::Done { said, path, report }) => {
                println!(
                    "DONE {said}
{}",
                    path.display()
                );
                if let Some(r) = report {
                    println!("REPORT {r}");
                }
            }
            Ok(Event::Stopped(t)) => println!("STOPPED {t}"),
            Ok(_) => println!("OTHER"),
            Err(why) => panic!("{why}"),
        }
        println!("{:.0} s", started.elapsed().as_secs_f64());
    }

    #[test]
    fn playlist_files_are_numbered_to_the_longest_place() {
        assert_eq!(item_name(true, 3, 12, "A: b"), "03 - A. b");
        assert_eq!(item_name(true, 7, 150, "Seven"), "007 - Seven");
        assert_eq!(item_name(false, 7, 150, "Seven"), "Seven");
    }
}
