//! Downloading through yt-dlp, the one other program VidFetch starts. From
//! Video Scribe's ytdlp.rs, which settled most of what is here.
//!
//! yt-dlp is found on the PATH, or fetched from its official GitHub release
//! into `%LOCALAPPDATA%\VidFetch\tools`, checked against the release's
//! SHA2-256SUMS. There is no FFmpeg: YouTube's picture and sound come down as
//! two files, and VidFetch puts them together itself with Media Foundation
//! (media.rs). `--ffmpeg-location` points at an empty folder, so a machine
//! that happens to have FFmpeg behaves like one that does not. Only formats
//! fetched over plain HTTP are asked for: a stream in pieces (HLS) would need
//! FFmpeg to be put together into a file Windows can read.

use std::io::{BufRead, BufReader};
use std::os::windows::io::AsRawHandle;
use std::os::windows::process::CommandExt;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc;
use std::time::{Duration, SystemTime};

use windows::core::PCWSTR;
use windows::Win32::Foundation::{CloseHandle, HANDLE};
use windows::Win32::System::JobObjects::*;

use crate::config::Format;
use crate::net;

const CREATE_NO_WINDOW: u32 = 0x0800_0000;
/// Marks our lines so nothing else yt-dlp prints is mistaken for one.
const PROGRESS: &str = "VFPROGRESS";
const FILE: &str = "VFFILE";
const RELEASE: &str = "/yt-dlp/yt-dlp/releases/latest/download/";
/// Our own copy is updated with `-U` when it is older than this: YouTube
/// changes often, and an old yt-dlp is the usual reason a download fails.
const UPDATE_AFTER: Duration = Duration::from_secs(14 * 24 * 3600);

/// `%LOCALAPPDATA%\VidFetch`, for yt-dlp and downloads in progress: they can
/// be fetched again, so they do not belong with the settings in roaming
/// AppData.
pub fn local_dir() -> PathBuf {
    std::env::var_os("LOCALAPPDATA")
        .map(PathBuf::from)
        .unwrap_or_else(std::env::temp_dir)
        .join("VidFetch")
}

fn own_copy() -> PathBuf {
    local_dir().join("tools").join("yt-dlp.exe")
}

/// yt-dlp on the PATH, or our own copy. None when neither is there.
pub fn find() -> Option<PathBuf> {
    let path = std::env::var_os("PATH")?;
    std::env::split_paths(&path)
        .map(|d| d.join("yt-dlp.exe"))
        .find(|p| p.is_file())
        .or_else(|| Some(own_copy()).filter(|p| p.is_file()))
}

/// Fetches yt-dlp.exe from its latest release and checks it against the
/// release's SHA2-256SUMS before keeping it. None if stopped.
pub fn fetch(
    stop: &AtomicBool,
    progress: &mut dyn FnMut(u64, u64),
) -> Result<Option<PathBuf>, String> {
    let sums = match net::download(
        "github.com",
        &format!("{RELEASE}SHA2-256SUMS"),
        stop,
        &mut |_, _| {},
    )? {
        Some(s) => String::from_utf8_lossy(&s).into_owned(),
        None => return Ok(None),
    };
    let Some(want) = expected_hash(&sums, "yt-dlp.exe") else {
        return Err("yt-dlp's release has no checksum for yt-dlp.exe, so it was not kept.".into());
    };
    let Some(exe) = net::download(
        "github.com",
        &format!("{RELEASE}yt-dlp.exe"),
        stop,
        progress,
    )?
    else {
        return Ok(None);
    };
    let got = net::hex(&net::sha256(&exe));
    if got != want {
        return Err(
            "The yt-dlp that was downloaded does not match its published checksum, so it was not \
             kept. A new release may have come out during the download: try again."
                .into(),
        );
    }
    let path = own_copy();
    let dir = path.parent().unwrap_or(Path::new("."));
    std::fs::create_dir_all(dir).map_err(|e| format!("{}: {e}", dir.display()))?;
    // Written beside, then renamed, so a half-written exe is never run.
    let part = path.with_extension("exe.part");
    std::fs::write(&part, &exe).map_err(|e| format!("{}: {e}", part.display()))?;
    std::fs::rename(&part, &path).map_err(|e| format!("{}: {e}", path.display()))?;
    Ok(Some(path))
}

/// The hash for `name` in a `<hash>  <name>` list.
fn expected_hash(sums: &str, name: &str) -> Option<String> {
    sums.lines().find_map(|l| {
        let mut it = l.split_whitespace();
        let hash = it.next()?;
        let file = it.next()?.trim_start_matches('*');
        (file == name && hash.len() == 64).then(|| hash.to_ascii_lowercase())
    })
}

/// True for our own copy when it is due an update.
pub fn stale(exe: &Path) -> bool {
    exe == own_copy()
        && std::fs::metadata(exe)
            .and_then(|m| m.modified())
            .ok()
            .and_then(|t| SystemTime::now().duration_since(t).ok())
            .is_some_and(|age| age > UPDATE_AFTER)
}

#[derive(Debug, PartialEq)]
pub struct Progress {
    /// What the file holds, in words: "picture", "sound", or "video" when
    /// the site did not say, which is usually both.
    pub what: &'static str,
    pub done: u64,
    pub total: Option<u64>,
}

