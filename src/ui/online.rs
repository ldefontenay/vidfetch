//! What the main list shows from YouTube, over the downloads: search
//! results, what is new from the channels followed, those channels, and one
//! channel's latest videos (log entry 3, plans 1 and 2). One list, one set of
//! keys, whatever it shows: Enter plays, Ctrl+D downloads, the context menu
//! has the rest, and Escape or Backspace goes back one list at a time, as in
//! File Explorer (Q2: "Yes. Also great that you thought of implementing
//! backspace.").
//!
//! Everything online is looked up on a thread of its own, so the window
//! stays free; one lookup at a time, and Escape stops it.

use super::*;
use crate::channels::{self, Channel};
use crate::history::{self, Played};
use crate::places;
use crate::youtube::{self, Video};

/// A lookup has finished: its LPARAM is a boxed `(number, Found)`.
pub const WM_FOUND: u32 = WM_APP + 22;

/// How many channels' feeds are fetched at once.
const AT_ONCE: usize = 8;

pub enum Row {
    /// A video, and whether it is new since the user last looked.
    Video {
        v: Video,
        new: bool,
    },
    Channel(Channel),
    /// The key to YouTube's next page of results.
    More(String),
    /// Something played lately.
    Played(Played),
}

impl Row {
    /// The line in the list; `channel`: say each video's channel, which a
    /// channel's own list does not need.
    fn line(&self, channel: bool) -> String {
        match self {
            Row::Video { v, new } => {
                let line = if channel {
                    v.line()
                } else {
                    Video {
                        channel: String::new(),
                        ..v.clone()
                    }
                    .line()
                };
                if *new {
                    format!("New: {line}")
                } else {
                    line
                }
            }
            Row::Channel(c) => format!("{}: YouTube channel", c.name),
            Row::Played(p) => {
                let what = if p.video().is_some() {
                    "on YouTube"
                } else if std::path::Path::new(&p.key).exists() {
                    "downloaded"
                } else {
                    "a download no longer there"
                };
                let place = match places::get(&p.key) {
                    Some(at) => format!(", stopped at {}", listen::clock(at)),
                    None => String::new(),
                };
                format!(
                    "{}: {what}{place}, played {}",
                    p.name,
                    youtube::ago(youtube::now(), p.when)
                )
            }
            Row::More(_) => "More results: Enter fetches the next page.".into(),
        }
    }
}

pub enum Shows {
    Search(String),
    New,
    Channels,
    Channel(Channel),
    History,
}

/// One list from YouTube, and which row was chosen when it was left.
pub struct Online {
    pub shows: Shows,
    pub rows: Vec<Row>,
    pub at: usize,
}

pub enum Found {
    Search {
        query: String,
        more: bool,
        result: Result<youtube::Results, String>,
        slow: bool,
    },
    New {
        videos: Vec<(Video, bool)>,
        failed: Vec<String>,
    },
    Channel {
        channel: Channel,
        result: Result<Vec<Video>, String>,
    },
    Sound {
        video: Video,
        result: Result<String, String>,
    },
    Follow(Result<(String, String), String>),
    Describe {
        name: String,
        result: Result<String, String>,
    },
}

/// The list's label for what it shows: always with W as its letter.
pub fn label(o: &Online) -> String {
    match &o.shows {
        Shows::Search(q) => format!("&What YouTube found for {q}:"),
        Shows::New => "New from your channels, ne&west first:".into(),
        Shows::Channels => "Channels you follo&w:".into(),
        Shows::Channel(c) => format!("Latest videos from {}, ne&west first:", c.name),
        Shows::History => "&What you played lately, newest first:".into(),
    }
}

/// The Enter button's words for the row chosen.
pub fn enter_words(app: &App, at: Option<usize>) -> &'static str {
    match top_row(app, at) {
        Some(Row::Channel(_)) => "O&pen the channel's videos  Enter",
        Some(Row::More(_)) => "Fetch more results, the next &page  Enter",
        _ => "&Play  Enter",
    }
}

fn top_row(app: &App, at: Option<usize>) -> Option<&Row> {
    app.online.last().and_then(|o| o.rows.get(at?))
}

fn chosen_row(hwnd: HWND, app: &App) -> Option<(usize, &Row)> {
    let at = downloads::chosen(list(hwnd))?;
    Some((at, top_row(app, Some(at))?))
}

fn count_of(n: usize, one: &str, many: &str) -> String {
    if n == 1 {
        format!("1 {one}")
    } else {
        format!("{n} {many}")
    }
}

/// Starts `work` on a thread of its own, in place of any lookup going on.
fn look<F>(hwnd: HWND, app: &mut App, work: F)
where
    F: FnOnce(&AtomicBool) -> Option<Found> + Send + 'static,
{
    if let Some((_, stop)) = app.lookup.take() {
        stop.store(true, Ordering::Relaxed);
    }
    app.next_lookup += 1;
    let id = app.next_lookup;
    let stop = Arc::new(AtomicBool::new(false));
    app.lookup = Some((id, stop.clone()));
    let to = hwnd.0 as isize;
    std::thread::spawn(move || {
        let Some(found) = work(&stop) else { return };
        if stop.load(Ordering::Relaxed) {
            return;
        }
        let raw = Box::into_raw(Box::new((id, found)));
        unsafe {
            if PostMessageW(
                Some(HWND(to as *mut _)),
                WM_FOUND,
                WPARAM(0),
                LPARAM(raw as isize),
            )
            .is_err()
            {
                drop(Box::from_raw(raw));
            }
        }
    });
}

