//! The YouTube window: search results, what is new from the channels
//! followed, those channels, a channel's videos and playlists, a playlist's
//! videos, and what was played lately (log entry 3, plans 1 and 2; idea 9).
//! One list, one set of keys, whatever it shows: Enter plays, Ctrl+D
//! downloads, the context menu has the rest, Backspace goes back one list
//! at a time, as in File Explorer, closing the window from the first, and
//! Escape closes the window from any list (log entry 6, Q1).
//!
//! These lists were in the main window at first, under its Category box,
//! which then made no sense and cleared them when changed; the user asked
//! for "its own window to avoid confusion" (milestone 4 test, sections 3 and
//! 5). The window stands in front of the main one, which waits, as a dialog
//! box's owner does, but downloads carry on behind it.
//!
//! Everything online is looked up on a thread of its own, so the window
//! stays free; one lookup at a time, and Escape stops it.

use std::collections::HashMap;
use std::sync::mpsc;
use std::sync::Mutex;
use std::time::{Duration, Instant};

use super::*;
use crate::channels::{self, Channel};
use crate::history::{self, Played};
use crate::places;
use crate::youtube::{self, Playlist, Query, SearchFor, Video};
use windows::Win32::UI::Input::KeyboardAndMouse::EnableWindow;

/// A lookup has finished: its LPARAM is a boxed `(number, Found)`.
pub const WM_FOUND: u32 = WM_APP + 22;

const IDD_BROWSE: u16 = 111;
const IDC_B_DOWNLOAD: i32 = 1120;
const IDC_B_SEARCH: i32 = 1121;
const IDC_B_CLOSE: i32 = 1122;
pub const IDM_PLAYLISTS: i32 = 2053;
pub const IDM_CLOSE: i32 = 2054;

/// How many channels are looked at at once.
const AT_ONCE: usize = 8;
/// How long a sound's address, from yt-dlp, is trusted: YouTube's last six
/// hours.
const FRESH: Duration = Duration::from_secs(3 * 3600);

/// The sounds' addresses found, by video: while one plays, the next one's is
/// found, so Page Down plays it at once.
pub type Sounds = Arc<Mutex<HashMap<String, Sound>>>;

pub enum Sound {
    Finding,
    Found(String, Instant),
}

#[derive(Clone)]
pub enum Row {
    /// A video, and whether it is new since the user last looked.
    Video {
        v: Video,
        new: bool,
    },
    Channel(Channel),
    /// The last line of a channel's videos: its playlists.
    Playlists(Channel),
    Playlist(Playlist),
    /// The key to YouTube's next page.
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
            Row::Playlists(c) => format!("Playlists of {}: Enter lists them.", c.name),
            Row::Playlist(p) if p.count.is_empty() => format!("{}: playlist", p.title),
            Row::Playlist(p) => format!("{}: playlist, {}", p.title, p.count),
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
            Row::More(_) => "More: Enter fetches the next page.".into(),
        }
    }

    fn playable(&self) -> bool {
        match self {
            Row::Video { v, .. } => !is_live(v),
            Row::Played(_) => true,
            _ => false,
        }
    }

    /// The YouTube video it is, if it is one.
    fn video(&self) -> Option<Video> {
        match self {
            Row::Video { v, .. } => Some(v.clone()),
            Row::Played(p) => p.video().map(|id| Video {
                id: id.to_string(),
                title: p.name.clone(),
                ..Video::default()
            }),
            _ => None,
        }
    }
}

/// A live stream, which VidFetch cannot take: it takes sound and picture over
/// plain web requests, and a live stream only comes in pieces.
fn is_live(v: &Video) -> bool {
    v.length == "live now"
}

const LIVE: &str =
    "That is a live stream, which VidFetch cannot play or download while it is live. \
                    Once it has ended, YouTube usually keeps it as an ordinary video.";

#[derive(Clone)]
pub enum Shows {
    Search(Query),
    /// The videos beside a video: its id and title.
    Related(String, String),
    /// A channel's own search.
    ChannelSearch(Channel, String),
    New,
    Channels,
    Channel(Channel),
    Playlists(Channel),
    Playlist(Playlist),
    History,
}

/// One list, and which row was chosen when it was left.
pub struct Online {
    pub shows: Shows,
    pub rows: Vec<Row>,
    pub at: usize,
    /// Shown while what it lists is being looked up.
    pub loading: bool,
}

pub enum Found {
    Search {
        query: Query,
        more: bool,
        result: Result<youtube::Results, String>,
        slow: bool,
    },
    New {
        videos: Vec<(Video, bool)>,
        /// Each channel reached, with its name and the videos it listed.
        listed: Vec<(String, String, Vec<String>)>,
        failed: Vec<String>,
    },
    Channel {
        channel: Channel,
        more: bool,
        result: Result<youtube::Results, String>,
    },
    Related {
        id: String,
        title: String,
        more: bool,
        result: Result<youtube::Results, String>,
    },
    ChannelSearch {
        channel: Channel,
        query: String,
        more: bool,
        result: Result<youtube::Results, String>,
    },
    Comments {
        name: String,
        result: Result<String, String>,
    },
    Transcript {
        name: String,
        result: Result<String, String>,
    },
    Playlists {
        channel: Channel,
        result: Result<Vec<Playlist>, String>,
    },
    Playlist {
        playlist: Playlist,
        more: bool,
        result: Result<youtube::Results, String>,
    },
    /// A channel to show: its id and name.
    OpenChannel(Result<(String, String), String>),
    Follow(Result<(String, String), String>),
    Unfollow(Result<(String, String), String>),
    Listen {
        url: String,
        name: String,
    },
    Describe {
        name: String,
        result: Result<String, String>,
    },
}

/// A name in a label, where an & would make a mnemonic.
fn plain(name: &str) -> String {
    name.replace('&', "&&")
}

/// The list's label for what it shows: always with W as its letter.
fn label(o: &Online) -> String {
    match &o.shows {
        Shows::Search(q) => format!("&What YouTube found for {}:", plain(&q.text)),
        Shows::Related(_, t) => format!(
            "Videos related to {}, as YouTube suggests them, sho&wn:",
            plain(t)
        ),
        Shows::ChannelSearch(c, q) => format!("&What {} has for {}:", plain(&c.name), plain(q)),
        Shows::New => "Ne&w videos from your channels, newest first:".into(),
        Shows::Channels => "Channels you follo&w:".into(),
        Shows::Channel(c) => format!("Latest videos from {}, ne&west first:", plain(&c.name)),
        Shows::Playlists(c) => format!(
            "Playlists of {}, as the channel sho&ws them:",
            plain(&c.name)
        ),
        Shows::Playlist(p) if p.title.is_empty() => {
            "Videos in the playlist, sho&wn in its order:".into()
        }
        Shows::Playlist(p) => format!(
            "Videos in the playlist {}, sho&wn in its order:",
            plain(&p.title)
        ),
        Shows::History => "&What you played lately, newest first:".into(),
    }
}

/// The window's title for what it shows.
fn title(o: &Online) -> String {
    let what = match &o.shows {
        Shows::Search(q) => format!("YouTube results for {}", q.text),
        Shows::Related(_, t) => format!("Related to {t}"),
        Shows::ChannelSearch(c, q) => format!("{} results for {q}", c.name),
        Shows::New => "New videos from your channels".into(),
        Shows::Channels => "Your channels".into(),
        Shows::Channel(c) => c.name.clone(),
        Shows::Playlists(c) => format!("Playlists of {}", c.name),
        Shows::Playlist(p) if p.title.is_empty() => "Playlist".into(),
        Shows::Playlist(p) => p.title.clone(),
        Shows::History => "Recently played".into(),
    };
    format!("{what} - VidFetch")
}

/// The Enter button's words for the row chosen.
fn enter_words(row: Option<&Row>) -> &'static str {
    match row {
        Some(Row::Channel(_)) => "O&pen channel  Enter",
        Some(Row::Playlists(_)) => "O&pen playlists  Enter",
        Some(Row::Playlist(_)) => "O&pen playlist  Enter",
        Some(Row::More(_)) => "Fetch &more  Enter",
        _ => "&Play  Enter",
    }
}

fn top_row(app: &App, at: Option<usize>) -> Option<&Row> {
    app.online.last().and_then(|o| o.rows.get(at?))
}

fn chosen_row(hwnd: HWND, app: &App) -> Option<(usize, Row)> {
    let at = downloads::chosen(list(hwnd))?;
    Some((at, top_row(app, Some(at))?.clone()))
}

fn count_of(n: usize, one: &str, many: &str) -> String {
    if n == 1 {
        format!("1 {one}")
    } else {
        format!("{n} {many}")
    }
}

/// The main window, from it or from the YouTube window.
fn main_of(hwnd: HWND, app: &App) -> HWND {
    app.main.unwrap_or(hwnd)
}

// ---------------------------------------------------------------- the window