/// One `--progress-template` line: `VFPROGRESS <vcodec> <downloaded> <total>
/// <estimate>`, where yt-dlp writes `NA` for a value it does not know and
/// `none` for the sound's video codec.
fn parse_progress(line: &str) -> Option<Progress> {
    let mut it = line.trim().strip_prefix(PROGRESS)?.split_whitespace();
    let codec = it.next()?;
    let done = it.next()?.parse().ok()?;
    let num = |s: Option<&str>| s.and_then(|s| s.parse::<f64>().ok()).map(|v| v as u64);
    let total = num(it.next());
    let estimate = num(it.next());
    Some(Progress {
        what: match codec {
            "none" => "sound",
            "NA" => "video",
            _ => "picture",
        },
        done,
        total: total.or(estimate),
    })
}

/// What to ask for. A video: the picture (H.264, which an MP4 holds and
/// every player plays) and the sound (AAC) apart, at most `lines` high, as
/// YouTube offers them; else one MP4 with both; else whatever single file
/// there is. Sound only: AAC first. Always over plain HTTP.
fn formats(format: Format, lines: u32) -> String {
    match format {
        Format::Video => format!(
            "(bv*[vcodec^=avc1][height<={lines}][protocol^=http],ba[ext=m4a][protocol^=http])\
             /b[ext=mp4][height<={lines}][protocol^=http]/b[ext=mp4][protocol^=http]\
             /b[protocol^=http]"
        ),
        Format::M4a | Format::Mp3 => {
            "ba[ext=m4a][protocol^=http]/ba[protocol^=http]/b[ext=mp4][protocol^=http]\
             /b[protocol^=http]"
                .into()
        }
    }
}

fn args(url: &str, out_dir: &Path, no_ffmpeg: &Path, format: Format, lines: u32) -> Vec<String> {
    let out = out_dir.display().to_string();
    let none = no_ffmpeg.display().to_string();
    // Files are named by our own key: a title would become a file name to
    // clean, and another site's ids may clash or hold any character. The
    // finished file is named after the title later (library.rs).
    let name = key(url).unwrap_or_else(|| "video".into());
    let formats = formats(format, lines);
    let media = format!("{name}.f%(format_id)s.%(ext)s");
    [
        "--ignore-config",
        // Titles in any language: piped, yt-dlp otherwise writes Windows'
        // old code page, where "Les Misérables" is not UTF-8.
        "--encoding",
        "utf-8",
        "--no-playlist",
        "--newline",
        // "--print" below implies quiet mode, which hides progress; this brings it back.
        "--progress",
        "--no-colors",
        "--no-mtime",
        "--retries",
        "5",
        "--fragment-retries",
        "5",
        "-f",
        &formats,
        "--ffmpeg-location",
        &none,
        "--progress-template",
        &format!(
            "download:{PROGRESS} %(info.vcodec)s %(progress.downloaded_bytes)s \
             %(progress.total_bytes)s %(progress.total_bytes_estimate)s"
        ),
        "-P",
        &out,
        "-o",
        &media,
        // A page with several videos, as archive.org's can be: the first.
        "--playlist-items",
        "1",
        "--print",
        &format!(
            "after_move:{FILE} %(filepath)s\t%(vcodec)s\t%(acodec)s\t%(upload_date)s\t\
             %(uploader)s\t%(title)s"
        ),
        "--",
        url,
    ]
    .iter()
    .map(|s| s.to_string())
    .collect()
}

/// A job object that ends yt-dlp, and the Python process it starts, when it
/// is closed or when VidFetch ends: killing the first process alone would
/// leave the second downloading.
struct Job(HANDLE);

impl Job {
    fn new() -> Option<Job> {
        unsafe {
            let job = CreateJobObjectW(None, PCWSTR::null()).ok()?;
            let mut info = JOBOBJECT_EXTENDED_LIMIT_INFORMATION::default();
            info.BasicLimitInformation.LimitFlags = JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE;
            SetInformationJobObject(
                job,
                JobObjectExtendedLimitInformation,
                &info as *const _ as *const core::ffi::c_void,
                std::mem::size_of::<JOBOBJECT_EXTENDED_LIMIT_INFORMATION>() as u32,
            )
            .ok()?;
            Some(Job(job))
        }
    }
}

impl Drop for Job {
    fn drop(&mut self) {
        unsafe {
            let _ = CloseHandle(self.0);
        }
    }
}

enum Line {
    Out(String),
    Err(String),
}