/// Escape while something is being looked up: stops it. False if nothing was.
pub fn stop_looking(hwnd: HWND, app: &mut App) -> bool {
    let Some((_, stop)) = app.lookup.take() else {
        return false;
    };
    stop.store(true, Ordering::Relaxed);
    say(hwnd, "Stopped looking.");
    true
}

/// Shows `o` in the list, on top of what was shown, or in place of it when
/// `replace`; says `said`.
fn push(hwnd: HWND, app: &mut App, o: Online, replace: bool, said: &str) {
    keep_place(hwnd, app);
    if replace {
        app.online.pop();
    }
    app.online.push(o);
    show_top(hwnd, app);
    say(hwnd, said);
    focus(hwnd, IDC_LIST);
}

/// Remembers which row is chosen in the list shown, to come back to it.
fn keep_place(hwnd: HWND, app: &mut App) {
    if let (Some(o), Some(at)) = (app.online.last_mut(), downloads::chosen(list(hwnd))) {
        o.at = at;
    }
}

/// Fills the list with the online list on top.
pub fn show_top(hwnd: HWND, app: &App) {
    let Some(o) = app.online.last() else { return };
    let channel = !matches!(o.shows, Shows::Channel(_));
    let lines: Vec<String> = o.rows.iter().map(|r| r.line(channel)).collect();
    downloads::fill_lines(list(hwnd), &lines, o.at);
    name_list(hwnd, app);
    name_enter(hwnd, app);
}

/// Back one list. False if no online list was shown.
pub fn back(hwnd: HWND, app: &mut App) -> bool {
    if app.online.pop().is_none() {
        return false;
    }
    if app.online.is_empty() {
        name_list(hwnd, app);
        refresh(hwnd, app);
        say(hwnd, "Back to your downloads.");
    } else {
        show_top(hwnd, app);
        let l = app.online.last().map(label).unwrap_or_default();
        say(
            hwnd,
            &format!("Back to {}", l.replace('&', "").trim_end_matches(':')),
        );
    }
    focus(hwnd, IDC_LIST);
    true
}

/// A lookup has finished.
pub fn found(hwnd: HWND, app: &mut App, lparam: LPARAM) {
    let (id, found) = *unsafe { Box::from_raw(lparam.0 as *mut (u64, Found)) };
    if app.lookup.as_ref().is_none_or(|(n, _)| *n != id) {
        return;
    }
    app.lookup = None;
    match found {
        Found::Search {
            query,
            more,
            result,
            slow,
        } => searched(hwnd, app, query, more, result, slow),
        Found::New { videos, failed } => new_found(hwnd, app, videos, failed),
        Found::Channel { channel, result } => match result {
            Ok(videos) => {
                let followed = app.channels.iter().any(|c| c.id == channel.id);
                let said = format!(
                    "The latest {} from {}, newest first.{}",
                    count_of(videos.len(), "video", "videos"),
                    channel.name,
                    if followed {
                        ""
                    } else {
                        " You do not follow this channel; the context menu can."
                    }
                );
                let replace =
                    matches!(app.online.last().map(|o| &o.shows), Some(Shows::Channel(_)));
                let rows = videos
                    .into_iter()
                    .map(|v| Row::Video { v, new: false })
                    .collect();
                push(
                    hwnd,
                    app,
                    Online {
                        shows: Shows::Channel(channel),
                        rows,
                        at: 0,
                    },
                    replace,
                    &said,
                );
            }
            Err(why) => problem(
                hwnd,
                &format!("The videos of {} could not be listed: {why}", channel.name),
            ),
        },
        Found::Sound { video, result } => match result {
            Ok(address) if in_front(hwnd) => play_online(hwnd, app, video, address),
            Ok(_) => say(
                hwnd,
                &format!(
                    "The sound of {} is ready, but another VidFetch window is open. Press Enter \
                     on it again once that is closed.",
                    video.title
                ),
            ),
            Err(why) => problem(
                hwnd,
                &format!("The sound of {} could not be played: {why}", video.title),
            ),
        },
        Found::Follow(result) => match result {
            Ok((id, name)) => follow(hwnd, app, id, name),
            Err(why) => problem(hwnd, &format!("No channel was followed: {why}")),
        },
        Found::Describe { name, result } => match result {
            Ok(text) => {
                message::description(hwnd, &text);
                focus(hwnd, IDC_LIST);
            }
            Err(why) => problem(
                hwnd,
                &format!("The description of {name} could not be fetched: {why}"),
            ),
        },
    }
}

fn problem(hwnd: HWND, text: &str) {
    set_text(hwnd, IDC_STATUS, "Stopped by a problem.");
    message::problem(hwnd, text);
    focus(hwnd, IDC_LIST);
}

// ---------------------------------------------------------------- searching

fn check_query(text: &str) -> Option<String> {
    text.trim()
        .is_empty()
        .then(|| "Type what to search for, such as Blender open movie.".to_string())
}