/// Opens the YouTube window in front of the main one, with `first` done in
/// it, and runs it until it closes. Already open, `first` is done in it.
fn open<F: FnOnce(HWND, &mut App)>(hwnd: HWND, app: &mut App, first: F) {
    if let Some(b) = app.browse {
        first(b, app);
        return;
    }
    let main = main_of(hwnd, app);
    let Ok(instance) = (unsafe { GetModuleHandleW(None) }) else {
        return;
    };
    let b = match unsafe {
        CreateDialogParamW(
            Some(instance.into()),
            PCWSTR(IDD_BROWSE as usize as *const u16),
            Some(main),
            Some(proc),
            LPARAM(app as *mut App as isize),
        )
    } {
        Ok(b) => b,
        Err(e) => {
            say(main, &format!("The YouTube window could not open: {e}"));
            return;
        }
    };
    app.browse = Some(b);
    unsafe {
        let _ = EnableWindow(main, false);
        let _ = ShowWindow(b, SW_SHOW);
        let _ = SetForegroundWindow(b);
    }
    first(b, app);
    // From here, the window's own handler has the app: this one is left be.
    run(b);
    focus(main, IDC_LIST);
    // What waited for the main window to be in front.
    if let Some(app) = unsafe { super::app(main) } {
        settle(main, app);
    }
}

/// The window's own message loop, as the main window's: its shortcuts, then
/// the dialog's keys.
fn run(b: HWND) {
    let mut msg = MSG::default();
    while unsafe { IsWindow(Some(b)) }.as_bool() {
        let r = unsafe { GetMessageW(&mut msg, None, 0, 0) };
        if r.0 <= 0 {
            // VidFetch is ending: the main loop takes the quit.
            unsafe { PostQuitMessage(msg.wParam.0 as i32) };
            return;
        }
        let ours = msg.hwnd == b || unsafe { IsChild(b, msg.hwnd) }.as_bool();
        if ours && msg.message == WM_KEYDOWN {
            if let Some(id) = shortcut(msg.wParam.0 as u16) {
                unsafe {
                    SendMessageW(b, WM_COMMAND, Some(WPARAM(id as usize)), None);
                }
                continue;
            }
        }
        if unsafe { IsDialogMessageW(b, &msg) }.as_bool() {
            continue;
        }
        unsafe {
            let _ = TranslateMessage(&msg);
            DispatchMessageW(&msg);
        }
    }
}

/// The window's shortcuts, in step with its menus in vidfetch.rc.
fn shortcut(key: u16) -> Option<i32> {
    let down = |k: VIRTUAL_KEY| unsafe { GetKeyState(k.0 as i32) } < 0;
    if down(VK_MENU) || !down(VK_CONTROL) {
        return None;
    }
    if down(VK_SHIFT) {
        return match key as u8 {
            b'L' => Some(IDM_CHANNELS),
            b'F' => Some(IDM_SEARCH_CHANNEL),
            _ => None,
        };
    }
    match key as u8 {
        b'F' => Some(IDM_SEARCH),
        b'L' => Some(IDM_NEW_VIDEOS),
        b'D' => Some(IDM_DOWNLOAD),
        b'I' => Some(IDM_DESCRIBE),
        b'H' => Some(IDM_HISTORY),
        b'R' => Some(IDM_RELATED),
        b'K' => Some(IDM_COMMENTS),
        b'T' => Some(IDM_TRANSCRIPT),
        _ => None,
    }
}

/// Closes the window, back to the main one.
fn close(hwnd: HWND, app: &mut App) {
    if let Some((_, stop)) = app.lookup.take() {
        stop.store(true, Ordering::Relaxed);
    }
    app.online.clear();
    app.browse = None;
    let main = main_of(hwnd, app);
    unsafe {
        // Enabled first, so Windows hands the main window the focus.
        let _ = EnableWindow(main, true);
        let _ = DestroyWindow(hwnd);
    }
}

unsafe extern "system" fn proc(hwnd: HWND, message: u32, wparam: WPARAM, lparam: LPARAM) -> isize {
    match message {
        WM_INITDIALOG => {
            SetWindowLongPtrW(hwnd, GWLP_USERDATA, lparam.0);
            0
        }
        WM_FOUND => {
            if let Some(app) = super::app(hwnd) {
                found(hwnd, app, lparam);
            }
            1
        }
        WM_COMMAND => {
            let id = (wparam.0 & 0xFFFF) as i32;
            let code = ((wparam.0 >> 16) & 0xFFFF) as u32;
            let Some(app) = super::app(hwnd) else {
                return 0;
            };
            let command_id = match (id, code) {
                (IDC_STATUS, EN_SETFOCUS) => {
                    caret_to_start(hwnd, IDC_STATUS);
                    return 1;
                }
                (IDC_LIST, LBN_SELCHANGE) => {
                    name_enter(hwnd, app);
                    return 0;
                }
                (IDC_LIST, LBN_DBLCLK) => IDM_OPEN,
                (IDC_LIST, _) | (IDC_STATUS, _) => return 0,
                (IDC_OPEN, _) => IDM_OPEN,
                (i, _) if i == IDOK.0 => IDM_OPEN,
                (IDC_B_DOWNLOAD, _) => IDM_DOWNLOAD,
                (IDC_B_SEARCH, _) => IDM_SEARCH,
                (IDC_B_CLOSE, _) => IDM_CLOSE,
                (i, _) if i == IDCANCEL.0 => {
                    escape(hwnd, app);
                    return 1;
                }
                (i, 0 | 1) if lparam.0 == 0 => i,
                _ => return 0,
            };
            if command(hwnd, app, command_id) {
                1
            } else {
                0
            }
        }
        // Delete and Backspace in the list (LBS_WANTKEYBOARDINPUT).
        WM_VKEYTOITEM => {
            let key = (wparam.0 & 0xFFFF) as u16;
            let Some(app) = super::app(hwnd) else {
                return -1;
            };
            if key == VK_DELETE.0 {
                command(hwnd, app, IDM_DELETE);
            } else if key == VK_BACK.0 {
                back(hwnd, app);
            } else {
                return -1;
            }
            -2
        }
        WM_CONTEXTMENU => {
            if wparam.0 != list(hwnd).0 as usize {
                return 0;
            }
            if let Some(app) = super::app(hwnd) {
                context_menu(hwnd, app, lparam);
            }
            1
        }
        WM_INITMENUPOPUP => {
            if let Some(app) = super::app(hwnd) {
                init_menu(hwnd, app, HMENU(wparam.0 as *mut _));
            }
            0
        }
        WM_HELP => {
            help(hwnd);
            1
        }
        WM_CLOSE => {
            if let Some(app) = super::app(hwnd) {
                close(hwnd, app);
            }
            1
        }
        WM_NCDESTROY => {
            // The app belongs to the main window.
            SetWindowLongPtrW(hwnd, GWLP_USERDATA, 0);
            0
        }
        _ => 0,
    }
}

/// Escape: stops a lookup, else closes the window, from whichever list.
fn escape(hwnd: HWND, app: &mut App) {
    if stop_looking(hwnd, app) {
        if app.online.last().is_some_and(|o| o.loading) {
            back(hwnd, app);
        }
        return;
    }
    close(hwnd, app);
}