/// Runs yt-dlp with `args`, handing each line of its output to `line`.
/// Returns the exit status and the error lines, or None if stopped.
fn run(
    exe: &Path,
    args: &[String],
    stop: &AtomicBool,
    line: &mut dyn FnMut(&str),
) -> Result<Option<(bool, Vec<String>)>, String> {
    let job = Job::new();
    let mut child = Command::new(exe)
        .args(args)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .creation_flags(CREATE_NO_WINDOW)
        .spawn()
        .map_err(|e| format!("yt-dlp would not start ({}): {e}", exe.display()))?;
    if let Some(job) = &job {
        unsafe {
            let _ = AssignProcessToJobObject(job.0, HANDLE(child.as_raw_handle()));
        }
    }
    let (tx, rx) = mpsc::channel();
    let lines = |r: Box<dyn std::io::Read + Send>, tx: mpsc::Sender<Line>, out: bool| {
        std::thread::spawn(move || {
            let mut r = BufReader::new(r);
            let mut buf = Vec::new();
            while r.read_until(b'\n', &mut buf).is_ok_and(|n| n > 0) {
                let l = String::from_utf8_lossy(&buf)
                    .trim_end_matches(['\r', '\n'])
                    .to_string();
                buf.clear();
                let _ = tx.send(if out { Line::Out(l) } else { Line::Err(l) });
            }
        })
    };
    let out = child
        .stdout
        .take()
        .map(|o| lines(Box::new(o), tx.clone(), true));
    let err = child.stderr.take().map(|e| lines(Box::new(e), tx, false));
    let mut errors = Vec::new();
    loop {
        if stop.load(Ordering::Relaxed) {
            if let Some(job) = &job {
                unsafe {
                    let _ = TerminateJobObject(job.0, 1);
                }
            }
            let _ = child.kill();
            let _ = child.wait();
            return Ok(None);
        }
        match rx.recv_timeout(Duration::from_millis(100)) {
            Ok(Line::Out(l)) => line(&l),
            Ok(Line::Err(l)) => {
                if !l.trim().is_empty() {
                    errors.push(l);
                }
            }
            Err(mpsc::RecvTimeoutError::Timeout) => {}
            Err(mpsc::RecvTimeoutError::Disconnected) => break,
        }
    }
    for t in [out, err].into_iter().flatten() {
        let _ = t.join();
    }
    let status = child.wait().map_err(|e| e.to_string())?;
    Ok(Some((status.success(), errors)))
}

/// `yt-dlp -U` on our own copy. A failed update is not a reason to stop.
/// What yt-dlp said, when it said why it could not update.
pub fn update(exe: &Path, stop: &AtomicBool) -> Result<(), String> {
    match run(exe, &["-U".to_string()], stop, &mut |_| {}) {
        Ok(Some((true, _))) => {
            // Touch the file, so an up-to-date copy is not checked every time.
            let _ = std::fs::OpenOptions::new()
                .append(true)
                .open(exe)
                .and_then(|f| f.set_modified(SystemTime::now()));
            Ok(())
        }
        Ok(Some((false, errors))) => Err(last_error(&errors)),
        Ok(None) => Ok(()),
        Err(e) => Err(e),
    }
}

/// One file yt-dlp saved, from a `VFFILE <path> <vcodec> <acodec> <date>
/// <uploader> <title>` line, tab-separated. yt-dlp writes `none` for what a
/// file lacks, and often `NA` for what another site did not say, which may
/// be either: a file is taken as holding both unless it says it lacks one.
#[derive(Clone, Debug)]
pub struct Saved {
    pub path: PathBuf,
    pub picture: bool,
    pub sound: bool,
    /// The year it was put online, when the site said.
    pub year: Option<u32>,
    /// The channel or person who put it online, when the site said.
    pub uploader: Option<String>,
    pub title: String,
}

fn parse_file(line: &str) -> Option<Saved> {
    let mut it = line.strip_prefix(FILE)?.trim_start().splitn(6, '\t');
    let path = PathBuf::from(it.next()?);
    let vcodec = it.next()?;
    let acodec = it.next()?;
    let known = |s: &str| (s != "NA" && !s.is_empty()).then(|| s.to_string());
    let year = it
        .next()
        .and_then(known)
        .and_then(|d| d.get(..4).and_then(|y| y.parse().ok()));
    let uploader = it.next().and_then(known);
    Some(Saved {
        path,
        picture: vcodec != "none",
        sound: acodec != "none",
        year,
        uploader,
        title: it.next().unwrap_or_default().to_string(),
    })
}

/// Downloads `url` into `dir`: the files yt-dlp saved, one or two. None if
/// stopped.
pub fn download(
    exe: &Path,
    url: &str,
    dir: &Path,
    format: Format,
    lines: u32,
    stop: &AtomicBool,
    progress: &mut dyn FnMut(Progress),
) -> Result<Option<Vec<Saved>>, String> {
    let no_ffmpeg = dir.join("no-ffmpeg");
    std::fs::create_dir_all(&no_ffmpeg).map_err(|e| format!("{}: {e}", no_ffmpeg.display()))?;
    let args = args(url, dir, &no_ffmpeg, format, lines);
    let mut errors = Vec::new();
    // YouTube now and then answers 403 for a moment; yt-dlp resumes where it was.
    for _attempt in 0..3 {
        let mut saved: Vec<Saved> = Vec::new();
        let result = run(exe, &args, stop, &mut |l| {
            if let Some(p) = parse_progress(l) {
                progress(p);
            } else if let Some(file) = parse_file(l) {
                saved.push(file);
            }
        })?;
        let Some((ok, errs)) = result else {
            return Ok(None);
        };
        if ok && saved.iter().any(|s| s.sound) {
            return Ok(Some(saved));
        }
        let forbidden = errs.iter().any(|e| e.contains("403"));
        errors = errs;
        if !forbidden {
            break;
        }
    }
    Err(explain(&errors, &site(url)))
}