/// Ctrl+F: asks what to search for, then searches.
pub fn search(hwnd: HWND, app: &mut App) {
    let Some(query) = crate::namebox::ask(
        hwnd,
        "Search YouTube - VidFetch",
        "&Search YouTube for:",
        &app.last_search,
        check_query,
    ) else {
        say(hwnd, "Nothing searched.");
        focus(hwnd, IDC_LIST);
        return;
    };
    let query = query.trim().to_string();
    app.last_search = query.clone();
    say(hwnd, &format!("Searching YouTube for {query}."));
    let exe = ytdlp::find();
    look(hwnd, app, move |stop| {
        let direct = youtube::search(&query, None, stop);
        if stop.load(Ordering::Relaxed) {
            return None;
        }
        let worked = direct.as_ref().is_ok_and(|r| !r.videos.is_empty());
        let (result, slow) = match (worked, exe) {
            (false, Some(exe)) => match ytdlp::search(&exe, &query, stop) {
                Ok(Some(videos)) => (Ok(youtube::Results { videos, more: None }), true),
                Ok(None) => return None,
                Err(e) => (direct.and(Err(e)), true),
            },
            _ => (direct, false),
        };
        Some(Found::Search {
            query,
            more: false,
            result,
            slow,
        })
    });
}

/// Enter on "More results".
fn more(hwnd: HWND, app: &mut App, token: String) {
    let query = match app.online.last().map(|o| &o.shows) {
        Some(Shows::Search(q)) => q.clone(),
        _ => return,
    };
    say(hwnd, "Fetching more results.");
    look(hwnd, app, move |stop| {
        let result = youtube::search(&query, Some(&token), stop);
        Some(Found::Search {
            query,
            more: true,
            result,
            slow: false,
        })
    });
}

fn searched(
    hwnd: HWND,
    app: &mut App,
    query: String,
    more: bool,
    result: Result<youtube::Results, String>,
    slow: bool,
) {
    let r = match result {
        Ok(r) => r,
        Err(why) => {
            problem(
                hwnd,
                &format!("YouTube could not be searched for {query}: {why}"),
            );
            return;
        }
    };
    if more {
        let Some(o) = app.online.last_mut() else {
            return;
        };
        o.rows.retain(|r| !matches!(r, Row::More(_)));
        let first = o.rows.len();
        let n = r.videos.len();
        o.rows
            .extend(r.videos.into_iter().map(|v| Row::Video { v, new: false }));
        if let Some(t) = r.more {
            o.rows.push(Row::More(t));
        }
        o.at = first;
        show_top(hwnd, app);
        say(
            hwnd,
            &format!(
                "{}, added at the end of the list.",
                count_of(n, "more video", "more videos")
            ),
        );
        focus(hwnd, IDC_LIST);
        return;
    }
    if r.videos.is_empty() {
        say(
            hwnd,
            &format!("YouTube found nothing for {query}. Ctrl+F searches again."),
        );
        return;
    }
    let n = r.videos.len();
    let mut rows: Vec<Row> = r
        .videos
        .into_iter()
        .map(|v| Row::Video { v, new: false })
        .collect();
    if let Some(t) = r.more {
        rows.push(Row::More(t));
    }
    let replace = matches!(app.online.last().map(|o| &o.shows), Some(Shows::Search(_)));
    let slow = if slow {
        " YouTube's own search did not answer, so yt-dlp searched instead."
    } else {
        ""
    };
    push(
        hwnd,
        app,
        Online {
            shows: Shows::Search(query.clone()),
            rows,
            at: 0,
        },
        replace,
        &format!(
            "{} found for {query}. Enter plays the sound; Ctrl+D downloads; Escape goes back.{slow}",
            count_of(n, "video", "videos")
        ),
    );
}

// ---------------------------------------------------------------- listening

/// Enter on a video: its sound from YouTube, in the Play window.
fn listen(hwnd: HWND, app: &mut App, v: Video) {
    let Some(exe) = ytdlp::find() else {
        say(
            hwnd,
            "Listening needs yt-dlp, which VidFetch fetches the first time you download \
             something. Press Ctrl+D to download this one.",
        );
        return;
    };
    say(
        hwnd,
        &format!(
            "Getting the sound of {} from YouTube. It takes about 5 seconds; Escape stops.",
            v.title
        ),
    );
    look(hwnd, app, move |stop| {
        let result = match ytdlp::sound_address(&exe, &v.url(), stop) {
            Ok(None) => return None,
            Ok(Some(a)) => Ok(a),
            Err(e) => Err(e),
        };
        Some(Found::Sound { video: v, result })
    });
}

/// Whether there is a video before and after the one chosen.
fn neighbours(hwnd: HWND, app: &App) -> (bool, bool) {
    let Some(o) = app.online.last() else {
        return (false, false);
    };
    let at = downloads::chosen(list(hwnd)).unwrap_or(0);
    let video = |r: &Row| matches!(r, Row::Video { .. } | Row::Played(_));
    (
        o.rows.iter().skip(at + 1).any(video),
        o.rows.iter().take(at).any(video),
    )
}