/// Stops the lookup going on, if any, and says so.
fn stop_looking(hwnd: HWND, app: &mut App) -> bool {
    let Some((_, stop)) = app.lookup.take() else {
        return false;
    };
    stop.store(true, Ordering::Relaxed);
    say(hwnd, "Stopped looking.");
    true
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

/// Shows `o`, on top of what was shown, or in place of it when `replace`;
/// says `said`.
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

/// Shows an empty list of `shows` while it is looked up: in place of one of
/// the same kind.
fn waiting(hwnd: HWND, app: &mut App, shows: Shows, said: &str) {
    let replace = app.online.last().is_some_and(|o| {
        o.loading
            || (std::mem::discriminant(&o.shows) == std::mem::discriminant(&shows)
                && !matches!(shows, Shows::Related(..)))
    });
    push(
        hwnd,
        app,
        Online {
            shows,
            rows: Vec::new(),
            at: 0,
            loading: true,
        },
        replace,
        said,
    );
}

/// What was looked up has come: in place of the empty list waiting for it.
fn arrive(hwnd: HWND, app: &mut App, o: Online, said: &str) {
    let replace = app.online.last().is_some_and(|t| t.loading);
    push(hwnd, app, o, replace, said);
}

/// A lookup failed: the list waiting for it stays, empty, to go back from.
fn not_found(hwnd: HWND, app: &mut App, text: &str) {
    if let Some(o) = app.online.last_mut() {
        o.loading = false;
    }
    problem(hwnd, text);
}

/// Remembers which row is chosen in the list shown, to come back to it.
fn keep_place(hwnd: HWND, app: &mut App) {
    if let (Some(o), Some(at)) = (app.online.last_mut(), downloads::chosen(list(hwnd))) {
        o.at = at;
    }
}

/// Fills the list with the list on top, and names it.
fn show_top(hwnd: HWND, app: &App) {
    let Some(o) = app.online.last() else { return };
    let channel = !matches!(o.shows, Shows::Channel(_));
    let lines: Vec<String> = o.rows.iter().map(|r| r.line(channel)).collect();
    downloads::fill_lines(list(hwnd), &lines, o.at);
    set_text(hwnd, IDC_LIST_LABEL, &label(o));
    let t = wide(&title(o));
    unsafe {
        let _ = SetWindowTextW(hwnd, PCWSTR(t.as_ptr()));
    }
    name_enter(hwnd, app);
}

/// The Enter button says what Enter does to the row chosen.
fn name_enter(hwnd: HWND, app: &App) {
    let text = enter_words(top_row(app, downloads::chosen(list(hwnd))));
    if get_text(hwnd, IDC_OPEN) != text {
        set_text(hwnd, IDC_OPEN, text);
    }
}

/// A label as it is read: no mnemonic, no colon.
fn spoken(label: &str) -> String {
    label
        .replace("&&", "\u{1}")
        .replace('&', "")
        .replace('\u{1}', "&")
        .trim_end_matches(':')
        .to_string()
}

/// Back one list; from the first, the window closes.
fn back(hwnd: HWND, app: &mut App) {
    app.online.pop();
    if app.online.is_empty() {
        close(hwnd, app);
        return;
    }
    show_top(hwnd, app);
    let l = app.online.last().map(label).unwrap_or_default();
    say(hwnd, &format!("Back to {}", spoken(&l)));
    focus(hwnd, IDC_LIST);
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
        Found::New {
            videos,
            listed,
            failed,
        } => new_found(hwnd, app, videos, listed, failed),
        Found::Comments { name, result } => match result {
            Ok(text) => {
                message::comments(hwnd, &format!("Comments on {name}.\n{text}"));
                focus(hwnd, IDC_LIST);
            }
            Err(why) => problem(
                hwnd,
                &format!("The comments of {name} could not be fetched: {why}"),
            ),
        },
        Found::Transcript { name, result } => match result {
            Ok(text) => {
                message::transcript(hwnd, &format!("{name}\n\n{text}"));
                focus(hwnd, IDC_LIST);
            }
            Err(why) => problem(hwnd, &format!("No transcript of {name}: {why}")),
        },
        Found::Related {
            id,
            title,
            more,
            result,
        } => match result {
            Ok(r) if more => appended(hwnd, app, r),
            Ok(r) => related_found(hwnd, app, id, title, r),
            Err(why) => not_found(
                hwnd,
                app,
                &format!("The videos related to {title} could not be listed: {why}"),
            ),
        },
        Found::ChannelSearch {
            channel,
            query,
            more,
            result,
        } => match result {
            Ok(r) if more => appended(hwnd, app, r),
            Ok(r) => channel_searched(hwnd, app, channel, query, r),
            Err(why) => not_found(
                hwnd,
                app,
                &format!("{} could not be searched: {why}", channel.name),
            ),
        },
        Found::Channel {
            channel,
            more,
            result,
        } => match result {
            Ok(r) if more => appended(hwnd, app, r),
            Ok(r) => channel_found(hwnd, app, channel, r),
            Err(why) => not_found(
                hwnd,
                app,
                &format!("The videos of {} could not be listed: {why}", channel.name),
            ),
        },
        Found::Playlists { channel, result } => match result {
            Ok(list) => playlists_found(hwnd, app, channel, list),
            Err(why) => not_found(
                hwnd,
                app,
                &format!(
                    "The playlists of {} could not be listed: {why}",
                    channel.name
                ),
            ),
        },
        Found::Playlist {
            playlist,
            more,
            result,
        } => match result {
            Ok(r) if more => appended(hwnd, app, r),
            Ok(r) => playlist_found(hwnd, app, playlist, r),
            Err(why) => not_found(
                hwnd,
                app,
                &format!(
                    "The videos of {} could not be listed: {why}",
                    if playlist.title.is_empty() {
                        "that playlist"
                    } else {
                        &playlist.title
                    }
                ),
            ),
        },
        Found::OpenChannel(result) => match result {
            Ok((id, name)) => show_channel(hwnd, app, Channel::named(&id, &name)),
            Err(why) => not_found(
                hwnd,
                app,
                &format!("That channel could not be found: {why}"),
            ),
        },
        Found::Follow(result) => match result {
            Ok((id, name)) => follow(hwnd, app, id, name),
            Err(why) => problem(hwnd, &format!("No channel was followed: {why}")),
        },
        Found::Listen { url, name } => play_address(hwnd, app, url, name),
        Found::Unfollow(result) => match result {
            Ok((id, name)) => unfollow(hwnd, app, &Channel::named(&id, &name)),
            Err(why) => problem(hwnd, &format!("The channel could not be found: {why}")),
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

/// Ctrl+F: asks what to search for, and how, then searches, in the YouTube
/// window.
fn search(hwnd: HWND, app: &mut App) {
    let Some(query) = crate::searchbox::ask(hwnd, &app.last_search) else {
        say(hwnd, "Nothing searched.");
        focus(hwnd, IDC_LIST);
        return;
    };
    app.last_search = query.clone();
    open(hwnd, app, move |b, app| start_search(b, app, query));
}

fn start_search(hwnd: HWND, app: &mut App, query: Query) {
    waiting(
        hwnd,
        app,
        Shows::Search(query.clone()),
        &format!("Searching YouTube for {}.", query.text),
    );
    // yt-dlp can stand in only for a plain search for videos.
    let plain = query.what == SearchFor::Videos
        && query.sort == 0
        && query.uploaded == 0
        && query.length == 0;
    let exe = ytdlp::find().filter(|_| plain);
    look(hwnd, app, move |stop| {
        let direct = youtube::search(&query, None, stop);
        if stop.load(Ordering::Relaxed) {
            return None;
        }
        let worked = direct.as_ref().is_ok_and(|r| r.found() > 0);
        let (result, slow) = match (worked, exe) {
            (false, Some(exe)) => match ytdlp::search(&exe, &query.text, stop) {
                Ok(Some(videos)) => (
                    Ok(youtube::Results {
                        videos,
                        ..Default::default()
                    }),
                    true,
                ),
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

/// Enter on "More": the next page of what is shown.
fn more(hwnd: HWND, app: &mut App, token: String) {
    let shows = app.online.last().map(|o| o.shows.clone());
    say(hwnd, "Fetching more.");
    match shows {
        Some(Shows::Search(query)) => look(hwnd, app, move |stop| {
            let result = youtube::search(&query, Some(&token), stop);
            Some(Found::Search {
                query,
                more: true,
                result,
                slow: false,
            })
        }),
        Some(Shows::Channel(channel)) => look(hwnd, app, move |stop| {
            let result = youtube::channel_more(&channel, &token, stop);
            Some(Found::Channel {
                channel,
                more: true,
                result,
            })
        }),
        Some(Shows::Related(id, title)) => look(hwnd, app, move |stop| {
            let result = youtube::related(&id, Some(&token), stop);
            Some(Found::Related {
                id,
                title,
                more: true,
                result,
            })
        }),
        Some(Shows::ChannelSearch(channel, query)) => look(hwnd, app, move |stop| {
            let result = youtube::channel_search(&channel, &query, Some(&token), stop);
            Some(Found::ChannelSearch {
                channel,
                query,
                more: true,
                result,
            })
        }),
        Some(Shows::Playlist(playlist)) => look(hwnd, app, move |stop| {
            let result = youtube::playlist_videos(&playlist.id, Some(&token), stop);
            Some(Found::Playlist {
                playlist,
                more: true,
                result,
            })
        }),
        _ => {}
    }
}

/// More videos, at the end of the list shown.
fn appended(hwnd: HWND, app: &mut App, r: youtube::Results) {
    let Some(o) = app.online.last_mut() else {
        return;
    };
    o.rows.retain(|r| !matches!(r, Row::More(_)));
    // A channel's last line, its playlists, stays last.
    let last = o
        .rows
        .last()
        .is_some_and(|r| matches!(r, Row::Playlists(_)))
        .then(|| o.rows.pop())
        .flatten();
    let first = o.rows.len();
    let rows = rows_of(r);
    let n = rows.iter().filter(|r| !matches!(r, Row::More(_))).count();
    o.rows.extend(rows);
    o.rows.extend(last);
    o.at = first.min(o.rows.len().saturating_sub(1));
    show_top(hwnd, app);
    if n == 0 {
        say(hwnd, "That was all: there are no more.");
    } else {
        say(
            hwnd,
            &format!(
                "{}, added at the end of the list.",
                count_of(n, "more result", "more results")
            ),
        );
    }
    focus(hwnd, IDC_LIST);
}

/// The number at the start of a playlist's count, "95 videos".
fn count_in(count: &str) -> Option<usize> {
    count
        .split_whitespace()
        .next()?
        .replace(',', "")
        .parse()
        .ok()
}

/// The rows for what YouTube found, with More last.
fn rows_of(r: youtube::Results) -> Vec<Row> {
    let mut rows: Vec<Row> = r
        .channels
        .iter()
        .map(|(id, name)| Row::Channel(Channel::named(id, name)))
        .collect();
    rows.extend(r.videos.into_iter().map(|v| Row::Video { v, new: false }));
    rows.extend(r.playlists.into_iter().map(Row::Playlist));
    rows.extend(r.more.map(Row::More));
    rows
}

fn searched(
    hwnd: HWND,
    app: &mut App,
    query: Query,
    more: bool,
    result: Result<youtube::Results, String>,
    slow: bool,
) {
    let r = match result {
        Ok(r) => r,
        Err(why) => {
            not_found(
                hwnd,
                app,
                &format!("YouTube could not be searched for {}: {why}", query.text),
            );
            return;
        }
    };
    if more {
        appended(hwnd, app, r);
        return;
    }
    if r.found() == 0 {
        if let Some(o) = app.online.last_mut() {
            o.loading = false;
        }
        say(
            hwnd,
            &format!(
                "YouTube found nothing for {}. Ctrl+F searches again.",
                query.text
            ),
        );
        return;
    }
    let n = r.found();
    let what = match query.what {
        SearchFor::Videos => count_of(n, "video", "videos"),
        SearchFor::Channels => count_of(n, "channel", "channels"),
        SearchFor::Playlists => count_of(n, "playlist", "playlists"),
    };
    let keys = if query.what == SearchFor::Videos {
        " Enter plays the sound; Ctrl+D downloads."
    } else {
        " Enter opens one."
    };
    let text = query.text.clone();
    let slow = if slow {
        " YouTube's own search did not answer, so yt-dlp searched instead."
    } else {
        ""
    };
    arrive(
        hwnd,
        app,
        Online {
            shows: Shows::Search(query.clone()),
            rows: rows_of(r),
            at: 0,
            loading: false,
        },
        &format!("{what} found for {text}.{keys}{slow}"),
    );
}

/// The videos YouTube suggests beside one.
fn show_related(hwnd: HWND, app: &mut App, id: String, title: String) {
    waiting(
        hwnd,
        app,
        Shows::Related(id.clone(), title.clone()),
        &format!("Getting videos related to {title}."),
    );
    look(hwnd, app, move |stop| {
        let result = youtube::related(&id, None, stop);
        if stop.load(Ordering::Relaxed) {
            return None;
        }
        Some(Found::Related {
            id,
            title,
            more: false,
            result,
        })
    });
}

fn related_found(hwnd: HWND, app: &mut App, id: String, title: String, r: youtube::Results) {
    let n = r.found();
    let said = format!(
        "{} related to {title}. Enter plays one; Ctrl+R shows those related to it; Backspace \
         goes back.",
        count_of(n, "video", "videos")
    );
    arrive(
        hwnd,
        app,
        Online {
            shows: Shows::Related(id, title),
            rows: rows_of(r),
            at: 0,
            loading: false,
        },
        &said,
    );
}

/// Asks what to look for in `channel`, then searches it.
fn search_channel(hwnd: HWND, app: &mut App, channel: Channel) {
    let label = format!("&Search {} for:", plain(&channel.name));
    let Some(query) = crate::namebox::ask(
        hwnd,
        "Search this channel - VidFetch",
        &label,
        "",
        check_query,
    ) else {
        say(hwnd, "Nothing searched.");
        focus(hwnd, IDC_LIST);
        return;
    };
    let query = query.trim().to_string();
    waiting(
        hwnd,
        app,
        Shows::ChannelSearch(channel.clone(), query.clone()),
        &format!("Searching {} for {query}.", channel.name),
    );
    look(hwnd, app, move |stop| {
        let result = youtube::channel_search(&channel, &query, None, stop);
        if stop.load(Ordering::Relaxed) {
            return None;
        }
        Some(Found::ChannelSearch {
            channel,
            query,
            more: false,
            result,
        })
    });
}

fn channel_searched(
    hwnd: HWND,
    app: &mut App,
    channel: Channel,
    query: String,
    r: youtube::Results,
) {
    if r.found() == 0 {
        if let Some(o) = app.online.last_mut() {
            o.loading = false;
        }
        say(hwnd, &format!("{} has nothing for {query}.", channel.name));
        return;
    }
    let said = format!(
        "{} found in {} for {query}.",
        count_of(r.found(), "video", "videos"),
        channel.name
    );
    arrive(
        hwnd,
        app,
        Online {
            shows: Shows::ChannelSearch(channel, query),
            rows: rows_of(r),
            at: 0,
            loading: false,
        },
        &said,
    );
}

/// Fetches a video's comments, to read in a window of their own.
fn show_comments(hwnd: HWND, app: &mut App, id: String, name: String) {
    say(hwnd, &format!("Fetching the comments on {name}."));
    look(hwnd, app, move |stop| {
        let result = youtube::comments(&id, stop);
        if stop.load(Ordering::Relaxed) {
            return None;
        }
        Some(Found::Comments { name, result })
    });
}

/// Fetches a video's captions as text, without downloading the video.
fn show_transcript(hwnd: HWND, app: &mut App, id: String, name: String) {
    if !may_use_ytdlp(hwnd) {
        return;
    }
    say(hwnd, &format!("Fetching the transcript of {name}."));
    look(hwnd, app, move |stop| {
        let exe = ytdlp_exe()?;
        let url = format!("https://www.youtube.com/watch?v={id}");
        let dir = std::env::temp_dir().join(format!("vidfetch-transcript-{id}"));
        let _ = std::fs::create_dir_all(&dir);
        let got = ytdlp::captions(&exe, &url, &dir, stop);
        if stop.load(Ordering::Relaxed) {
            let _ = std::fs::remove_dir_all(&dir);
            return None;
        }
        let result = match got {
            Ok(Some((file, _))) => std::fs::read_to_string(&file)
                .map(|t| crate::captions::as_text(&crate::captions::parse(&t)))
                .map_err(|e| e.to_string()),
            Ok(None) => Err("this video has no captions, in any language.".to_string()),
            Err(e) => Err(e),
        };
        let _ = std::fs::remove_dir_all(&dir);
        Some(Found::Transcript { name, result })
    });
}

// ---------------------------------------------------------------- listening

/// The address of a video's sound, found on a thread, or at once if it was
/// found lately: yt-dlp takes about 4 seconds.
fn sound_of(app: &App, url: &str) -> mpsc::Receiver<Result<String, String>> {
    let (tx, rx) = mpsc::channel();
    let sounds = app.sounds.clone();
    let url = url.to_string();
    std::thread::spawn(move || {
        let _ = tx.send(find_sound(&sounds, &url));
    });
    rx
}

/// Finds the next one's sound while this one plays, so it starts at once.
fn prefetch(app: &App, url: &str) {
    let fresh = match app.sounds.lock().unwrap().get(url) {
        Some(Sound::Finding) => true,
        Some(Sound::Found(_, when)) => when.elapsed() < FRESH,
        None => false,
    };
    if !fresh {
        let sounds = app.sounds.clone();
        let url = url.to_string();
        std::thread::spawn(move || {
            let _ = find_sound(&sounds, &url);
        });
    }
}

/// The user has agreed to fetching yt-dlp, for listening.
static AGREED: AtomicBool = AtomicBool::new(false);
/// One thread fetches it; the others wait for that one.
static FETCHING: Mutex<()> = Mutex::new(());

/// Whether yt-dlp is here or may be fetched, asking if need be. A first-time
/// user's first act is likely to be listening, not downloading.
fn may_use_ytdlp(hwnd: HWND) -> bool {
    if ytdlp::find().is_some() || AGREED.load(Ordering::Relaxed) {
        return true;
    }
    if message::ask(
        hwnd,
        "VidFetch plays and downloads videos with yt-dlp, a free program that is not on this \
         computer yet. Fetch it now from its own page on GitHub? The first time takes about a \
         minute.",
        "&Fetch it",
        "&Cancel",
        true,
    ) {
        AGREED.store(true, Ordering::Relaxed);
        return true;
    }
    say(hwnd, "Listening needs yt-dlp, so nothing was played.");
    focus(hwnd, IDC_LIST);
    false
}

/// yt-dlp, fetched first if the user has agreed; for a thread to call.
fn ytdlp_exe() -> Option<PathBuf> {
    if let Some(exe) = ytdlp::find() {
        return Some(exe);
    }
    if !AGREED.load(Ordering::Relaxed) {
        return None;
    }
    let _one = FETCHING.lock().unwrap();
    ytdlp::find().or_else(|| {
        ytdlp::fetch(&AtomicBool::new(false), &mut |_, _| {})
            .ok()
            .flatten()
    })
}

fn find_sound(sounds: &Sounds, url: &str) -> Result<String, String> {
    let started = Instant::now();
    loop {
        {
            let mut all = sounds.lock().unwrap();
            match all.get(url) {
                Some(Sound::Found(a, when)) if when.elapsed() < FRESH => return Ok(a.clone()),
                // Another thread is finding it: wait for that.
                Some(Sound::Finding) if started.elapsed() < Duration::from_secs(60) => {}
                _ => {
                    all.insert(url.to_string(), Sound::Finding);
                    break;
                }
            }
        }
        std::thread::sleep(Duration::from_millis(100));
    }
    let r = match ytdlp_exe() {
        None => Err(
            "listening needs yt-dlp, which VidFetch fetches the first time you \
                     download something."
                .to_string(),
        ),
        Some(exe) => {
            let stop = AtomicBool::new(false);
            ytdlp::sound_address(&exe, url, &stop)
                .and_then(|a| a.ok_or_else(|| "it was stopped.".to_string()))
        }
    };
    let mut all = sounds.lock().unwrap();
    match &r {
        Ok(a) => {
            all.insert(url.to_string(), Sound::Found(a.clone(), Instant::now()));
        }
        Err(_) => {
            all.remove(url);
        }
    }
    r
}

/// What the Play window needs for the row at `i`: a video's sound comes
/// from YouTube, a download's from its file. The next video's sound is
/// found meanwhile.
fn track_at(hwnd: HWND, app: &mut App, i: usize) -> Option<listen::Track> {
    let o = app.online.last()?;
    let row = o.rows.get(i)?.clone();
    let has_next = o.rows[i + 1..].iter().any(Row::playable);
    let has_previous = o.rows[..i].iter().any(Row::playable);
    let next = o.rows[i + 1..]
        .iter()
        .find(|r| r.playable())
        .and_then(Row::video);
    if let Some(n) = next {
        prefetch(app, &n.url());
    }
    if let Some(v) = row.video() {
        history::record(&places::of_video(&v.id), &v.title);
        let source = listen::Source::Coming(sound_of(app, &v.url()));
        let key = places::of_video(&v.id);
        let (name, video) = (v.title.clone(), Some(v.id.clone()));
        return Some(listen::Track {
            source,
            name,
            key,
            video,
            page: Some(v.url()),
            has_next,
            has_previous,
            download: Some(Box::new(move || download_video(hwnd, &v))),
        });
    }
    let Row::Played(p) = row else {
        return None;
    };
    history::record(&p.key, &p.name);
    let path = PathBuf::from(&p.key);
    Some(listen::Track {
        source: listen::Source::Ready(p.key.clone()),
        name: p.name.clone(),
        key: p.key.clone(),
        video: library::video_of(Config::past_path().as_deref(), &path),
        page: None,
        has_next,
        has_previous,
        download: None,
    })
}

/// Enter on a video, or on something played lately: the Play window, which
/// goes on through the list with Page Down and Page Up.
fn play_row(hwnd: HWND, app: &mut App, i: usize) {
    if matches!(top_row(app, Some(i)), Some(Row::Video { .. })) && !may_use_ytdlp(hwnd) {
        return;
    }
    if let Some(Row::Played(p)) = top_row(app, Some(i)) {
        if p.video().is_none() && !std::path::Path::new(&p.key).exists() {
            let p = p.clone();
            download_gone_file(hwnd, &PathBuf::from(&p.key), &p.name);
            return;
        }
    }
    let Some(track) = track_at(hwnd, app, i) else {
        return;
    };
    let (speed, play_next, cut) = (app.cfg.speed, app.cfg.play_next, app.cfg.cut_silence);
    let mut step = move |next: bool| -> Option<listen::Track> {
        let app = unsafe { super::app(hwnd) }?;
        let lb = list(hwnd);
        let at = downloads::chosen(lb)?;
        let o = app.online.last()?;
        let to = if next {
            (at + 1..o.rows.len()).find(|&i| o.rows[i].playable())
        } else {
            (0..at).rev().find(|&i| o.rows[i].playable())
        }?;
        unsafe {
            SendMessageW(lb, LB_SETCURSEL, Some(WPARAM(to)), None);
        }
        name_enter(hwnd, app);
        track_at(hwnd, app, to)
    };
    let mut l = listen::Listen {
        track,
        speed,
        play_next,
        cut_silence: cut,
        step: Some(&mut step),
    };
    let r = listen::show(hwnd, &mut l);
    let (speed, name) = (l.speed, l.track.name.clone());
    drop(l);
    let Some(app) = (unsafe { super::app(hwnd) }) else {
        return;
    };
    keep_speed(app, speed);
    match r {
        Err(why) => problem(hwnd, &format!("{name} could not be played: {why}")),
        // Shown, not said: the Play window said it all.
        Ok(()) => set_text(
            hwnd,
            IDC_STATUS,
            &format!("Stopped playing {name}. Enter carries on from there."),
        ),
    }
    focus(hwnd, IDC_LIST);
}

/// Enter on a download whose file has gone: offers to download it again,
/// if VidFetch knows where it came from (log entry 5, Q3).
pub fn download_gone_file(hwnd: HWND, path: &std::path::Path, name: &str) {
    let Some(id) = library::video_of(Config::past_path().as_deref(), path) else {
        say(
            hwnd,
            &format!(
                "{name} is no longer on this computer, and VidFetch does not know where it came \
                 from. Delete takes it off this list."
            ),
        );
        return;
    };
    if !message::ask(
        hwnd,
        &format!("{name} is no longer on this computer. Download it again?"),
        "&Download again",
        "&Cancel",
        true,
    ) {
        say(hwnd, "Not downloaded.");
        focus(hwnd, IDC_LIST);
        return;
    }
    let Some(app) = (unsafe { super::app(hwnd) }) else {
        return;
    };
    let main = main_of(hwnd, app);
    let folder = path
        .parent()
        .filter(|f| f.is_dir())
        .map(PathBuf::from)
        .unwrap_or_else(|| app.cfg.download_folder());
    HUSH.with(|h| h.set(true));
    submit(
        main,
        app,
        format!("https://www.youtube.com/watch?v={id}"),
        false,
        None,
        folder,
    );
    HUSH.with(|h| h.set(false));
    say(hwnd, &get_text(main, IDC_STATUS));
    focus(hwnd, IDC_LIST);
}

/// Listen without downloading, from New download or a copied address: its
/// name is found first, while its sound is found alongside.
pub fn listen_address(hwnd: HWND, app: &mut App, url: String) {
    if ytdlp::is_youtube(&url) && ytdlp::video_id(&url).is_none() {
        open_address(hwnd, app, url);
        return;
    }
    let url = match ytdlp::video_id(&url).filter(|_| ytdlp::is_youtube(&url)) {
        Some(id) => format!("https://www.youtube.com/watch?v={id}"),
        None => url,
    };
    if !may_use_ytdlp(hwnd) {
        return;
    }
    say(hwnd, "Please wait.");
    prefetch(app, &url);
    look(hwnd, app, move |stop| {
        let name = ytdlp_exe()
            .and_then(|exe| ytdlp::title(&exe, &url, false, stop))
            .unwrap_or_else(|| url.clone());
        if stop.load(Ordering::Relaxed) {
            return None;
        }
        Some(Found::Listen { url, name })
    });
}

/// A playlist's or channel's address: opens its list of videos in the YouTube
/// window, to play or download from (milestone 6 test, 3.4).
fn open_address(hwnd: HWND, app: &mut App, url: String) {
    if let Some(id) = ytdlp::list_id(&url) {
        let playlist = Playlist {
            id,
            ..Playlist::default()
        };
        open(hwnd, app, move |b, app| show_playlist(b, app, playlist));
        return;
    }
    open(hwnd, app, move |b, app| {
        waiting(
            b,
            app,
            Shows::Channel(Channel::named("", "the channel")),
            "Finding the channel.",
        );
        look(b, app, move |stop| {
            let r = youtube::channel_of(&url, stop);
            if stop.load(Ordering::Relaxed) {
                return None;
            }
            Some(Found::OpenChannel(r))
        });
    });
}

/// Plays one web address, on its own.
fn play_address(hwnd: HWND, app: &mut App, url: String, name: String) {
    let video = ytdlp::video_id(&url).filter(|_| ytdlp::is_youtube(&url));
    let key = match &video {
        Some(id) => {
            let key = places::of_video(id);
            history::record(&key, &name);
            key
        }
        None => url.clone(),
    };
    let address = url.clone();
    let track = listen::Track {
        source: listen::Source::Coming(sound_of(app, &url)),
        name: name.clone(),
        key,
        video,
        page: Some(url),
        has_next: false,
        has_previous: false,
        download: Some(Box::new(move || download_address(hwnd, &address))),
    };
    let mut l = listen::Listen {
        track,
        speed: app.cfg.speed,
        play_next: false,
        cut_silence: app.cfg.cut_silence,
        step: None,
    };
    let r = listen::show(hwnd, &mut l);
    let speed = l.speed;
    let Some(app) = (unsafe { super::app(hwnd) }) else {
        return;
    };
    keep_speed(app, speed);
    match r {
        Err(why) => problem(hwnd, &format!("{name} could not be played: {why}")),
        Ok(()) => set_text(hwnd, IDC_STATUS, &format!("Stopped playing {name}.")),
    }
    focus(hwnd, IDC_LIST);
}

/// Ctrl+D while listening to a web address; what to say.
fn download_address(hwnd: HWND, url: &str) -> String {
    let Some(app) = (unsafe { super::app(hwnd) }) else {
        return String::new();
    };
    let main = main_of(hwnd, app);
    if !download_again(main, app, url) {
        return "Not downloaded again.".into();
    }
    let folder = app.cfg.download_folder();
    HUSH.with(|h| h.set(true));
    submit(main, app, url.to_string(), false, None, folder);
    HUSH.with(|h| h.set(false));
    get_text(main, IDC_STATUS)
}

/// Ctrl+H: what was played lately.
fn recently_played(hwnd: HWND, app: &mut App) {
    let all = history::load();
    if all.is_empty() {
        say(
            hwnd,
            "Nothing played yet. What you play, downloaded or from YouTube, is listed in Recently \
             played, with where you stopped.",
        );
        return;
    }
    open(hwnd, app, move |b, app| {
        let n = all.len();
        let replace = matches!(app.online.last().map(|o| &o.shows), Some(Shows::History));
        push(
            b,
            app,
            Online {
                shows: Shows::History,
                rows: all.into_iter().map(Row::Played).collect(),
                at: 0,
                loading: false,
            },
            replace,
            &format!(
                "{}, newest first. Enter carries on where you stopped; Delete takes one off the \
                 list.",
                count_of(n, "recently played item", "recently played items")
            ),
        );
    });
}

// ---------------------------------------------------------------- downloading

/// Downloads `v` with the options as they stand, into the category shown,
/// now or in its turn; what to say, which the window in front says.
pub fn download_video(hwnd: HWND, v: &Video) -> String {
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
    let main = main_of(hwnd, app);
    let folder = app.cfg.download_folder();
    HUSH.with(|h| h.set(true));
    submit(main, app, v.url(), false, None, folder);
    HUSH.with(|h| h.set(false));
    get_text(main, IDC_STATUS)
}

/// Asks, then downloads every video of `url`, a channel's or a playlist's,
/// into a folder of its own.
fn download_all(hwnd: HWND, app: &mut App, url: String, question: &str) {
    if !message::ask(hwnd, question, "&Download them all", "&Cancel", false) {
        say(hwnd, "Nothing downloaded.");
        focus(hwnd, IDC_LIST);
        return;
    }
    let main = main_of(hwnd, app);
    let folder = app.cfg.download_folder();
    HUSH.with(|h| h.set(true));
    submit(main, app, url, true, None, folder);
    HUSH.with(|h| h.set(false));
    say(hwnd, &get_text(main, IDC_STATUS));
    focus(hwnd, IDC_LIST);
}

// ---------------------------------------------------------------- channels

const NO_CHANNELS: &str = "You follow no channels yet. Follow one from the context menu of a \
                           video YouTube found, or of one of your downloads, or with its address \
                           in New download, Ctrl+N.";

/// Ctrl+L: what is new from the channels followed.
fn new_videos(hwnd: HWND, app: &mut App) {
    if app.channels.is_empty() {
        say(hwnd, NO_CHANNELS);
        return;
    }
    open(hwnd, app, |b, app| {
        let list = app.channels.clone();
        waiting(
            b,
            app,
            Shows::New,
            &format!(
                "Looking for new videos from your {}.",
                count_of(list.len(), "channel", "channels")
            ),
        );
        look(b, app, move |stop| {
            let mut videos = Vec::new();
            let mut listed = Vec::new();
            let mut failed = Vec::new();
            for group in list.chunks(AT_ONCE) {
                let got: Vec<Result<(String, Vec<Video>), String>> = std::thread::scope(|sc| {
                    let threads: Vec<_> = group
                        .iter()
                        .map(|c| sc.spawn(move || youtube::channel_videos(&c.id, stop)))
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
                        Ok((name, vs)) => {
                            listed.push((
                                c.id.clone(),
                                name,
                                vs.iter().map(|v| v.id.clone()).collect(),
                            ));
                            videos.extend(vs.into_iter().map(|v| {
                                let new = c.is_new(&v.id, v.published);
                                (v, new)
                            }));
                        }
                        Err(e) => failed.push(format!("{}: {e}", c.name)),
                    }
                }
            }
            videos.sort_by_key(|(v, _)| std::cmp::Reverse(v.published));
            Some(Found::New {
                videos,
                listed,
                failed,
            })
        });
    });
}

fn new_found(
    hwnd: HWND,
    app: &mut App,
    videos: Vec<(Video, bool)>,
    listed: Vec<(String, String, Vec<String>)>,
    failed: Vec<String>,
) {
    let fresh = videos.iter().filter(|(_, new)| *new).count();
    let total = videos.len();
    let channels = count_of(app.channels.len(), "channel", "channels");
    // Looked at now: next time, new is what was not listed this time.
    let now = youtube::now();
    for (id, name, ids) in listed {
        if let Some(c) = app.channels.iter_mut().find(|c| c.id == id) {
            c.looked_at(now, ids);
            if !name.is_empty() {
                c.name = name;
            }
        }
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
    let rows = videos
        .into_iter()
        .map(|(v, new)| Row::Video { v, new })
        .collect();
    arrive(
        hwnd,
        app,
        Online {
            shows: Shows::New,
            rows,
            at: 0,
            loading: false,
        },
        &said,
    );
}

/// Ctrl+Shift+L: the channels followed.
fn your_channels(hwnd: HWND, app: &mut App) {
    if app.channels.is_empty() {
        say(hwnd, NO_CHANNELS);
        return;
    }
    open(hwnd, app, |b, app| {
        let rows = app.channels.iter().cloned().map(Row::Channel).collect();
        let replace = matches!(app.online.last().map(|o| &o.shows), Some(Shows::Channels));
        let said = format!(
            "You follow {}. Enter shows one's latest videos; Delete stops following it.",
            count_of(app.channels.len(), "channel", "channels")
        );
        push(
            b,
            app,
            Online {
                shows: Shows::Channels,
                rows,
                at: 0,
                loading: false,
            },
            replace,
            &said,
        );
    });
}

/// A channel's latest videos, and a line for its playlists.
fn show_channel(hwnd: HWND, app: &mut App, channel: Channel) {
    waiting(
        hwnd,
        app,
        Shows::Channel(channel.clone()),
        &format!("Getting the latest videos from {}.", channel.name),
    );
    look(hwnd, app, move |stop| {
        let result = youtube::channel_page(&channel.id, stop).map(|(_, r)| r);
        if stop.load(Ordering::Relaxed) {
            return None;
        }
        Some(Found::Channel {
            channel,
            more: false,
            result,
        })
    });
}

fn channel_found(hwnd: HWND, app: &mut App, channel: Channel, r: youtube::Results) {
    let followed = app.channels.iter().any(|c| c.id == channel.id);
    let said = format!(
        "The latest {} from {}, newest first. {}The last line lists its playlists.{}",
        count_of(r.videos.len(), "video", "videos"),
        channel.name,
        if r.more.is_some() {
            "More, near the end, fetches older ones. "
        } else {
            ""
        },
        if followed {
            ""
        } else {
            " You do not follow this channel; the context menu can."
        }
    );
    let mut rows = rows_of(r);
    rows.push(Row::Playlists(channel.clone()));
    arrive(
        hwnd,
        app,
        Online {
            shows: Shows::Channel(channel),
            rows,
            at: 0,
            loading: false,
        },
        &said,
    );
}

/// A channel's playlists (milestone 4 test, section 4).
fn show_playlists(hwnd: HWND, app: &mut App, channel: Channel) {
    waiting(
        hwnd,
        app,
        Shows::Playlists(channel.clone()),
        &format!("Getting the playlists of {}.", channel.name),
    );
    look(hwnd, app, move |stop| {
        let result = youtube::channel_playlists(&channel.id, stop);
        if stop.load(Ordering::Relaxed) {
            return None;
        }
        Some(Found::Playlists { channel, result })
    });
}

fn playlists_found(hwnd: HWND, app: &mut App, channel: Channel, list: Vec<Playlist>) {
    if list.is_empty() {
        if let Some(o) = app.online.last_mut() {
            o.loading = false;
        }
        say(
            hwnd,
            &format!("{} has no playlists. Backspace goes back.", channel.name),
        );
        return;
    }
    let said = format!(
        "{} of {}. Enter shows one's videos; Ctrl+D downloads a whole playlist.",
        count_of(list.len(), "playlist", "playlists"),
        channel.name
    );
    arrive(
        hwnd,
        app,
        Online {
            shows: Shows::Playlists(channel),
            rows: list.into_iter().map(Row::Playlist).collect(),
            at: 0,
            loading: false,
        },
        &said,
    );
}

/// A playlist's videos, in its order.
fn show_playlist(hwnd: HWND, app: &mut App, playlist: Playlist) {
    waiting(
        hwnd,
        app,
        Shows::Playlist(playlist.clone()),
        &if playlist.title.is_empty() {
            "Getting the playlist's videos.".to_string()
        } else {
            format!("Getting the videos of {}.", playlist.title)
        },
    );
    look(hwnd, app, move |stop| {
        let result = youtube::playlist_videos(&playlist.id, None, stop);
        if stop.load(Ordering::Relaxed) {
            return None;
        }
        Some(Found::Playlist {
            playlist,
            more: false,
            result,
        })
    });
}

fn playlist_found(hwnd: HWND, app: &mut App, mut playlist: Playlist, mut r: youtube::Results) {
    if playlist.title.is_empty() {
        playlist.title = std::mem::take(&mut r.title);
    }
    let n = r.videos.len();
    // YouTube offers a next page even when every video is listed.
    if count_in(&playlist.count).is_some_and(|all| n >= all) {
        r.more = None;
    }
    let more = if r.more.is_some() {
        " More, at the end, fetches the rest."
    } else {
        ""
    };
    let said = format!(
        "{} in the playlist {}, in its order. Enter plays one; Ctrl+D downloads it.{more}",
        count_of(n, "video", "videos"),
        playlist.title
    );
    arrive(
        hwnd,
        app,
        Online {
            shows: Shows::Playlist(playlist),
            rows: rows_of(r),
            at: 0,
            loading: false,
        },
        &said,
    );
}

/// Follows the channel `id`, called `name`.
fn follow(hwnd: HWND, app: &mut App, id: String, name: String) {
    if let Some(c) = app.channels.iter().find(|c| c.id == id) {
        say(hwnd, &format!("You already follow {}.", c.name));
        return;
    }
    let mut c = Channel::named(&id, &name);
    c.looked = youtube::now();
    app.channels.push(c);
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

/// Follows the channel of a video, found by its id.
fn follow_video(hwnd: HWND, app: &mut App, id: String, name: &str) {
    say(hwnd, &format!("Finding the channel of {name}."));
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

/// The main window's commands that open the YouTube window, and those of
/// the YouTube menu on a download. False if `id` is none of them.
pub fn download_command(hwnd: HWND, app: &mut App, id: i32) -> bool {
    match id {
        IDM_SEARCH => search(hwnd, app),
        IDM_NEW_VIDEOS => new_videos(hwnd, app),
        IDM_CHANNELS => your_channels(hwnd, app),
        IDM_HISTORY => recently_played(hwnd, app),
        IDM_DESCRIBE | IDM_FOLLOW | IDM_UNFOLLOW | IDM_COPY | IDM_BROWSER | IDM_RELATED
        | IDM_COMMENTS | IDM_TRANSCRIPT => {
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
            let known = video_of(&e);
            match (id, known) {
                (
                    IDM_DESCRIBE | IDM_FOLLOW | IDM_UNFOLLOW | IDM_RELATED | IDM_COMMENTS
                    | IDM_TRANSCRIPT,
                    None,
                ) => say(
                    hwnd,
                    &format!(
                        "VidFetch does not know which YouTube video {} came from.",
                        e.name
                    ),
                ),
                (IDM_DESCRIBE, Some(v)) => describe_id(hwnd, app, v, e.name.clone()),
                (IDM_RELATED, Some(v)) => {
                    let name = e.name.clone();
                    open(hwnd, app, move |b, app| show_related(b, app, v, name));
                }
                (IDM_COMMENTS, Some(v)) => show_comments(hwnd, app, v, e.name.clone()),
                (IDM_TRANSCRIPT, Some(v)) => show_transcript(hwnd, app, v, e.name.clone()),
                (IDM_FOLLOW, Some(v)) => follow_video(hwnd, app, v, &e.name),
                (IDM_UNFOLLOW, Some(v)) => {
                    say(hwnd, &format!("Finding the channel of {}.", e.name));
                    look(hwnd, app, move |stop| {
                        let r = youtube::details(&v, stop).map(|d| (d.channel_id, d.channel));
                        if stop.load(Ordering::Relaxed) {
                            return None;
                        }
                        Some(Found::Unfollow(r))
                    });
                }
                (_, known) => {
                    let url = match &e.kind {
                        Kind::Playlist { url, .. } if !url.is_empty() => Some(url.clone()),
                        _ => known.map(|v| format!("https://www.youtube.com/watch?v={v}")),
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

/// Whether a download is known to have come from YouTube, for the menus.
pub fn knows_video(e: &library::Entry) -> bool {
    video_of(e).is_some()
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

/// The channel a row is about, or the one whose list is shown.
fn channel_in(app: &App, row: Option<&Row>) -> Option<Channel> {
    match row {
        Some(Row::Channel(c)) | Some(Row::Playlists(c)) => Some(c.clone()),
        Some(Row::Video { v, .. }) if !v.channel_id.is_empty() => {
            Some(Channel::named(&v.channel_id, &v.channel))
        }
        _ => match app.online.last().map(|o| &o.shows) {
            Some(Shows::Channel(c)) | Some(Shows::Playlists(c)) => Some(c.clone()),
            _ => None,
        },
    }
}

fn follows(app: &App, c: &Channel) -> bool {
    app.channels.iter().any(|x| x.id == c.id)
}

/// A command in the YouTube window. False for what it does not know.
fn command(hwnd: HWND, app: &mut App, id: i32) -> bool {
    let row = chosen_row(hwnd, app);
    let r = row.as_ref().map(|(_, r)| r);
    match id {
        IDM_SEARCH => search(hwnd, app),
        IDM_NEW_VIDEOS => new_videos(hwnd, app),
        IDM_CHANNELS => your_channels(hwnd, app),
        IDM_HISTORY => recently_played(hwnd, app),
        IDM_BACK => back(hwnd, app),
        IDM_CLOSE => close(hwnd, app),
        IDM_HELP => help(hwnd),
        IDM_RELATED => match r.and_then(Row::video) {
            Some(v) => show_related(hwnd, app, v.id, v.title),
            None => say(hwnd, "Choose a video in the list first."),
        },
        IDM_COMMENTS => match r.and_then(Row::video) {
            Some(v) => show_comments(hwnd, app, v.id, v.title),
            None => say(hwnd, "Choose a video in the list first."),
        },
        IDM_TRANSCRIPT => match r.and_then(Row::video) {
            Some(v) => show_transcript(hwnd, app, v.id, v.title),
            None => say(hwnd, "Choose a video in the list first."),
        },
        IDM_SEARCH_CHANNEL => match channel_in(app, r) {
            Some(c) => search_channel(hwnd, app, c),
            None => say(hwnd, "Choose a channel, or a video from one, first."),
        },
        IDM_OPEN => match row {
            Some((_, Row::Video { v, .. })) if is_live(&v) => say(hwnd, LIVE),
            Some((i, Row::Video { .. } | Row::Played(_))) => play_row(hwnd, app, i),
            Some((_, Row::Channel(c))) => show_channel(hwnd, app, c),
            Some((_, Row::Playlists(c))) => show_playlists(hwnd, app, c),
            Some((_, Row::Playlist(p))) => show_playlist(hwnd, app, p),
            Some((_, Row::More(t))) => more(hwnd, app, t),
            None => say(hwnd, "The list is empty. Backspace goes back."),
        },
        IDM_DOWNLOAD => match row {
            Some((_, Row::Channel(c))) | Some((_, Row::Playlists(c))) => download_all(
                hwnd,
                app,
                c.videos_url(),
                &format!(
                    "Download every video of {}, into a folder of its own? A channel can have \
                     hundreds. Ctrl+U fetches only its new ones later.",
                    c.name
                ),
            ),
            Some((_, Row::Playlist(p))) => download_all(
                hwnd,
                app,
                p.url(),
                &format!(
                    "Download the whole playlist {}{}, into a folder of its own? Ctrl+U fetches \
                     only its new videos later.",
                    p.title,
                    if p.count.is_empty() {
                        String::new()
                    } else {
                        format!(", {}", p.count)
                    }
                ),
            ),
            Some((_, Row::Video { v, .. })) if is_live(&v) => say(hwnd, LIVE),
            Some((_, row)) => match row.video() {
                Some(v) => {
                    let said = download_video(hwnd, &v);
                    say(hwnd, &said);
                    focus(hwnd, IDC_LIST);
                }
                None => say(hwnd, "That is a download already."),
            },
            None => say(hwnd, "Choose a video in the list first."),
        },
        IDM_DESCRIBE => match r.and_then(Row::video) {
            Some(v) if !v.description.trim().is_empty() => {
                message::description(
                    hwnd,
                    &format!(
                        "{}\nBy {}, {}.\n\n{}",
                        v.title, v.channel, v.when, v.description
                    ),
                );
                focus(hwnd, IDC_LIST);
            }
            Some(v) => describe_id(hwnd, app, v.id.clone(), v.title.clone()),
            None => say(hwnd, "Choose a video in the list first."),
        },
        IDM_CHANNEL => match (channel_in(app, r), r.and_then(Row::video)) {
            (Some(c), _) => show_channel(hwnd, app, c),
            // Related videos do not say their channel's id.
            (None, Some(v)) => {
                say(hwnd, "Finding the channel.");
                look(hwnd, app, move |stop| {
                    let r = youtube::details(&v.id, stop).map(|d| (d.channel_id, d.channel));
                    if stop.load(Ordering::Relaxed) {
                        return None;
                    }
                    Some(Found::OpenChannel(r))
                });
            }
            (None, None) => say(hwnd, "VidFetch does not know this one's channel."),
        },
        IDM_PLAYLISTS => match channel_in(app, r) {
            Some(c) => show_playlists(hwnd, app, c),
            None => say(hwnd, "VidFetch does not know this one's channel."),
        },
        IDM_FOLLOW => match (r, channel_in(app, r)) {
            (_, Some(c)) => follow(hwnd, app, c.id, c.name),
            (Some(row), None) => match row.video() {
                Some(v) => follow_video(hwnd, app, v.id.clone(), &v.title),
                None => say(hwnd, "Choose a video in the list first."),
            },
            (None, None) => say(hwnd, "Choose a video in the list first."),
        },
        IDM_DELETE if matches!(r, Some(Row::Played(_))) => {
            if let Some((at, Row::Played(p))) = row {
                history::remove(&p.key);
                if let Some(o) = app.online.last_mut() {
                    o.rows.remove(at);
                    o.at = at.min(o.rows.len().saturating_sub(1));
                }
                show_top(hwnd, app);
                let left = app.online.last().map_or(0, |o| o.rows.len());
                let left = if left == 0 {
                    "The list is empty now; Backspace goes back.".to_string()
                } else {
                    format!("{} left.", count_of(left, "item", "items"))
                };
                say(
                    hwnd,
                    &format!("Took {} off the list; nothing was deleted. {left}", p.name),
                );
            }
        }
        IDM_UNFOLLOW | IDM_DELETE => match channel_in(app, r) {
            Some(c) if id == IDM_UNFOLLOW || matches!(r, Some(Row::Channel(_))) => {
                unfollow(hwnd, app, &c)
            }
            _ if id == IDM_DELETE => say(
                hwnd,
                "Delete takes something off Recently played, or stops following a channel in \
                 Your channels. Downloads are deleted in the main window.",
            ),
            _ => say(hwnd, "Choose a channel, or a video from one, first."),
        },
        IDM_COPY | IDM_BROWSER => {
            let url_name = match r {
                Some(Row::Channel(c)) | Some(Row::Playlists(c)) => Some((c.url(), c.name.clone())),
                Some(Row::Playlist(p)) => Some((p.url(), p.title.clone())),
                Some(row) => row.video().map(|v| (v.url(), v.title)),
                None => None,
            };
            match url_name {
                Some((url, name)) => address(hwnd, &url, &name, id == IDM_COPY),
                None => say(
                    hwnd,
                    "That is a download, with no web address VidFetch knows.",
                ),
            }
        }
        _ => return false,
    }
    true
}

/// Greys out what cannot apply to the row chosen, as the user asked
/// (milestone 4 test, section 1); the buttons stay as they are.
fn init_menu(hwnd: HWND, app: &App, menu: HMENU) {
    let row = chosen_row(hwnd, app).map(|(_, r)| r);
    let r = row.as_ref();
    let video = r.and_then(Row::video);
    let channel = channel_in(app, r);
    let followed = channel.as_ref().is_some_and(|c| follows(app, c));
    let whole = matches!(
        r,
        Some(Row::Channel(_) | Row::Playlists(_) | Row::Playlist(_))
    );
    let enable = |id: i32, on: bool| unsafe {
        let _ = EnableMenuItem(
            menu,
            id as u32,
            MF_BYCOMMAND | if on { MF_ENABLED } else { MF_GRAYED },
        );
    };
    enable(IDM_OPEN, r.is_some());
    enable(IDM_DOWNLOAD, video.is_some() || whole);
    enable(IDM_DESCRIBE, video.is_some());
    enable(
        IDM_CHANNEL,
        (channel.is_some() || video.is_some())
            && !matches!(app.online.last().map(|o| &o.shows), Some(Shows::Channel(_))),
    );
    enable(IDM_RELATED, video.is_some());
    enable(IDM_COMMENTS, video.is_some());
    enable(IDM_TRANSCRIPT, video.is_some());
    enable(IDM_SEARCH_CHANNEL, channel.is_some());
    enable(IDM_PLAYLISTS, channel.is_some());
    enable(
        IDM_FOLLOW,
        (channel.is_some() && !followed) || (channel.is_none() && video.is_some()),
    );
    enable(IDM_UNFOLLOW, followed);
    enable(IDM_COPY, video.is_some() || whole);
    enable(IDM_BROWSER, video.is_some() || whole);
    enable(
        IDM_DELETE,
        matches!(r, Some(Row::Played(_) | Row::Channel(_))),
    );
    enable(IDM_BACK, app.online.len() > 1);
}

/// The context menu for the row chosen.
fn context_menu(hwnd: HWND, app: &mut App, at: LPARAM) {
    let lb = list(hwnd);
    let (x, y) = (
        (at.0 & 0xFFFF) as i16 as i32,
        ((at.0 >> 16) & 0xFFFF) as i16 as i32,
    );
    let mouse = (x, y) != (-1, -1);
    if mouse {
        choose_under(lb, x, y);
    }
    let Some((_, row)) = chosen_row(hwnd, app) else {
        say(hwnd, "The list is empty. Backspace goes back.");
        return;
    };
    let shown_channel = matches!(app.online.last().map(|o| &o.shows), Some(Shows::Channel(_)));
    let channel = channel_in(app, Some(&row));
    let followed = channel.as_ref().is_some_and(|c| follows(app, c));
    let mut items: Vec<(i32, &str)> = match &row {
        Row::Video { .. } => {
            let mut items = vec![
                (IDM_OPEN, "&Play\tEnter"),
                (IDM_DOWNLOAD, "&Download\tCtrl+D"),
                (IDM_DESCRIBE, "Show d&escription\tCtrl+I"),
                (IDM_RELATED, "Related &videos\tCtrl+R"),
                (IDM_COMMENTS, "Co&mments\tCtrl+K"),
                (IDM_TRANSCRIPT, "&Transcript\tCtrl+T"),
            ];
            if !shown_channel {
                items.push((IDM_CHANNEL, "Show &channel"));
            }
            if channel.is_some() {
                items.push((IDM_PLAYLISTS, "Show channel's p&laylists"));
                items.push((IDM_SEARCH_CHANNEL, "Search t&his channel...\tCtrl+Shift+F"));
            }
            if followed {
                items.push((IDM_UNFOLLOW, "&Unfollow channel..."));
            } else {
                items.push((IDM_FOLLOW, "&Follow channel"));
            }
            items.extend([
                (IDM_COPY, "Cop&y web address"),
                (IDM_BROWSER, "Open in &browser"),
            ]);
            items
        }
        Row::Channel(_) => vec![
            (IDM_OPEN, "&Open channel\tEnter"),
            (IDM_PLAYLISTS, "Show p&laylists"),
            (IDM_DOWNLOAD, "&Download all its videos..."),
            (IDM_COPY, "Cop&y web address"),
            (IDM_BROWSER, "Open in &browser"),
            (IDM_SEARCH_CHANNEL, "Search t&his channel...\tCtrl+Shift+F"),
            if followed {
                (IDM_UNFOLLOW, "&Unfollow channel...\tDelete")
            } else {
                (IDM_FOLLOW, "&Follow channel")
            },
        ],
        Row::Playlists(_) => vec![(IDM_OPEN, "Show p&laylists\tEnter")],
        Row::Playlist(_) => vec![
            (IDM_OPEN, "&Open playlist\tEnter"),
            (IDM_DOWNLOAD, "&Download whole playlist...\tCtrl+D"),
            (IDM_COPY, "Cop&y web address"),
            (IDM_BROWSER, "Open in &browser"),
        ],
        Row::More(_) => vec![(IDM_OPEN, "Fetch &more\tEnter")],
        Row::Played(p) => {
            let mut items = vec![(IDM_OPEN, "&Play, from where you stopped\tEnter")];
            if p.video().is_some() {
                items.extend([
                    (IDM_DOWNLOAD, "&Download\tCtrl+D"),
                    (IDM_DESCRIBE, "Show d&escription\tCtrl+I"),
                    (IDM_COPY, "Cop&y web address"),
                ]);
            }
            items.push((IDM_DELETE, "&Remove from list\tDelete"));
            items
        }
    };
    if app.online.len() > 1 {
        items.extend([(0, ""), (IDM_BACK, "&Back\tBackspace")]);
    }
    if let Some(id) = downloads::context_menu(hwnd, lb, mouse.then_some((x, y)), &items) {
        command(hwnd, app, id);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn names_with_an_ampersand_make_no_mnemonic() {
        let o = Online {
            shows: Shows::Search(Query {
                text: "M&S".into(),
                ..Query::default()
            }),
            rows: Vec::new(),
            at: 0,
            loading: false,
        };
        assert_eq!(label(&o), "&What YouTube found for M&&S:");
        assert_eq!(spoken(&label(&o)), "What YouTube found for M&S");
        assert_eq!(title(&o), "YouTube results for M&S - VidFetch");
    }

    #[test]
    fn a_playlists_count_is_read() {
        assert_eq!(count_in("95 videos"), Some(95));
        assert_eq!(count_in("1,204 videos"), Some(1204));
        assert_eq!(count_in(""), None);
    }

    #[test]
    fn the_enter_button_says_what_enter_does() {
        assert_eq!(enter_words(None), "&Play  Enter");
        assert_eq!(
            enter_words(Some(&Row::More("x".into()))),
            "Fetch &more  Enter"
        );
    }
}