/// Which captions to save: the track's name and whether they are automatic.
/// A person's come first: in the video's own language, then English. Then the
/// automatic captions of what is said (`en-orig`), never YouTube's machine
/// translations; a video with dubbed sound has one `-orig` track for each
/// language, so the video's own is chosen, as `en-US` for an `en-orig`. A
/// person's in another language come last, as they may be a translation.
fn choose_captions(
    language: Option<&str>,
    made: &[String],
    automatic: &[String],
) -> Option<(String, bool)> {
    let is = |c: &str, l: &str| c == l || c.starts_with(&format!("{l}-"));
    let own = language.and_then(|l| l.split('-').next());
    let real: Vec<&String> = made.iter().filter(|c| *c != "live_chat").collect();
    let person = own
        .and_then(|l| real.iter().find(|c| is(c, l)))
        .or_else(|| real.iter().find(|c| is(c, "en")));
    if let Some(c) = person {
        return Some((c.to_string(), false));
    }
    let orig: Vec<&String> = automatic.iter().filter(|c| c.ends_with("-orig")).collect();
    let machine = own
        .and_then(|l| orig.iter().find(|c| is(c, l)))
        .or_else(|| orig.iter().find(|c| is(c, "en")))
        .or_else(|| orig.first())
        .map(|c| c.as_str())
        .or_else(|| own.and_then(|l| automatic.iter().find(|c| *c == l).map(|c| c.as_str())));
    if let Some(c) = machine {
        return Some((c.to_string(), true));
    }
    real.first().map(|c| (c.to_string(), false))
}

/// A captions track's name as it goes in a file name: `en` for YouTube's
/// `en-orig` and `en-eEY6OEpapPo`, while `pt-BR` and `zh-Hans` stay as they are.
fn short_lang(track: &str) -> String {
    let parts: Vec<&str> = track
        .split('-')
        .take_while(|p| p.len() <= 4 && *p != "orig")
        .collect();
    if parts.is_empty() {
        track.to_string()
    } else {
        parts.join("-")
    }
}

/// What yt-dlp's `-J` answer says about a video's captions.
fn captions_in(v: &serde_json::Value) -> Option<(String, bool)> {
    let v = v["entries"].get(0).unwrap_or(v);
    let names = |k: &str| -> Vec<String> {
        v[k].as_object()
            .map(|o| o.keys().cloned().collect())
            .unwrap_or_default()
    };
    choose_captions(
        v["language"].as_str(),
        &names("subtitles"),
        &names("automatic_captions"),
    )
}

/// Fetches `url`'s captions into `dir`: the file, and its language as it goes
/// in a file name, such as `en`. None when it has none, or if stopped. Asked
/// for apart from the download, as YouTube sometimes refuses captions and that
/// must not cost the video.
pub fn captions(
    exe: &Path,
    url: &str,
    dir: &Path,
    stop: &AtomicBool,
) -> Result<Option<(PathBuf, String)>, String> {
    let strings = |a: &[&str]| a.iter().map(|s| s.to_string()).collect::<Vec<String>>();
    let args = strings(&[
        "--ignore-config",
        "--encoding",
        "utf-8",
        "--no-playlist",
        "--playlist-items",
        "1",
        "--no-colors",
        "--skip-download",
        "-J",
        "--",
        url,
    ]);
    let mut json = String::new();
    let Some((ok, errors)) = run(exe, &args, stop, &mut |l| json.push_str(l))? else {
        return Ok(None);
    };
    if !ok {
        return Err(last_error(&errors));
    }
    let v: serde_json::Value = serde_json::from_str(&json).map_err(|e| e.to_string())?;
    let Some((lang, automatic)) = captions_in(&v) else {
        return Ok(None);
    };
    let name = key(url).unwrap_or_else(|| "video".into());
    let out = dir.display().to_string();
    let template = format!("{name}.captions.%(ext)s");
    let args = strings(&[
        "--ignore-config",
        "--encoding",
        "utf-8",
        "--no-playlist",
        "--playlist-items",
        "1",
        "--no-colors",
        "--skip-download",
        if automatic {
            "--write-auto-subs"
        } else {
            "--write-subs"
        },
        "--sub-langs",
        &lang,
        "--sub-format",
        "vtt/srt/best",
        "-P",
        &out,
        "-o",
        &template,
        "--",
        url,
    ]);
    let Some((ok, errors)) = run(exe, &args, stop, &mut |_| {})? else {
        return Ok(None);
    };
    let prefix = format!("{name}.captions.");
    let file = std::fs::read_dir(dir)
        .into_iter()
        .flatten()
        .flatten()
        .find(|e| e.file_name().to_string_lossy().starts_with(&prefix))
        .map(|e| e.path());
    match file {
        Some(f) => Ok(Some((f, short_lang(&lang)))),
        None if ok => Ok(None),
        None => Err(last_error(&errors)),
    }
}