fn play_online(hwnd: HWND, app: &mut App, video: Video, address: String) {
    let (has_next, has_previous) = neighbours(hwnd, app);
    history::record(&places::of_video(&video.id), &video.title);
    let mut download = || download_video(hwnd, &video, true);
    let mut l = listen::Listen {
        source: &address,
        name: &video.title,
        key: places::of_video(&video.id),
        speed: app.cfg.speed,
        has_next,
        has_previous,
        play_next: app.cfg.play_next,
        download: Some(&mut download),
        video: Some(video.id.clone()),
    };
    let outcome = listen::show(hwnd, &mut l);
    let speed = l.speed;
    let Some(app) = (unsafe { super::app(hwnd) }) else {
        return;
    };
    keep_speed(app, speed);
    match outcome {
        Err(why) => problem(
            hwnd,
            &format!("The sound of {} could not be played: {why}", video.title),
        ),
        Ok(listen::Outcome::Closed) => {
            // Shown, not said: the Play window said it all.
            set_text(
                hwnd,
                IDC_STATUS,
                &format!(
                    "Stopped playing {}. Enter carries on from there; Ctrl+D downloads it.",
                    video.title
                ),
            );
            focus(hwnd, IDC_LIST);
        }
        Ok(o) => step(hwnd, app, o == listen::Outcome::Next),
    }
}

/// Goes on to the next video in the list, or the previous, and plays it.
fn step(hwnd: HWND, app: &mut App, next: bool) {
    let lb = list(hwnd);
    let at = downloads::chosen(lb).unwrap_or(0);
    let Some(o) = app.online.last() else { return };
    let playable = |i: usize| matches!(o.rows[i], Row::Video { .. } | Row::Played(_));
    let found = if next {
        (at + 1..o.rows.len()).find(|&i| playable(i))
    } else {
        (0..at).rev().find(|&i| playable(i))
    };
    let Some(i) = found else {
        focus(hwnd, IDC_LIST);
        return;
    };
    unsafe {
        SendMessageW(lb, LB_SETCURSEL, Some(WPARAM(i)), None);
    }
    name_enter(hwnd, app);
    focus(hwnd, IDC_LIST);
    match app.online.last().map(|o| &o.rows[i]) {
        Some(Row::Video { v, .. }) => {
            let v = v.clone();
            listen(hwnd, app, v);
        }
        Some(Row::Played(p)) => {
            let p = p.clone();
            play_again(hwnd, app, p);
        }
        _ => {}
    }
}

/// Ctrl+H: what was played lately.
pub fn recently_played(hwnd: HWND, app: &mut App) {
    let all = history::load();
    if all.is_empty() {
        say(
            hwnd,
            "Nothing played yet. What you play, downloaded or from YouTube, is listed here, with \
             where you stopped.",
        );
        return;
    }
    let n = all.len();
    let replace = matches!(app.online.last().map(|o| &o.shows), Some(Shows::History));
    push(
        hwnd,
        app,
        Online {
            shows: Shows::History,
            rows: all.into_iter().map(Row::Played).collect(),
            at: 0,
        },
        replace,
        &format!(
            "{} played lately, newest first. Enter carries on where you stopped; Delete takes \
             one off the list.",
            count_of(n, "thing", "things")
        ),
    );
}

/// Plays something from the list of what was played lately.
fn play_again(hwnd: HWND, app: &mut App, p: Played) {
    if let Some(id) = p.video() {
        let v = Video {
            id: id.to_string(),
            title: p.name.clone(),
            ..Video::default()
        };
        listen(hwnd, app, v);
        return;
    }
    let path = PathBuf::from(&p.key);
    if !path.exists() {
        say(
            hwnd,
            &format!(
                "{} is no longer where it was: it was moved or deleted outside VidFetch. Delete \
                 takes it off this list.",
                p.name
            ),
        );
        return;
    }
    let (has_next, has_previous) = neighbours(hwnd, app);
    history::record(&p.key, &p.name);
    let mut l = listen::Listen {
        source: &p.key,
        name: &p.name,
        key: p.key.clone(),
        speed: app.cfg.speed,
        has_next,
        has_previous,
        play_next: app.cfg.play_next,
        download: None,
        video: library::video_of(Config::past_path().as_deref(), &path),
    };
    let outcome = listen::show(hwnd, &mut l);
    let speed = l.speed;
    let Some(app) = (unsafe { super::app(hwnd) }) else {
        return;
    };
    keep_speed(app, speed);
    match outcome {
        Err(why) => problem(hwnd, &why),
        Ok(listen::Outcome::Closed) => focus(hwnd, IDC_LIST),
        Ok(o) => step(hwnd, app, o == listen::Outcome::Next),
    }
}

// ---------------------------------------------------------------- downloading