/// A playlist, or a channel's videos, as yt-dlp lists them.
#[derive(Clone, Debug, PartialEq)]
pub struct Playlist {
    /// The site's own id for it, such as YouTube's "PL..." list id.
    pub id: String,
    pub title: String,
    pub videos: Vec<Item>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct Item {
    /// The site's id for the video, to know it next time.
    pub id: String,
    pub url: String,
    pub title: String,
}

/// What an address holds: one video, or a list of them. None if stopped.
pub fn probe(exe: &Path, url: &str, stop: &AtomicBool) -> Result<Option<Option<Playlist>>, String> {
    let args: Vec<String> = [
        "--ignore-config",
        "--encoding",
        "utf-8",
        "--flat-playlist",
        "--yes-playlist",
        "--no-colors",
        "-J",
        "--",
        url,
    ]
    .iter()
    .map(|s| s.to_string())
    .collect();
    let mut json = String::new();
    let Some((ok, errors)) = run(exe, &args, stop, &mut |l| json.push_str(l))? else {
        return Ok(None);
    };
    if !ok {
        return Err(explain(&errors, &site(url)));
    }
    let v: serde_json::Value = serde_json::from_str(&json)
        .map_err(|e| format!("yt-dlp's list of what is at {url} could not be read: {e}"))?;
    Ok(Some(playlist_of(&v)))
}

/// The title of what is at `url`: the video's, or with `whole` the playlist
/// it is in, for the queue (log entry 2, Q2). None if stopped or not known.
/// A YouTube video's comes from YouTube's oEmbed answer, in a fraction of a
/// second; yt-dlp takes some 15 seconds for one video.
pub fn title(exe: &Path, url: &str, whole: bool, stop: &AtomicBool) -> Option<String> {
    if let Some(id) = video_id(url).filter(|_| is_youtube(url) && !whole) {
        let path = format!("/oembed?format=json&url=https://www.youtube.com/watch?v={id}");
        let found = crate::net::download("www.youtube.com", &path, stop, &mut |_, _| {})
            .ok()
            .flatten()
            .and_then(|body| serde_json::from_slice::<serde_json::Value>(&body).ok())
            .and_then(|v| v["title"].as_str().map(|t| t.trim().to_string()))
            .filter(|t| !t.is_empty());
        if found.is_some() {
            return found;
        }
    }
    let args: Vec<String> = [
        "--ignore-config",
        "--encoding",
        "utf-8",
        "--flat-playlist",
        if whole {
            "--yes-playlist"
        } else {
            "--no-playlist"
        },
        "--no-colors",
        "-J",
        "--",
        url,
    ]
    .iter()
    .map(|s| s.to_string())
    .collect();
    let mut json = String::new();
    let (ok, _) = run(exe, &args, stop, &mut |l| json.push_str(l)).ok()??;
    if !ok {
        return None;
    }
    let v: serde_json::Value = serde_json::from_str(&json).ok()?;
    v["title"]
        .as_str()
        .map(|t| t.trim().to_string())
        .filter(|t| !t.is_empty())
}

/// A list from yt-dlp's `-J --flat-playlist` answer; None for one video.
fn playlist_of(v: &serde_json::Value) -> Option<Playlist> {
    if v["_type"].as_str() != Some("playlist") {
        return None;
    }
    let text = |x: &serde_json::Value| x.as_str().unwrap_or_default().to_string();
    let videos: Vec<Item> = v["entries"]
        .as_array()?
        .iter()
        .filter_map(|e| {
            let url = e["url"]
                .as_str()
                .or(e["webpage_url"].as_str())
                .filter(|u| u.starts_with("http"))?;
            Some(Item {
                id: e["id"].as_str().unwrap_or(url).to_string(),
                url: url.to_string(),
                title: text(&e["title"]),
            })
        })
        .collect();
    Some(Playlist {
        id: text(&v["id"]),
        title: text(&v["title"]),
        videos,
    })
}

/// The name a download's files go by while they are fetched: a YouTube
/// video's id, or for another address its site and a short hash of it. No
/// dots, as files are matched to their video by what comes before the first.
pub fn key(url: &str) -> Option<String> {
    if is_youtube(url) {
        if let Some(id) = video_id(url) {
            return Some(id);
        }
    }
    let site: String = site(url)
        .chars()
        .map(|c| if c.is_ascii_alphanumeric() { c } else { '-' })
        .collect();
    if site.is_empty() {
        return None;
    }
    let hash = net::hex(&net::sha256(url.trim().as_bytes()));
    Some(format!("{site}-{}", &hash[..10]))
}

/// The 11-character video id in a YouTube address, if there is one.
pub fn video_id(url: &str) -> Option<String> {
    let ok = |c: char| c.is_ascii_alphanumeric() || c == '-' || c == '_';
    let after = ["v=", "youtu.be/", "/shorts/", "/live/", "/embed/"]
        .iter()
        .find_map(|m| url.find(m).map(|i| &url[i + m.len()..]))?;
    let id: String = after.chars().take_while(|&c| ok(c)).collect();
    (id.len() == 11).then_some(id)
}

/// The playlist id in a YouTube address (`list=`), if there is one.
pub fn list_id(url: &str) -> Option<String> {
    let ok = |c: char| c.is_ascii_alphanumeric() || c == '-' || c == '_';
    let i = url.find("list=")?;
    let id: String = url[i + 5..].chars().take_while(|&c| ok(c)).collect();
    (!id.is_empty()).then_some(id)
}

/// A mix is YouTube's endless list made for one video ("RD..."), not a
/// playlist anyone made: an address with one is taken as the video alone.
pub fn is_mix(list: &str) -> bool {
    list.starts_with("RD")
}

/// Whether `url` is a YouTube address.
pub fn is_youtube(url: &str) -> bool {
    let host = host(url);
    host == "youtube.com" || host == "youtu.be" || host.ends_with(".youtube.com")
}

/// The address's host, lower case, without "www." or "m.".
fn host(url: &str) -> String {
    let lower = url.trim().to_ascii_lowercase();
    let rest = lower
        .trim_start_matches("https://")
        .trim_start_matches("http://");
    let host = rest.split(['/', '?', '#']).next().unwrap_or_default();
    let host = host.rsplit('@').next().unwrap_or_default();
    let host = host.split(':').next().unwrap_or_default();
    host.trim_start_matches("www.")
        .trim_start_matches("m.")
        .to_string()
}

/// The site, as said to the user: "YouTube", or the address's host.
pub fn site(url: &str) -> String {
    if is_youtube(url) {
        "YouTube".into()
    } else {
        host(url)
    }
}

/// Whether `text` looks like a web address without its "https://", such
/// as "vimeo.com/123": a host with a dot, then a slash, and no spaces.
fn bare_address(text: &str) -> bool {
    let Some((host, _)) = text.split_once('/') else {
        return false;
    };
    host.contains('.')
        && !host.contains(['\\', ':', ' '])
        && host
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '.' || c == '-')
}