/// Downloads `v` with the options as they stand, into the category shown,
/// now or in its turn; what to say. `hush`: another window is in front,
/// which says it instead (the Play window).
pub fn download_video(hwnd: HWND, v: &Video, hush: bool) -> String {
    let Some(app) = (unsafe { super::app(hwnd) }) else {
        return String::new();
    };
    let owner = unsafe { GetForegroundWindow() };
    let lines = job::past_lines(app.cfg.format, app.cfg.lines);
    if let Some(file) =
        Config::past_path().and_then(|r| library::already(&r, &v.id, app.cfg.format.word(), lines))
    {
        let name = file
            .file_name()
            .unwrap_or_default()
            .to_string_lossy()
            .into_owned();
        if !message::ask(
            owner,
            &format!("Already downloaded as {name}, at this quality. Download it again?"),
            "&Download again",
            "&Cancel",
            false,
        ) {
            return "Not downloaded again.".into();
        }
    }
    let folder = app.cfg.download_folder();
    HUSH.with(|h| h.set(hush));
    submit(hwnd, app, v.url(), false, None, folder);
    HUSH.with(|h| h.set(false));
    get_text(hwnd, IDC_STATUS)
}

// ---------------------------------------------------------------- channels

/// Ctrl+L: what is new from the channels followed.
pub fn new_videos(hwnd: HWND, app: &mut App) {
    if app.channels.is_empty() {
        say(
            hwnd,
            "You follow no channels yet. Follow one from the context menu of a video YouTube \
             found, or of one of your downloads, or with its address in New download, Ctrl+N.",
        );
        return;
    }
    let list = app.channels.clone();
    say(
        hwnd,
        &format!(
            "Looking for new videos from your {}.",
            count_of(list.len(), "channel", "channels")
        ),
    );
    look(hwnd, app, move |stop| {
        let mut videos = Vec::new();
        let mut failed = Vec::new();
        for group in list.chunks(AT_ONCE) {
            let got: Vec<Result<(String, Vec<Video>), String>> = std::thread::scope(|sc| {
                let threads: Vec<_> = group
                    .iter()
                    .map(|c| sc.spawn(move || youtube::feed(&c.id, stop)))
                    .collect();
                threads
                    .into_iter()
                    .map(|t| t.join().unwrap_or_else(|_| Err("it failed".into())))
                    .collect()
            });
            if stop.load(Ordering::Relaxed) {
                return None;
            }
            for (c, r) in group.iter().zip(got) {
                match r {
                    Ok((_, vs)) => videos.extend(vs.into_iter().map(|v| {
                        let new = v.published > c.looked;
                        (v, new)
                    })),
                    Err(e) => failed.push(format!("{}: {e}", c.name)),
                }
            }
        }
        videos.sort_by_key(|(v, _)| std::cmp::Reverse(v.published));
        Some(Found::New { videos, failed })
    });
}

fn new_found(hwnd: HWND, app: &mut App, videos: Vec<(Video, bool)>, failed: Vec<String>) {
    let fresh = videos.iter().filter(|(_, new)| *new).count();
    let total = videos.len();
    let channels = count_of(app.channels.len(), "channel", "channels");
    // Looked at now: next time, new means newer than this.
    let now = youtube::now();
    for c in app.channels.iter_mut() {
        c.looked = now;
    }
    channels::save(&app.channels);
    let mut said = match fresh {
        0 => format!(
            "Nothing new since you last looked. The latest {} from your {channels}, newest first.",
            count_of(total, "video", "videos")
        ),
        n => format!(
            "{} since you last looked, from your {channels}, newest first; {total} in all.",
            count_of(n, "new video", "new videos")
        ),
    };
    if !failed.is_empty() {
        said += &format!(" Not reached: {}.", failed.join("; "));
    }
    let replace = matches!(app.online.last().map(|o| &o.shows), Some(Shows::New));
    let rows = videos
        .into_iter()
        .map(|(v, new)| Row::Video { v, new })
        .collect();
    push(
        hwnd,
        app,
        Online {
            shows: Shows::New,
            rows,
            at: 0,
        },
        replace,
        &said,
    );
}

/// Ctrl+Shift+L: the channels followed.
pub fn your_channels(hwnd: HWND, app: &mut App) {
    if app.channels.is_empty() {
        say(
            hwnd,
            "You follow no channels yet. Follow one from the context menu of a video YouTube \
             found, or of one of your downloads, or with its address in New download, Ctrl+N.",
        );
        return;
    }
    let rows = app.channels.iter().cloned().map(Row::Channel).collect();
    let replace = matches!(app.online.last().map(|o| &o.shows), Some(Shows::Channels));
    let said = format!(
        "You follow {}. Enter shows one's latest videos; Delete stops following it.",
        count_of(app.channels.len(), "channel", "channels")
    );
    push(
        hwnd,
        app,
        Online {
            shows: Shows::Channels,
            rows,
            at: 0,
        },
        replace,
        &said,
    );
}

/// A channel's latest videos.
fn show_channel(hwnd: HWND, app: &mut App, channel: Channel) {
    say(
        hwnd,
        &format!("Getting the latest videos from {}.", channel.name),
    );
    look(hwnd, app, move |stop| {
        let result = youtube::feed(&channel.id, stop).map(|(_, v)| v);
        if stop.load(Ordering::Relaxed) {
            return None;
        }
        Some(Found::Channel { channel, result })
    });
}

/// Follows the channel `id`, called `name`.
fn follow(hwnd: HWND, app: &mut App, id: String, name: String) {
    if let Some(c) = app.channels.iter().find(|c| c.id == id) {
        say(hwnd, &format!("You already follow {}.", c.name));
        return;
    }
    app.channels.push(Channel {
        id,
        name: name.clone(),
        looked: youtube::now(),
    });
    channels::save(&app.channels);
    refill_channels(hwnd, app);
    say(
        hwnd,
        &format!(
            "Now following {name}. Ctrl+L shows what is new from your channels; Ctrl+Shift+L \
             lists them."
        ),
    );
}

/// Follows the channel of an address typed in New download.
pub fn follow_address(hwnd: HWND, app: &mut App, url: String) {
    say(hwnd, "Finding the channel.");
    look(hwnd, app, move |stop| {
        let r = youtube::channel_of(&url, stop);
        if stop.load(Ordering::Relaxed) {
            return None;
        }
        Some(Found::Follow(r))
    });
}

/// Follows the channel of a downloaded file, found by its video.
fn follow_download(hwnd: HWND, app: &mut App, e: &library::Entry) {
    let Some(id) = video_of(e) else {
        say(
            hwnd,
            &format!(
                "VidFetch does not know which YouTube video {} came from, so it cannot find its \
                 channel.",
                e.name
            ),
        );
        return;
    };
    say(hwnd, &format!("Finding the channel of {}.", e.name));
    look(hwnd, app, move |stop| {
        let r = youtube::details(&id, stop).map(|d| (d.channel_id, d.channel));
        if stop.load(Ordering::Relaxed) {
            return None;
        }
        Some(Found::Follow(r))
    });
}

/// Asks, then stops following `c`.
fn unfollow(hwnd: HWND, app: &mut App, c: &Channel) {
    if !app.channels.iter().any(|x| x.id == c.id) {
        say(hwnd, &format!("You do not follow {}.", c.name));
        return;
    }
    if !message::ask(
        hwnd,
        &format!(
            "Stop following {}? Whatever you downloaded from it stays where it is.",
            c.name
        ),
        "&Stop following",
        "&Keep following",
        false,
    ) {
        say(hwnd, &format!("Still following {}.", c.name));
        focus(hwnd, IDC_LIST);
        return;
    }
    app.channels.retain(|x| x.id != c.id);
    channels::save(&app.channels);
    refill_channels(hwnd, app);
    let left = count_of(app.channels.len(), "channel", "channels");
    say(
        hwnd,
        &format!("No longer following {}. You follow {left}.", c.name),
    );
    focus(hwnd, IDC_LIST);
}

/// Shows the channels followed again, if the list is showing them.
fn refill_channels(hwnd: HWND, app: &mut App) {
    let Some(o) = app.online.last_mut() else {
        return;
    };
    if !matches!(o.shows, Shows::Channels) {
        return;
    }
    o.rows = app.channels.iter().cloned().map(Row::Channel).collect();
    o.at = downloads::chosen(list(hwnd))
        .unwrap_or(0)
        .min(o.rows.len().saturating_sub(1));
    show_top(hwnd, app);
}

// ---------------------------------------------------------------- downloads

/// The YouTube video a download came from.
fn video_of(e: &library::Entry) -> Option<String> {
    library::video_of(Config::past_path().as_deref(), &e.path)
}

/// Ctrl+I on a download: its description, from YouTube.
fn describe_download(hwnd: HWND, app: &mut App, e: &library::Entry) {
    let Some(id) = video_of(e) else {
        say(
            hwnd,
            &format!(
                "VidFetch does not know which YouTube video {} came from, so it cannot fetch its \
                 description.",
                e.name
            ),
        );
        return;
    };
    describe_id(hwnd, app, id, e.name.clone());
}

fn describe_id(hwnd: HWND, app: &mut App, id: String, name: String) {
    say(hwnd, &format!("Fetching the description of {name}."));
    look(hwnd, app, move |stop| {
        let result = youtube::details(&id, stop).map(|d| {
            let mut head = vec![d.channel.clone()];
            if !d.date.is_empty() {
                head.push(d.date.clone());
            }
            if d.seconds > 0 {
                head.push(listen::clock(d.seconds as f64));
            }
            let body = if d.description.trim().is_empty() {
                "It has no description.".to_string()
            } else {
                d.description
            };
            format!("{}\nBy {}.\n\n{body}", d.title, head.join(", "))
        });
        if stop.load(Ordering::Relaxed) {
            return None;
        }
        Some(Found::Describe { name, result })
    });
}

/// The commands of the YouTube menu, and Ctrl+I, on a download chosen in the
/// list of downloads. False if `id` is none of them.
pub fn download_command(hwnd: HWND, app: &mut App, id: i32) -> bool {
    match id {
        IDM_SEARCH => search(hwnd, app),
        IDM_NEW_VIDEOS => new_videos(hwnd, app),
        IDM_CHANNELS => your_channels(hwnd, app),
        IDM_DOWNLOAD => say(
            hwnd,
            "Ctrl+D downloads a video YouTube found, or one from your channels. Ctrl+N downloads \
             from a web address.",
        ),
        IDM_UNFOLLOW => say(
            hwnd,
            "Your channels, Ctrl+Shift+L, lists the channels you follow; Delete there stops \
             following one.",
        ),
        IDM_DESCRIBE | IDM_FOLLOW | IDM_COPY | IDM_BROWSER => {
            let Some(e) = chosen(hwnd, app) else {
                return true;
            };
            if e.is_playlist() && id != IDM_COPY && id != IDM_BROWSER {
                say(
                    hwnd,
                    "That works on one video: Enter shows the playlist's videos, then choose one.",
                );
                return true;
            }
            match id {
                IDM_DESCRIBE => describe_download(hwnd, app, &e),
                IDM_FOLLOW => follow_download(hwnd, app, &e),
                _ => {
                    let url = match &e.kind {
                        Kind::Playlist { url, .. } if !url.is_empty() => Some(url.clone()),
                        _ => video_of(&e).map(|v| format!("https://www.youtube.com/watch?v={v}")),
                    };
                    let Some(url) = url else {
                        say(
                            hwnd,
                            &format!("VidFetch does not know where {} came from.", e.name),
                        );
                        return true;
                    };
                    address(hwnd, &url, &e.name, id == IDM_COPY);
                }
            }
        }
        _ => return false,
    }
    true
}