/// The address box's text as a web address, or what is wrong with it and
/// how to put it right.
pub fn parse_address(text: &str) -> Result<String, String> {
    let text = text.trim().trim_matches('"').trim();
    if text.is_empty() {
        return Err(
            "The web address box is empty. Copy the address of a video or a playlist from the \
             browser's address bar, with Alt+D then Ctrl+C, and paste it here with Ctrl+V."
                .into(),
        );
    }
    let lower = text.to_ascii_lowercase();
    if lower.starts_with("http://") || lower.starts_with("https://") {
        if host(text).is_empty() {
            return Err(format!(
                "{text} is not a whole web address. Copy it again from the browser's address \
                 bar, with Alt+D then Ctrl+C."
            ));
        }
        return Ok(text.to_string());
    }
    // "youtu.be/..." or "www.example.com/...", typed without "https://".
    if lower.starts_with("www.") || bare_address(text) {
        return Ok(format!("https://{text}"));
    }
    Err(format!(
        "{text} is not a web address. Copy the address of a video or a playlist from the \
         browser's address bar, with Alt+D then Ctrl+C."
    ))
}

/// The last thing yt-dlp said went wrong.
fn last_error(errors: &[String]) -> String {
    errors
        .iter()
        .rev()
        .find(|e| e.starts_with("ERROR"))
        .or(errors.last())
        .map(|s| s.trim_start_matches("ERROR:").trim().to_string())
        .unwrap_or_else(|| "it stopped without saying why".into())
}

/// yt-dlp's last error, in words a person can act on.
fn explain(errors: &[String], site: &str) -> String {
    let last = last_error(errors);
    let lower = last.to_lowercase();
    let hint = if lower.contains("requested format is not available") && site != "YouTube" {
        // Our formats ask for plain HTTP only (formats()).
        " This site offers the video only as a stream in pieces, which VidFetch cannot put \
         together without FFmpeg. Look for a download link to the video file itself, and paste \
         that instead."
    } else if lower.contains("unsupported url") {
        " yt-dlp found no video at that address. Open the video itself in the browser and copy \
         its address."
    } else if lower.contains("private video")
        || lower.contains("sign in")
        || lower.contains("logged-in")
        || lower.contains("cookies")
    {
        " The video may be private, age-restricted or need signing in, which VidFetch cannot do. \
         If YouTube asks to confirm you are not a bot, that usually passes: try again later."
    } else if lower.contains("unavailable") || lower.contains("not available") {
        " The video may have been removed, or not be available in this country."
    } else if lower.contains("unable to download") || lower.contains("getaddrinfo") {
        " Check the internet connection, and try again."
    } else {
        " yt-dlp may need updating: video sites change often. Choose Update yt-dlp in the File \
         menu, then try again."
    };
    format!("The video from {site} could not be downloaded. yt-dlp said: {last}{hint}")
}

/// Deletes the files of one download from `dir`.
pub fn forget(dir: &Path, key: &str) {
    let Ok(list) = std::fs::read_dir(dir) else {
        return;
    };
    for e in list.flatten() {
        if e.file_name()
            .to_string_lossy()
            .starts_with(&format!("{key}."))
        {
            let _ = std::fs::remove_file(e.path());
        }
    }
}

/// Deletes downloads left in `dir` for more than a day: a stopped download
/// is kept that long, so starting it again carries on where it was.
pub fn tidy(dir: &Path) {
    let Ok(list) = std::fs::read_dir(dir) else {
        return;
    };
    let day = Duration::from_secs(24 * 3600);
    for e in list.flatten() {
        let old = e
            .metadata()
            .ok()
            .filter(|m| m.is_file())
            .and_then(|m| m.modified().ok())
            .and_then(|t| t.elapsed().ok())
            .is_some_and(|age| age > day);
        if old {
            let _ = std::fs::remove_file(e.path());
        }
    }
}

/// One MP4 box: (type, start of box, start of body, end).
type Mp4Box = ([u8; 4], usize, usize, usize);

fn boxes(data: &[u8], start: usize, end: usize) -> Vec<Mp4Box> {
    let mut out = Vec::new();
    let mut pos = start;
    while pos + 8 <= end {
        let size32 = u32::from_be_bytes(data[pos..pos + 4].try_into().unwrap()) as usize;
        let kind: [u8; 4] = data[pos + 4..pos + 8].try_into().unwrap();
        let (size, body) = match size32 {
            1 if pos + 16 <= end => (
                u64::from_be_bytes(data[pos + 8..pos + 16].try_into().unwrap()) as usize,
                pos + 16,
            ),
            0 => (end - pos, pos + 8),
            n => (n, pos + 8),
        };
        if size < 8 || pos + size > end {
            break;
        }
        out.push((kind, pos, body, pos + size));
        pos += size;
    }
    out
}