/// Copies `url`, or opens it in the browser; says so.
fn address(hwnd: HWND, url: &str, name: &str, copy: bool) {
    if copy {
        if downloads::copy(hwnd, url) {
            say(hwnd, &format!("Copied the web address of {name}."));
        } else {
            say(
                hwnd,
                "Windows would not let VidFetch use the clipboard. Try again.",
            );
        }
    } else if downloads::browse(url) {
        say(hwnd, &format!("Opening {name} in the browser."));
    } else {
        say(hwnd, "Windows could not open the browser.");
    }
}

// ---------------------------------------------------------------- commands

/// A command while an online list is shown. None for the commands that work
/// the same everywhere, such as New download.
pub fn command(hwnd: HWND, app: &mut App, id: i32) -> Option<bool> {
    let row = chosen_row(hwnd, app).map(|(i, r)| {
        (
            i,
            match r {
                Row::Video { v, .. } => Row::Video {
                    v: v.clone(),
                    new: false,
                },
                Row::Channel(c) => Row::Channel(c.clone()),
                Row::More(t) => Row::More(t.clone()),
                Row::Played(p) => Row::Played(p.clone()),
            },
        )
    });
    let shown_channel = match app.online.last().map(|o| &o.shows) {
        Some(Shows::Channel(c)) => Some(c.clone()),
        _ => None,
    };
    match id {
        IDM_SEARCH => search(hwnd, app),
        IDM_NEW_VIDEOS => new_videos(hwnd, app),
        IDM_CHANNELS => your_channels(hwnd, app),
        IDM_BACK => {
            back(hwnd, app);
        }
        IDM_HISTORY => recently_played(hwnd, app),
        IDM_DELETE if matches!(row, Some((_, Row::Played(_)))) => {
            if let Some((at, Row::Played(p))) = row {
                history::remove(&p.key);
                if let Some(o) = app.online.last_mut() {
                    o.rows.remove(at);
                    o.at = at.min(o.rows.len().saturating_sub(1));
                }
                show_top(hwnd, app);
                let left = app.online.last().map_or(0, |o| o.rows.len());
                let left = if left == 0 {
                    "The list is empty now; Escape goes back.".to_string()
                } else {
                    format!("{} left.", count_of(left, "thing", "things"))
                };
                say(
                    hwnd,
                    &format!("Took {} off the list; nothing was deleted. {left}", p.name),
                );
            }
        }
        IDM_OPEN => match row {
            Some((_, Row::Video { v, .. })) => listen(hwnd, app, v),
            Some((_, Row::Channel(c))) => show_channel(hwnd, app, c),
            Some((_, Row::More(t))) => more(hwnd, app, t),
            Some((_, Row::Played(p))) => play_again(hwnd, app, p),
            None => say(hwnd, "The list is empty. Escape goes back."),
        },
        IDM_DOWNLOAD => match row {
            Some((_, Row::Video { v, .. })) => {
                let said = download_video(hwnd, &v, false);
                if said == "Not downloaded again." {
                    say(hwnd, &said);
                }
                focus(hwnd, IDC_LIST);
            }
            Some((_, Row::Channel(c))) => {
                if message::ask(
                    hwnd,
                    &format!(
                        "Download every video of {}, into a folder of its own? A channel can \
                         have hundreds. Ctrl+U fetches only its new ones later.",
                        c.name
                    ),
                    "&Download them all",
                    "&Cancel",
                    false,
                ) {
                    let folder = app.cfg.download_folder();
                    submit(hwnd, app, c.videos_url(), false, None, folder);
                } else {
                    say(hwnd, "Nothing downloaded.");
                }
                focus(hwnd, IDC_LIST);
            }
            _ => say(hwnd, "Choose a video in the list first."),
        },
        IDM_FOLLOW => match (row, shown_channel) {
            (Some((_, Row::Video { v, .. })), _) if !v.channel_id.is_empty() => {
                follow(hwnd, app, v.channel_id.clone(), v.channel.clone())
            }
            (Some((_, Row::Video { v, .. })), _) => {
                say(hwnd, &format!("Finding the channel of {}.", v.title));
                let id = v.id.clone();
                look(hwnd, app, move |stop| {
                    let r = youtube::details(&id, stop).map(|d| (d.channel_id, d.channel));
                    Some(Found::Follow(r))
                });
            }
            (Some((_, Row::Channel(c))), _) => {
                say(hwnd, &format!("You already follow {}.", c.name))
            }
            (_, Some(c)) => follow(hwnd, app, c.id, c.name),
            _ => say(hwnd, "Choose a video in the list first."),
        },
        IDM_UNFOLLOW | IDM_DELETE => match (row, shown_channel) {
            (Some((_, Row::Channel(c))), _) => unfollow(hwnd, app, &c),
            (_, Some(c)) => unfollow(hwnd, app, &c),
            (Some((_, Row::Video { v, .. })), _) if !v.channel_id.is_empty() => {
                let c = Channel {
                    id: v.channel_id.clone(),
                    name: v.channel.clone(),
                    looked: 0,
                };
                unfollow(hwnd, app, &c)
            }
            _ if id == IDM_DELETE => say(
                hwnd,
                "Delete works on your downloads, and on Your channels. Escape goes back to your \
                 downloads.",
            ),
            _ => say(hwnd, "Choose a channel, or a video from one, first."),
        },
        IDM_CHANNEL => match row {
            Some((_, Row::Video { v, .. })) if !v.channel_id.is_empty() => show_channel(
                hwnd,
                app,
                Channel {
                    id: v.channel_id.clone(),
                    name: v.channel.clone(),
                    looked: 0,
                },
            ),
            Some((_, Row::Channel(c))) => show_channel(hwnd, app, c),
            _ => say(hwnd, "VidFetch does not know this video's channel."),
        },
        IDM_DESCRIBE => match row {
            Some((_, Row::Video { v, .. })) if !v.description.trim().is_empty() => {
                message::description(
                    hwnd,
                    &format!(
                        "{}\nBy {}, {}.\n\n{}",
                        v.title, v.channel, v.when, v.description
                    ),
                );
                focus(hwnd, IDC_LIST);
            }
            Some((_, Row::Video { v, .. })) => {
                describe_id(hwnd, app, v.id.clone(), v.title.clone())
            }
            _ => say(hwnd, "Choose a video in the list first."),
        },
        IDM_COPY | IDM_BROWSER => match row {
            Some((_, Row::Video { v, .. })) => address(hwnd, &v.url(), &v.title, id == IDM_COPY),
            Some((_, Row::Channel(c))) => address(hwnd, &c.url(), &c.name, id == IDM_COPY),
            _ => say(hwnd, "Choose a video in the list first."),
        },
        IDM_EXTERNAL => match row {
            Some((_, Row::Video { v, .. })) => address(hwnd, &v.url(), &v.title, false),
            Some((_, Row::Channel(c))) => address(hwnd, &c.url(), &c.name, false),
            _ => say(hwnd, "Choose a video in the list first."),
        },
        IDM_UPDATE | IDM_RENAME | IDM_SHOW | IDM_MOVE => say(
            hwnd,
            "That works on your downloads. Escape goes back to them.",
        ),
        _ => return None,
    }
    Some(true)
}