/// YouTube's separate picture file carries an edit list (`edts`) that shifts
/// it by the B-frame delay, and Media Foundation's MP4 reader then delivers
/// no frames at all (Video Scribe, log entry 13). In our own downloaded copy
/// the box is renamed `free`, which every reader skips. Returns the shift in
/// seconds, so that a time t on the video's clock is t + shift to Media
/// Foundation.
pub fn drop_edit_list(path: &Path) -> Result<f64, String> {
    use std::io::{Read, Seek, SeekFrom, Write};
    let fail = |e: std::io::Error| format!("The downloaded picture could not be prepared: {e}");
    let mut f = std::fs::OpenOptions::new()
        .read(true)
        .write(true)
        .open(path)
        .map_err(fail)?;
    // The moov sits at the front of these files; 64 KB is plenty.
    let mut head = vec![0u8; 65_536];
    let n = f.read(&mut head).map_err(fail)?;
    head.truncate(n);
    let Some(moov) = boxes(&head, 0, head.len())
        .into_iter()
        .find(|b| &b.0 == b"moov")
    else {
        return Ok(0.0);
    };
    let mut shift = 0.0;
    for trak in boxes(&head, moov.2, moov.3)
        .into_iter()
        .filter(|b| &b.0 == b"trak")
    {
        let inner = boxes(&head, trak.2, trak.3);
        let elst_in = |b: &Mp4Box| boxes(&head, b.2, b.3).into_iter().find(|c| &c.0 == b"elst");
        let Some((edts, elst)) = inner
            .iter()
            .filter(|b| &b.0 == b"edts" || &b.0 == b"free")
            .find_map(|b| elst_in(b).map(|e| (*b, e)))
        else {
            continue;
        };
        let timescale = inner
            .iter()
            .find(|b| &b.0 == b"mdia")
            .and_then(|m| boxes(&head, m.2, m.3).into_iter().find(|b| &b.0 == b"mdhd"))
            .map(|h| {
                let at = if head[h.2] == 1 { h.2 + 20 } else { h.2 + 12 };
                u32::from_be_bytes(head[at..at + 4].try_into().unwrap())
            })
            .unwrap_or(0);
        let v = head[elst.2];
        let count = u32::from_be_bytes(head[elst.2 + 4..elst.2 + 8].try_into().unwrap());
        if count >= 1 && timescale > 0 {
            let e = elst.2 + 8;
            let media_time = if v == 1 {
                i64::from_be_bytes(head[e + 8..e + 16].try_into().unwrap())
            } else {
                i32::from_be_bytes(head[e + 4..e + 8].try_into().unwrap()) as i64
            };
            if media_time > 0 {
                shift = media_time as f64 / timescale as f64;
            }
        }
        if &edts.0 == b"edts" {
            f.seek(SeekFrom::Start(edts.1 as u64 + 4)).map_err(fail)?;
            f.write_all(b"free").map_err(fail)?;
        }
    }
    Ok(shift)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn progress_lines_parse() {
        assert_eq!(
            parse_progress("VFPROGRESS none 1024 4096 NA"),
            Some(Progress {
                what: "sound",
                done: 1024,
                total: Some(4096)
            })
        );
        assert_eq!(
            parse_progress("VFPROGRESS avc1.4d401e 10 NA 5000.5")
                .unwrap()
                .total,
            Some(5000)
        );
        assert_eq!(
            parse_progress("VFPROGRESS NA 10 NA NA").unwrap().what,
            "video"
        );
        assert_eq!(parse_progress("[download] 10%"), None);
    }

    #[test]
    fn the_url_comes_after_the_options_end() {
        let a = args(
            "-o evil",
            Path::new("out"),
            Path::new("none"),
            Format::Video,
            720,
        );
        let i = a.iter().position(|s| s == "--").unwrap();
        assert_eq!(a[i + 1], "-o evil");
        assert_eq!(i + 2, a.len());
    }

    #[test]
    fn formats_follow_the_choice() {
        assert!(formats(Format::Video, 720).contains("[height<=720]"));
        assert!(formats(Format::Video, 720).starts_with("(bv*[vcodec^=avc1]"));
        assert!(formats(Format::Mp3, 720).starts_with("ba[ext=m4a]"));
        assert!(!formats(Format::M4a, 1080).contains("height"));
    }

    #[test]
    fn saved_files_say_what_they_hold() {
        let f = |v: &str, a: &str| {
            parse_file(&format!(
                "{FILE} C:\\d\\x.mp4\t{v}\t{a}\t20240131\tBlender Studio\tSprite Fright"
            ))
            .unwrap()
        };
        let both = f("NA", "NA");
        assert!(both.picture && both.sound);
        assert_eq!(both.title, "Sprite Fright");
        assert_eq!(both.year, Some(2024));
        assert_eq!(both.uploader.as_deref(), Some("Blender Studio"));
        assert!(!f("none", "mp4a.40.2").picture);
        assert!(!f("avc1.4D401E", "none").sound);
        let unknown = parse_file(&format!("{FILE} C:\\d\\x.mp4\tNA\tNA\tNA\tNA\tA title")).unwrap();
        assert_eq!((unknown.year, unknown.uploader), (None, None));
        assert!(parse_file("[download] x").is_none());
    }

    #[test]
    fn the_right_captions_are_chosen() {
        let v = |a: &[&str]| a.iter().map(|s| s.to_string()).collect::<Vec<_>>();
        let auto = v(&["af", "en", "fr", "fr-orig"]);
        // A person's, in the video's language, before anything automatic.
        assert_eq!(
            choose_captions(Some("fr"), &v(&["en", "fr-CA"]), &auto),
            Some(("fr-CA".into(), false))
        );
        // Then English, then whichever there is.
        assert_eq!(
            choose_captions(Some("de"), &v(&["es", "en-GB"]), &[]),
            Some(("en-GB".into(), false))
        );
        assert_eq!(
            choose_captions(None, &v(&["live_chat", "es"]), &[]),
            Some(("es".into(), false))
        );
        // Automatic: what is said, never a translation.
        assert_eq!(
            choose_captions(
                Some("en-US"),
                &[],
                &v(&["ar-orig", "en-orig", "fr-orig", "ar"])
            ),
            Some(("en-orig".into(), true))
        );
        // A person's translation does not beat the words actually said.
        assert_eq!(
            choose_captions(Some("en"), &v(&["ar"]), &v(&["en-orig"])),
            Some(("en-orig".into(), true))
        );
        assert_eq!(
            choose_captions(Some("en"), &v(&["live_chat"]), &auto),
            Some(("fr-orig".into(), true))
        );
        assert_eq!(
            choose_captions(Some("en"), &[], &v(&["af", "en"])),
            Some(("en".into(), true))
        );
        assert_eq!(choose_captions(None, &[], &v(&["af", "en"])), None);
        assert_eq!(choose_captions(Some("en"), &[], &[]), None);
        assert_eq!(short_lang("en-orig"), "en");
        assert_eq!(short_lang("en-eEY6OEpapPo"), "en");
        assert_eq!(short_lang("pt-BR"), "pt-BR");
        assert_eq!(short_lang("zh-Hans"), "zh-Hans");
    }

    #[test]
    fn the_checksum_list_is_read() {
        let sums = "aa  yt-dlp_linux\n\
            9CCDACC4719D8AA7E45A77536BFABEC45A03BA1F2FAD5E241AB4060B24238A85  yt-dlp.exe\n";
        assert_eq!(
            expected_hash(sums, "yt-dlp.exe").unwrap(),
            "9ccdacc4719d8aa7e45a77536bfabec45a03ba1f2fad5e241ab4060b24238a85"
        );
        assert_eq!(expected_hash(sums, "yt-dlp_x86.exe"), None);
    }

    #[test]
    fn ids_are_found_in_every_kind_of_address() {
        for url in [
            "https://www.youtube.com/watch?v=_cMxraX_5RE",
            "https://youtube.com/watch?feature=share&v=_cMxraX_5RE&t=10",
            "https://youtu.be/_cMxraX_5RE?si=abc",
            "https://www.youtube.com/shorts/_cMxraX_5RE",
        ] {
            assert_eq!(video_id(url).as_deref(), Some("_cMxraX_5RE"), "{url}");
        }
        let both = "https://www.youtube.com/watch?v=_cMxraX_5RE&list=PLx0sYbCqOb8&index=2";
        assert_eq!(list_id(both).as_deref(), Some("PLx0sYbCqOb8"));
        assert_eq!(
            list_id("https://www.youtube.com/playlist?list=PLx0sYbCqOb8").as_deref(),
            Some("PLx0sYbCqOb8")
        );
        assert_eq!(list_id("https://youtu.be/_cMxraX_5RE"), None);
        assert!(is_mix("RD_cMxraX_5RE") && !is_mix("PLx0sYbCqOb8"));
        let page = "https://archive.org/details/BigBuckBunny_124";
        let k = key(page).unwrap();
        assert!(k.starts_with("archive-org-") && !k.contains('.'), "{k}");
        // A playlist page has no video id: it is named like another site's.
        assert!(key("https://www.youtube.com/playlist?list=PLx").is_some());
    }

    #[test]
    fn addresses_are_accepted_or_explained() {
        assert_eq!(
            parse_address("  youtu.be/_cMxraX_5RE "),
            Ok("https://youtu.be/_cMxraX_5RE".into())
        );
        assert!(parse_address("").unwrap_err().contains("empty"));
        assert!(parse_address("C:\\Videos\\a.mp4")
            .unwrap_err()
            .contains("not a web address"));
    }

    #[test]
    fn a_playlist_is_read_from_ytdlps_answer() {
        let v: serde_json::Value = serde_json::from_str(
            r#"{"_type":"playlist","id":"PLx","title":"Open movies","entries":[
                {"_type":"url","id":"aaaaaaaaaaa","url":"https://www.youtube.com/watch?v=aaaaaaaaaaa","title":"One"},
                {"_type":"url","id":"bbbbbbbbbbb","url":"https://www.youtube.com/watch?v=bbbbbbbbbbb","title":"Two"},
                {"_type":"url","id":"x"}]}"#,
        )
        .unwrap();
        let p = playlist_of(&v).unwrap();
        assert_eq!(p.title, "Open movies");
        assert_eq!(p.videos.len(), 2);
        assert_eq!(p.videos[1].title, "Two");
        let one: serde_json::Value =
            serde_json::from_str(r#"{"_type":"video","id":"aaaaaaaaaaa","title":"One"}"#).unwrap();
        assert_eq!(playlist_of(&one), None);
    }

    #[test]
    fn errors_are_explained() {
        let said = |e: &str, site: &str| explain(&[format!("ERROR: [x] 1: {e}")], site);
        assert!(said("Requested format is not available.", "dailymotion.com").contains("pieces"));
        assert!(said("Private video. Sign in", "YouTube").contains("signing in"));
    }
}