/// The context menu for the row chosen.
pub fn context_menu(hwnd: HWND, app: &mut App, mouse: Option<(i32, i32)>) {
    let Some((_, row)) = chosen_row(hwnd, app) else {
        say(hwnd, "The list is empty. Escape goes back.");
        return;
    };
    let shown_channel = matches!(app.online.last().map(|o| &o.shows), Some(Shows::Channel(_)));
    let mut items: Vec<(i32, &str)> = match row {
        Row::Video { v, .. } => {
            let followed = app.channels.iter().any(|c| c.id == v.channel_id);
            let mut items = vec![
                (IDM_OPEN, "&Play the sound\tEnter"),
                (IDM_DOWNLOAD, "&Download it\tCtrl+D"),
                (IDM_DESCRIBE, "Read the descr&iption\tCtrl+I"),
            ];
            if !shown_channel {
                items.push((IDM_CHANNEL, "Show the &channel's latest videos"));
            }
            if followed {
                items.push((IDM_UNFOLLOW, "Stop following the channel (&unfollow)..."));
            } else {
                items.push((IDM_FOLLOW, "&Follow the channel"));
            }
            items.extend([
                (IDM_COPY, "Cop&y the web address"),
                (IDM_BROWSER, "Open in the &browser"),
            ]);
            items
        }
        Row::Channel(_) => vec![
            (IDM_OPEN, "Show its latest &videos\tEnter"),
            (IDM_DOWNLOAD, "&Download all its videos, as a playlist..."),
            (IDM_COPY, "Cop&y the web address"),
            (IDM_BROWSER, "Open in the &browser"),
            (IDM_UNFOLLOW, "&Stop following...\tDelete"),
        ],
        Row::More(_) => vec![(IDM_OPEN, "Fetch the next &page of results\tEnter")],
        Row::Played(_) => vec![
            (IDM_OPEN, "&Play, from where you stopped\tEnter"),
            (IDM_DELETE, "&Take it off this list\tDelete"),
        ],
    };
    items.extend([(0, ""), (IDM_BACK, "&Back\tEscape")]);
    if let Some(id) = downloads::context_menu(hwnd, list(hwnd), mouse, &items) {
        command(hwnd, app, id);
    }
}
