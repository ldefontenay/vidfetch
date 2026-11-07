//! The main window: a modeless dialog from the IDD_MAIN template, so every
//! control is a real Win32 control that a screen reader already knows. Video
//! Scribe's main window, without the describing.
//!
//! Its list shows the downloads, a playlist's videos, or what comes from
//! YouTube (online.rs); the Category to show box picks the category
//! (category.rs).

mod category;
mod online;

use std::cell::Cell;
use std::collections::VecDeque;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

use windows::core::{PCWSTR, PWSTR};
use windows::Win32::Foundation::{HWND, LPARAM, WPARAM};
use windows::Win32::System::Com::{CoCreateInstance, CoTaskMemFree, CLSCTX_INPROC_SERVER};
use windows::Win32::System::LibraryLoader::GetModuleHandleW;
use windows::Win32::UI::Input::KeyboardAndMouse::{
    GetFocus, GetKeyState, IsWindowEnabled, VIRTUAL_KEY, VK_BACK, VK_CONTROL, VK_DELETE, VK_ESCAPE,
    VK_F2, VK_MENU, VK_SHIFT,
};
use windows::Win32::UI::Shell::{
    FileOpenDialog, IFileOpenDialog, IShellItem, SHCreateItemFromParsingName, FOS_FORCEFILESYSTEM,
    FOS_PICKFOLDERS, SIGDN_FILESYSPATH,
};
use windows::Win32::UI::WindowsAndMessaging::GetForegroundWindow;
use windows::Win32::UI::WindowsAndMessaging::*;

use crate::channels;
use crate::config::{Config, FORMATS, QUALITIES};
use crate::downloads;
use crate::job::{self, Event};
use crate::library::{self, Kind};
use crate::listen;
use crate::message;
use crate::queue::{self, Queued};
use crate::wide;
use crate::ytdlp;

const IDD_MAIN: u16 = 100;
const IDC_CATEGORY: i32 = 1002;
const IDC_LIST_LABEL: i32 = 1003;
const IDC_STATUS: i32 = 1004;
const IDC_LIST: i32 = 1005;
const IDC_OPEN: i32 = 1006;
const IDC_ADD: i32 = 1009;
const IDC_RENAME: i32 = 1010;
const IDC_SHOW: i32 = 1011;
const IDC_DELETE: i32 = 1012;
const IDC_UPDATE: i32 = 1013;
const IDC_QUEUE: i32 = 1014;

const IDD_QUEUE: u16 = 109;
const IDC_Q_NOW: i32 = 1100;
const IDC_Q_LIST: i32 = 1101;
const IDC_Q_REMOVE: i32 = 1102;
const IDC_Q_ALL: i32 = 1103;

// Menu commands, in step with vidfetch.rc.
const IDM_FOLDER: i32 = 2002;
const IDM_EXPLORER: i32 = 2003;
const IDM_EXIT: i32 = 2004;
const IDM_UPDATE_YTDLP: i32 = 2005;
const IDM_STOP: i32 = 2011;
const IDM_NUMBER: i32 = 2012;
const IDM_CAPTIONS: i32 = 2013;
const IDM_QUEUE: i32 = 2014;
const IDM_CLEAR_QUEUE: i32 = 2015;
const IDM_BACK: i32 = 2016;
const IDM_EXTERNAL: i32 = 2017;
const IDM_DOWNLOAD: i32 = 2018;
const IDM_DESCRIBE: i32 = 2019;
const IDM_MOVE: i32 = 2020;
const IDM_SEARCH: i32 = 2040;
const IDM_NEW_VIDEOS: i32 = 2041;
const IDM_CHANNELS: i32 = 2042;
const IDM_FOLLOW: i32 = 2043;
const IDM_UNFOLLOW: i32 = 2044;
const IDM_COPY: i32 = 2045;
const IDM_BROWSER: i32 = 2046;
/// In the context menu of a video found online only.
const IDM_CHANNEL: i32 = 2047;
const IDM_NEW_CATEGORY: i32 = 2050;
const IDM_RENAME_CATEGORY: i32 = 2051;
const IDM_DELETE_CATEGORY: i32 = 2052;
const IDM_PLAY_NEXT: i32 = 2060;
const IDM_HISTORY: i32 = 2048;
const IDM_CAPTIONS_TEXT: i32 = 2049;
const IDM_ADD: i32 = 2023;
const IDM_RENAME: i32 = 2024;
const IDM_DELETE: i32 = 2025;
const IDM_SHOW: i32 = 2026;
const IDM_OPEN: i32 = 2027;
const IDM_UPDATE: i32 = 2028;
const IDM_HELP: i32 = 2030;
const IDM_ABOUT: i32 = 2031;
// The first number of each list of choices, filled in as its menu opens.
const IDM_FORMAT: i32 = 2100;
const IDM_QUALITY: i32 = 2200;
const IDM_SPEED: i32 = 2300;

// Where each menu sits, in step with vidfetch.rc.
const OPTIONS_MENU: i32 = 4;
const FORMAT_MENU: i32 = 0;
const QUALITY_MENU: i32 = 1;
const SPEED_MENU: i32 = 4;

/// The speeds the Options menu offers.
const SPEEDS: &[f64] = &[0.5, 0.75, 1.0, 1.25, 1.5, 1.75, 2.0, 2.5, 3.0];

const EM_SETSEL: u32 = 0x00B1;
const EM_SCROLLCARET: u32 = 0x00B7;
const EN_SETFOCUS: u32 = 0x0100;
const LBN_SELCHANGE: u32 = 1;
const CBN_SELCHANGE: u32 = 1;
const LBN_DBLCLK: u32 = 2;

const WM_WELCOME: u32 = WM_APP + 13;
/// A window in front of this one has closed: do what was waiting for that.
const WM_SETTLE: u32 = WM_APP + 14;
/// Offer what was left in the queue when VidFetch last closed.
const WM_RESUME: u32 = WM_APP + 15;

thread_local! {
    /// While set, `say` only shows: a window in front says it instead.
    static HUSH: Cell<bool> = const { Cell::new(false) };
}

const WELCOME: &str = "Welcome to VidFetch. It downloads videos and whole playlists from YouTube \
and many other sites, as video or as sound only.\n\n\
Copy the address of a video or a playlist in your browser, with Alt+D then Ctrl+C, then press \
Ctrl+N here: the address is already filled in, so Enter starts the download. Each download takes \
its place in the list of downloads, where Enter plays it and the Applications key opens a menu of \
what else can be done. A playlist gets a folder of its own: Enter on it shows its videos, and \
Ctrl+U fetches only its new ones.\n\n\
Downloads are sound only to begin with, as an M4A. The Options menu (Alt+O), and the New download \
window, choose video instead, and the picture quality. While one download runs, Ctrl+N adds the \
next to a queue.\n\n\
Ctrl+F searches YouTube: Enter on a video plays its sound straight away, and Ctrl+D downloads it. \
Follow a channel from a video's context menu, and Ctrl+L shows what is new from your channels. \
F1 opens the help.";

struct App {
    cfg: Config,
    /// No settings file yet: say hello and explain.
    welcome: bool,
    /// The job running now, by its stop flag.
    job: Option<Arc<AtomicBool>>,
    /// What the job running now is, unless it is updating yt-dlp.
    current: Option<Queued>,
    /// What waits its turn, next first.
    queue: VecDeque<Queued>,
    /// The job running now came from the queue: its steps are shown, not
    /// said, as the user is busy with something else by then.
    quiet: bool,
    /// What went wrong and what was reported, kept to show together when the
    /// queue is done and no other window is in front.
    problems: Vec<String>,
    reports: Vec<String>,
    /// A download has ended and the queue is done: focus goes to the list when
    /// no other window is in front.
    ending: bool,
    /// The queue's window, while it is open.
    queue_window: Option<HWND>,
    /// What was left in the queue when VidFetch last closed, to offer again.
    resume: Vec<Queued>,
    /// The playlist folder whose videos the list shows, instead of all
    /// downloads (milestone 2 test, section 5).
    inside: Option<PathBuf>,
    /// The downloads in the list, most recent first, or a playlist's videos
    /// in its order.
    entries: Vec<library::Entry>,
    /// The file or folder to choose in the list next time it is filled.
    select: Option<PathBuf>,
    /// Lists from YouTube shown over the downloads, the last on top; Escape
    /// goes back one at a time.
    online: Vec<online::Online>,
    /// The YouTube channels followed.
    channels: Vec<channels::Channel>,
    /// What is being looked up online now: its number, to know its answer,
    /// and its stop flag.
    lookup: Option<(u64, Arc<AtomicBool>)>,
    next_lookup: u64,
    /// What was searched for last, to start the next search from.
    last_search: String,
    /// Downloads moved into Uncategorised as VidFetch started, to say so.
    tidied: usize,
}

pub fn run(cfg: Config, welcome: bool) {
    let app = Box::new(App {
        cfg,
        welcome,
        job: None,
        current: None,
        queue: VecDeque::new(),
        quiet: false,
        problems: Vec::new(),
        reports: Vec::new(),
        ending: false,
        queue_window: None,
        resume: queue::load(),
        inside: None,
        entries: Vec::new(),
        select: None,
        online: Vec::new(),
        channels: channels::load(),
        lookup: None,
        next_lookup: 0,
        last_search: String::new(),
        tidied: 0,
    });
    let mut app = app;
    app.tidied = category::tidy(&mut app);
    let hwnd = unsafe {
        let Ok(instance) = GetModuleHandleW(None) else {
            return;
        };
        match CreateDialogParamW(
            Some(instance.into()),
            PCWSTR(IDD_MAIN as usize as *const u16),
            None,
            Some(dialog_proc),
            LPARAM(Box::into_raw(app) as isize),
        ) {
            Ok(h) => h,
            Err(_) => return,
        }
    };
    unsafe {
        let _ = ShowWindow(hwnd, SW_SHOW);
        let _ = SetForegroundWindow(hwnd);
    }
    let mut msg = MSG::default();
    loop {
        // Zero is WM_QUIT and -1 an error; as_bool would spin forever on -1.
        let r = unsafe { GetMessageW(&mut msg, None, 0, 0) };
        if r.0 <= 0 {
            break;
        }
        // Alt+T on the Status box already: say it again, every time. Left to
        // the dialog, the third press said nothing (milestone 2 test, 5.3).
        if msg.message == WM_SYSKEYDOWN && msg.wParam.0 as u8 == b'T' {
            let status = unsafe { GetDlgItem(Some(hwnd), IDC_STATUS) };
            if status.is_ok_and(|s| unsafe { GetFocus() } == s) {
                crate::announce::say(hwnd, &get_text(hwnd, IDC_STATUS));
                continue;
            }
        }
        // The menus' shortcuts, for this window only: the other windows are
        // modal and run their own loops.
        if msg.message == WM_KEYDOWN {
            if let Some(id) = shortcut(msg.wParam.0 as u16) {
                unsafe {
                    SendMessageW(hwnd, WM_COMMAND, Some(WPARAM(id as usize)), None);
                }
                continue;
            }
        }
        // The whole keyboard behaviour of the window: Tab, Enter for the
        // default button, Escape, mnemonics.
        if unsafe { IsDialogMessageW(hwnd, &msg) }.as_bool() {
            continue;
        }
        unsafe {
            let _ = TranslateMessage(&msg);
            DispatchMessageW(&msg);
        }
    }
}

unsafe fn app<'a>(hwnd: HWND) -> Option<&'a mut App> {
    (GetWindowLongPtrW(hwnd, GWLP_USERDATA) as *mut App).as_mut()
}

unsafe extern "system" fn dialog_proc(
    hwnd: HWND,
    message: u32,
    wparam: WPARAM,
    lparam: LPARAM,
) -> isize {
    match message {
        WM_INITDIALOG => {
            SetWindowLongPtrW(hwnd, GWLP_USERDATA, lparam.0);
            if let Some(app) = app(hwnd) {
                category::fill(hwnd, app);
                name_list(hwnd, app);
                refresh(hwnd, app);
                let mut ready = match app.entries.len() {
                    0 => "Ready. Nothing downloaded yet: press Ctrl+N to download a video or a \
                          playlist."
                        .to_string(),
                    1 => "Ready. 1 download. Enter plays it; Ctrl+N downloads another.".to_string(),
                    n => format!(
                        "Ready. {n} downloads. Enter plays the chosen one, or shows a playlist's \
                         videos; Ctrl+N downloads another."
                    ),
                };
                if app.cfg.category.is_some() {
                    ready = format!("Showing {}. {ready}", category::shown(app));
                }
                if app.tidied > 0 {
                    ready = format!(
                        "{ready} Your {} were moved into the category Uncategorised, as the \
                         output folder now holds categories.",
                        count(app.tidied)
                    );
                }
                set_text(hwnd, IDC_STATUS, &ready);
                focus(hwnd, IDC_LIST);
            }
            if app(hwnd).is_some_and(|a| a.welcome) {
                let _ = PostMessageW(Some(hwnd), WM_WELCOME, WPARAM(0), LPARAM(0));
            }
            if app(hwnd).is_some_and(|a| !a.resume.is_empty()) {
                let _ = PostMessageW(Some(hwnd), WM_RESUME, WPARAM(0), LPARAM(0));
            }
            0 // zero: we set the focus ourselves
        }
        WM_RESUME => {
            if let Some(app) = app(hwnd) {
                resume(hwnd, app);
            }
            1
        }
        online::WM_FOUND => {
            if let Some(app) = app(hwnd) {
                online::found(hwnd, app, lparam);
            }
            1
        }
        queue::WM_TITLE => {
            let (id, title) = queue::take_title(lparam);
            if let Some(app) = app(hwnd) {
                for q in app.current.iter_mut().chain(app.queue.iter_mut()) {
                    if q.id == id {
                        q.title = Some(title.clone());
                    }
                }
                refill_queue(app);
                save_queue(app);
            }
            1
        }
        WM_WELCOME => {
            message::welcome(hwnd, WELCOME);
            if let Some(app) = app(hwnd) {
                // Written now, so the welcome comes only once.
                app.cfg.save();
            }
            focus(hwnd, IDC_LIST);
            1
        }
        WM_COMMAND => {
            let id = (wparam.0 & 0xFFFF) as i32;
            let code = ((wparam.0 >> 16) & 0xFFFF) as u32;
            let Some(app) = app(hwnd) else { return 0 };
            match (id, code) {
                (IDC_STATUS, EN_SETFOCUS) => caret_to_start(hwnd, IDC_STATUS),
                (IDC_CATEGORY, CBN_SELCHANGE) => category::chosen(hwnd, app),
                (IDC_CATEGORY, _) => return 0,
                (IDC_LIST, LBN_DBLCLK) => {
                    command(hwnd, app, IDM_OPEN);
                }
                (IDC_LIST, LBN_SELCHANGE) => {
                    name_enter(hwnd, app);
                    return 0;
                }
                (IDC_LIST, _) => return 0,
                // The buttons do what their menu items do. Enter is Play
                // anywhere it is the default.
                (IDC_OPEN, _) => {
                    command(hwnd, app, IDM_OPEN);
                }
                (id, _) if id == IDOK.0 => {
                    command(hwnd, app, IDM_OPEN);
                }
                (IDC_ADD, _) => {
                    command(hwnd, app, IDM_ADD);
                }
                (IDC_UPDATE, _) => {
                    command(hwnd, app, IDM_UPDATE);
                }
                (IDC_QUEUE, _) => {
                    command(hwnd, app, IDM_QUEUE);
                }
                (IDC_RENAME, _) => {
                    command(hwnd, app, IDM_RENAME);
                }
                (IDC_SHOW, _) => {
                    command(hwnd, app, IDM_SHOW);
                }
                (IDC_DELETE, _) => {
                    command(hwnd, app, IDM_DELETE);
                }
                (id, _) if id == IDCANCEL.0 => escape(hwnd),
                // Menus and shortcuts: code 0 or 1, no control.
                (id, 0 | 1) if lparam.0 == 0 => {
                    if !command(hwnd, app, id) {
                        return 0;
                    }
                }
                _ => return 0,
            }
            1
        }
        // Delete in the list (LBS_WANTKEYBOARDINPUT), and Backspace out of a
        // playlist's videos; F2 is a shortcut everywhere in the window.
        WM_VKEYTOITEM => {
            let key = (wparam.0 & 0xFFFF) as u16;
            let Some(app) = app(hwnd) else { return -1 };
            if key == VK_DELETE.0 {
                command(hwnd, app, IDM_DELETE);
            } else if key == VK_BACK.0 && (app.inside.is_some() || !app.online.is_empty()) {
                if !online::back(hwnd, app) {
                    back(hwnd, app);
                }
            } else {
                return -1;
            }
            -2
        }
        // Shift+F10, the Applications key or a right click on the list.
        WM_CONTEXTMENU => {
            let lb = list(hwnd);
            if wparam.0 != lb.0 as usize {
                return 0;
            }
            if let Some(app) = app(hwnd) {
                context_menu(hwnd, app, lparam);
            }
            1
        }
        job::WM_JOB => {
            let event = job::take(lparam);
            if let Some(app) = app(hwnd) {
                job_event(hwnd, app, *event);
            }
            1
        }
        // A window in front has closed. Posted, as that window is still
        // closing at this point.
        WM_ENABLE => {
            if wparam.0 != 0 {
                let _ = PostMessageW(Some(hwnd), WM_SETTLE, WPARAM(0), LPARAM(0));
            }
            0
        }
        WM_SETTLE => {
            if let Some(app) = app(hwnd) {
                settle(hwnd, app);
            }
            1
        }
        WM_INITMENUPOPUP => {
            if let Some(app) = app(hwnd) {
                init_menu(hwnd, app, HMENU(wparam.0 as *mut _));
            }
            0
        }
        WM_HELP => {
            help(hwnd);
            1
        }
        WM_CLOSE => {
            // A multi-line edit turns Escape into WM_CLOSE on its dialog; only
            // Alt+F4 and the close button should end the app.
            if GetKeyState(VK_ESCAPE.0 as i32) < 0 {
                escape(hwnd);
                return 1;
            }
            if let Some(app) = app(hwnd) {
                let running = app.job.is_some() && app.current.is_some();
                let what = match (running, app.queue.len()) {
                    (false, 0) => None,
                    (true, 0) => Some("A download is running.".to_string()),
                    (true, n) => Some(format!(
                        "A download is running, and {} more waiting in the queue.",
                        count(n)
                    )),
                    (false, n) => Some(format!("{} waiting in the queue.", count(n))),
                };
                if let Some(what) = what {
                    if !message::ask(
                        hwnd,
                        &format!(
                            "{what} Exit now? What has not finished is kept, and VidFetch offers \
                             to carry on with it when it starts again."
                        ),
                        "E&xit",
                        "&Keep downloading",
                        false,
                    ) {
                        return 1;
                    }
                }
                save_queue(app);
                app.cfg.save();
                // yt-dlp ends with VidFetch in any case (ytdlp.rs, Job).
                if let Some(stop) = &app.job {
                    stop.store(true, Ordering::Relaxed);
                }
            }
            let _ = DestroyWindow(hwnd);
            1
        }
        WM_DESTROY => {
            PostQuitMessage(0);
            0
        }
        WM_NCDESTROY => {
            let ptr = SetWindowLongPtrW(hwnd, GWLP_USERDATA, 0);
            if ptr != 0 {
                drop(Box::from_raw(ptr as *mut App));
            }
            0
        }
        _ => 0,
    }
}

fn list(hwnd: HWND) -> HWND {
    unsafe { GetDlgItem(Some(hwnd), IDC_LIST).unwrap_or_default() }
}

/// Fills the list again from the output folder, or from the playlist's folder
/// it shows, keeping the one chosen, or choosing the one a job just saved.
fn refresh(hwnd: HWND, app: &mut App) {
    // A list from YouTube is shown: the downloads are filled in on the way
    // back to them.
    if !app.online.is_empty() {
        return;
    }
    let lb = list(hwnd);
    let at = downloads::chosen(lb);
    let path = app
        .select
        .take()
        .or_else(|| at.and_then(|i| app.entries.get(i)).map(|e| e.path.clone()));
    // A playlist's folder gone from under it: back to all downloads.
    if app.inside.as_deref().is_some_and(|f| !f.is_dir()) {
        app.inside = None;
        name_list(hwnd, app);
    }
    app.entries = match &app.inside {
        Some(folder) => library::videos_in(folder),
        None => library::list_all(&app.cfg.output_folder(), app.cfg.category.as_deref()),
    };
    let all = app.cfg.category.is_none() && app.inside.is_none();
    downloads::fill(lb, &app.entries, path.as_deref(), at.unwrap_or(0), all);
    name_enter(hwnd, app);
}

/// The list's label says which list it is: a screen reader reads it as the
/// list's name.
fn name_list(hwnd: HWND, app: &App) {
    let label = match (&app.inside, app.online.last()) {
        (_, Some(o)) => online::label(o),
        (Some(folder), None) => format!(
            "Do&wnloads in the playlist {}, in its order:",
            folder.file_name().unwrap_or_default().to_string_lossy()
        ),
        (None, None) => match &app.cfg.category {
            Some(c) => format!("Do&wnloads in {c}, most recent first:"),
            None => "Do&wnloads, most recent first:".to_string(),
        },
    };
    set_text(hwnd, IDC_LIST_LABEL, &label);
}

/// The Enter button says what Enter does to the one chosen.
fn name_enter(hwnd: HWND, app: &App) {
    if !app.online.is_empty() {
        let text = online::enter_words(app, downloads::chosen(list(hwnd)));
        if get_text(hwnd, IDC_OPEN) != text {
            set_text(hwnd, IDC_OPEN, text);
        }
        return;
    }
    let playlist = downloads::chosen(list(hwnd))
        .and_then(|i| app.entries.get(i))
        .is_some_and(|e| e.is_playlist());
    let text = if playlist {
        "Show the &playlist's videos  Enter"
    } else {
        "&Play  Enter"
    };
    if get_text(hwnd, IDC_OPEN) != text {
        set_text(hwnd, IDC_OPEN, text);
    }
}

/// The download chosen in the list; when there is none, that is said.
fn chosen(hwnd: HWND, app: &App) -> Option<library::Entry> {
    let e = downloads::chosen(list(hwnd))
        .and_then(|i| app.entries.get(i))
        .cloned();
    if e.is_none() {
        say(
            hwnd,
            if app.inside.is_some() {
                "There are no videos in this playlist's folder. Escape goes back to all downloads."
            } else {
                "There is nothing in the list of downloads. Press Ctrl+N to download a video or a \
                 playlist."
            },
        );
    }
    e
}

/// Shows the videos of the playlist `e` in the list, in its order (milestone
/// 2 test, section 5). Escape or Backspace goes back.
fn open_playlist(hwnd: HWND, app: &mut App, e: &library::Entry) {
    app.inside = Some(e.path.clone());
    app.entries = library::videos_in(&e.path);
    name_list(hwnd, app);
    downloads::fill(list(hwnd), &app.entries, None, 0, false);
    name_enter(hwnd, app);
    let n = app.entries.len();
    let videos = if n == 1 {
        "1 video".to_string()
    } else {
        format!("{n} videos")
    };
    say(
        hwnd,
        &format!(
            "The playlist {}: {videos}. Enter plays the chosen one; Escape goes back to all \
             downloads.",
            e.name
        ),
    );
    focus(hwnd, IDC_LIST);
}

/// Back from a playlist's videos to all downloads, on that playlist. False if
/// the list was showing all downloads already.
fn back(hwnd: HWND, app: &mut App) -> bool {
    let Some(folder) = app.inside.take() else {
        return false;
    };
    name_list(hwnd, app);
    app.select = Some(folder.clone());
    refresh(hwnd, app);
    say(
        hwnd,
        &format!(
            "All downloads, on the playlist {}.",
            folder.file_name().unwrap_or_default().to_string_lossy()
        ),
    );
    focus(hwnd, IDC_LIST);
    true
}

/// Escape: stops a lookup online, else goes back from a list from YouTube or
/// a playlist's videos, else stops the download running.
fn escape(hwnd: HWND) {
    let Some(app) = (unsafe { app(hwnd) }) else {
        return;
    };
    if !online::stop_looking(hwnd, app) && !online::back(hwnd, app) && !back(hwnd, app) {
        stop(hwnd);
    }
}

/// Keeps the speed the Play window was left at.
fn keep_speed(app: &mut App, speed: f64) {
    if (app.cfg.speed - speed).abs() > 0.001 {
        app.cfg.speed = speed;
        app.cfg.save();
    }
}

/// The captions saved beside a download, as text to read (log entry 3,
/// idea 8).
fn read_captions(hwnd: HWND, e: &library::Entry) {
    if e.is_playlist() {
        say(
            hwnd,
            "Captions belong to one video: Enter shows the playlist's videos, then choose one.",
        );
        return;
    }
    let Some(file) = library::captions_of(&e.path).into_iter().next() else {
        say(
            hwnd,
            &format!(
                "{} has no captions saved beside it. Save captions as a file, in the Options \
                 menu, keeps them with the next downloads.",
                e.name
            ),
        );
        return;
    };
    let text = std::fs::read_to_string(&file).unwrap_or_default();
    let cues = crate::captions::parse(&text);
    if cues.is_empty() {
        say(
            hwnd,
            &format!("The captions of {} could not be read.", e.name),
        );
        return;
    }
    message::captions(
        hwnd,
        &format!("{}\n\n{}", e.name, crate::captions::as_text(&cues)),
    );
    focus(hwnd, IDC_LIST);
}

/// Plays the download at `at` in the list in the Play window, then the next
/// or previous one if asked, skipping playlists.
fn play(hwnd: HWND, mut at: usize) {
    loop {
        let Some(app) = (unsafe { app(hwnd) }) else {
            return;
        };
        let Some(e) = app.entries.get(at).cloned() else {
            return;
        };
        let file = |e: &library::Entry| !e.is_playlist();
        let next = (at + 1..app.entries.len()).find(|&i| file(&app.entries[i]));
        let previous = (0..at).rev().find(|&i| file(&app.entries[i]));
        let source = e.path.display().to_string();
        crate::history::record(&crate::places::of_file(&e.path), &e.name);
        let mut l = listen::Listen {
            source: &source,
            name: &e.name,
            key: crate::places::of_file(&e.path),
            speed: app.cfg.speed,
            has_next: next.is_some(),
            has_previous: previous.is_some(),
            play_next: app.cfg.play_next,
            download: None,
            video: library::video_of(Config::past_path().as_deref(), &e.path),
        };
        let outcome = listen::show(hwnd, &mut l);
        let speed = l.speed;
        let Some(app) = (unsafe { self::app(hwnd) }) else {
            return;
        };
        keep_speed(app, speed);
        let to = match outcome {
            Err(why) => {
                message::problem(hwnd, &why);
                None
            }
            Ok(listen::Outcome::Next) => next,
            Ok(listen::Outcome::Previous) => previous,
            Ok(listen::Outcome::Closed) => None,
        };
        let Some(to) = to else {
            focus(hwnd, IDC_LIST);
            return;
        };
        // The list follows, so Escape lands on the one last played.
        at = to;
        unsafe {
            SendMessageW(list(hwnd), LB_SETCURSEL, Some(WPARAM(at)), None);
        }
        name_enter(hwnd, app);
    }
}

/// The running job is filling this playlist's folder, so it and its videos
/// must stay as they are.
fn busy_with(hwnd: HWND, app: &App, e: &library::Entry) -> bool {
    let folder = if e.is_playlist() {
        e.path.as_path()
    } else {
        match app.inside.as_deref() {
            Some(f) => f,
            None => return false,
        }
    };
    let here = |q: &Queued| q.into.as_deref() == Some(folder);
    if app.current.as_ref().is_some_and(here) {
        say(
            hwnd,
            "VidFetch is downloading into this playlist's folder now. Wait for it to finish, or \
             press Escape to stop it first.",
        );
        return true;
    }
    if app.queue.iter().any(here) {
        say(
            hwnd,
            "This playlist is waiting in the queue to be updated. Remove it from the queue first, \
             with Ctrl+Q.",
        );
        return true;
    }
    false
}

/// "1 download" or "3 downloads".
fn count(n: usize) -> String {
    if n == 1 {
        "1 download".to_string()
    } else {
        format!("{n} downloads")
    }
}

/// What is said about the queue after something else: nothing if it is empty.
fn more_waiting(app: &App) -> String {
    match app.queue.len() {
        0 => String::new(),
        n => format!(" {} more in the queue.", count(n)),
    }
}

/// No other window is in front of the main window. A modal one disables it.
fn in_front(hwnd: HWND) -> bool {
    unsafe { IsWindowEnabled(hwnd) }.as_bool()
}

/// Keeps the download running and those waiting for next time.
fn save_queue(app: &App) {
    queue::save(app.current.iter().chain(app.queue.iter()));
}

/// Offers what was left in the queue when VidFetch last closed: carry on, or
/// empty the queue (log entry 2, Q3).
fn resume(hwnd: HWND, app: &mut App) {
    let kept = std::mem::take(&mut app.resume);
    if kept.is_empty() {
        return;
    }
    let names: Vec<String> = kept.iter().map(|q| q.label()).collect();
    let empty = message::ask(
        hwnd,
        &format!(
            "When VidFetch last closed, {} had not finished:\n\n{}\n\nCarry on downloading now, \
             or empty the queue?",
            count(kept.len()),
            names.join("\n")
        ),
        "&Empty the queue",
        "&Carry on downloading",
        false,
    );
    if empty {
        queue::save(std::iter::empty());
        say(hwnd, "The queue is empty. Nothing more will be downloaded.");
        focus(hwnd, IDC_LIST);
        return;
    }
    for q in &kept {
        queue::fetch_title(hwnd, q);
    }
    app.queue = kept.into();
    if let Some(q) = app.queue.pop_front() {
        begin(hwnd, app, q, false);
    }
}

/// The context menu of the list, for the download chosen, or the one under
/// the mouse.
fn context_menu(hwnd: HWND, app: &mut App, at: LPARAM) {
    let lb = list(hwnd);
    let (x, y) = (
        (at.0 & 0xFFFF) as i16 as i32,
        ((at.0 >> 16) & 0xFFFF) as i16 as i32,
    );
    let mouse = (x, y) != (-1, -1);
    if mouse {
        // A right click chooses the download it is on, as in File Explorer.
        let mut p = windows::Win32::Foundation::POINT { x, y };
        unsafe {
            let _ = windows::Win32::Graphics::Gdi::ScreenToClient(lb, &mut p);
            let r = SendMessageW(
                lb,
                LB_ITEMFROMPOINT,
                None,
                Some(LPARAM(
                    ((p.y as isize & 0xFFFF) << 16) | (p.x as isize & 0xFFFF),
                )),
            )
            .0;
            if (r >> 16) & 0xFFFF == 0 {
                SendMessageW(lb, LB_SETCURSEL, Some(WPARAM((r & 0xFFFF) as usize)), None);
            }
        }
    }
    if !app.online.is_empty() {
        online::context_menu(hwnd, app, mouse.then_some((x, y)));
        return;
    }
    let Some(e) = chosen(hwnd, app) else {
        return;
    };
    let mut items = if e.is_playlist() {
        vec![
            (IDM_OPEN, "Show its &videos\tEnter"),
            (IDM_UPDATE, "&Update the playlist\tCtrl+U"),
            (IDM_EXTERNAL, "&Open the folder in File Explorer"),
        ]
    } else {
        vec![
            (IDM_OPEN, "&Play\tEnter"),
            (IDM_EXTERNAL, "&Open in the default player"),
            (IDM_DESCRIBE, "Read its descr&iption\tCtrl+I"),
            (IDM_CAPTIONS_TEXT, "Read its &captions as text"),
            (IDM_FOLLOW, "&Follow its channel"),
        ]
    };
    if app.inside.is_some() {
        items.push((IDM_BACK, "&Back to all downloads\tEscape"));
    } else {
        items.push((IDM_MOVE, "Mo&ve to a category...\tCtrl+Shift+M"));
    }
    items.extend([
        (IDM_COPY, "Cop&y its web address"),
        (0, ""),
        (IDM_RENAME, "&Rename\tF2"),
        (IDM_SHOW, "Show in File E&xplorer"),
        (IDM_DELETE, "&Delete\tDelete"),
    ]);
    if let Some(id) = downloads::context_menu(hwnd, lb, mouse.then_some((x, y)), &items) {
        command(hwnd, app, id);
    }
}

/// New download: the New download window, starting from `text`, then
/// downloading.
fn add(hwnd: HWND, app: &mut App, text: &str) {
    // A copy: a download ending behind this window also uses `app`.
    let busy = app.job.is_some();
    let mut cfg = app.cfg.clone();
    let categories = library::categories(&app.cfg.output_folder());
    let chosen = crate::addvideo::show(hwnd, &mut cfg, text, busy, &categories);
    let Some(app) = (unsafe { self::app(hwnd) }) else {
        return;
    };
    if cfg != app.cfg {
        app.cfg = cfg;
        app.cfg.save();
    }
    match chosen {
        Some(crate::addvideo::Chosen::Follow(url)) => online::follow_address(hwnd, app, url),
        Some(crate::addvideo::Chosen::Download { url, category }) => {
            let whole = ytdlp::is_youtube(&url)
                && ytdlp::video_id(&url).is_some()
                && ytdlp::list_id(&url).is_some_and(|l| !ytdlp::is_mix(&l))
                && message::ask(
                    hwnd,
                    "This video is part of a playlist. Download the whole playlist, into a folder \
                     of its own, or just this video?",
                    "The &whole playlist",
                    "Just this &video",
                    false,
                );
            if !whole && !download_again(hwnd, app, &url) {
                say(
                    hwnd,
                    if busy {
                        "Nothing added to the queue."
                    } else {
                        "Nothing downloaded."
                    },
                );
                if !busy {
                    focus(hwnd, IDC_LIST);
                }
                return;
            }
            let folder = app.cfg.output_folder().join(category);
            submit(hwnd, app, url, whole, None, folder);
        }
        None => {
            say(
                hwnd,
                if busy {
                    "Nothing added to the queue."
                } else {
                    "Nothing downloaded."
                },
            );
            if !busy {
                focus(hwnd, IDC_LIST);
            }
        }
    }
}

/// This address is already saved at this format and quality, so a second copy
/// is asked about first, with Cancel the default (milestone 2 test, 3.6). True
/// to download it again; otherwise the copy there is chosen in the list. A
/// different format or quality just downloads (log entry 1, Q2).
fn download_again(hwnd: HWND, app: &mut App, url: &str) -> bool {
    let (Some(record), Some(key)) = (Config::past_path(), ytdlp::key(url)) else {
        return true;
    };
    let lines = job::past_lines(app.cfg.format, app.cfg.lines);
    let Some(file) = library::already(&record, &key, app.cfg.format.word(), lines) else {
        return true;
    };
    let name = file
        .file_name()
        .unwrap_or_default()
        .to_string_lossy()
        .into_owned();
    let again = message::ask(
        hwnd,
        &format!("Already downloaded as {name}, at this quality. Download it again?"),
        "&Download again",
        "&Cancel",
        false,
    );
    if !again && app.inside.is_none() {
        app.select = Some(file);
        refresh(hwnd, app);
    }
    again
}

/// Downloads `url`, now or when its turn comes, as the options stand, into
/// `folder`, a category's; `whole` takes the whole playlist a video is in,
/// and `into` is the playlist's folder when it is being updated.
fn submit(
    hwnd: HWND,
    app: &mut App,
    url: String,
    whole: bool,
    into: Option<PathBuf>,
    folder: PathBuf,
) {
    let settings = job::Settings {
        url,
        output_folder: folder,
        format: app.cfg.format,
        lines: app.cfg.lines,
        number: app.cfg.number,
        captions: app.cfg.captions,
        whole_list: whole,
        fetch_ytdlp: false,
    };
    let q = Queued::new(settings, into);
    queue::fetch_title(hwnd, &q);
    if app.job.is_none() {
        begin(hwnd, app, q, false);
        return;
    }
    if app.current.as_ref().is_some_and(|c| c.same_as(&q)) {
        say(
            hwnd,
            "That is the download running now, so it was not added again.",
        );
    } else if app.queue.iter().any(|w| w.same_as(&q)) {
        say(
            hwnd,
            "That is already in the queue, so it was not added again.",
        );
    } else {
        app.queue.push_back(q);
        let place = app.queue.len();
        refill_queue(app);
        save_queue(app);
        say(
            hwnd,
            &format!(
                "Added to the queue, number {place}. It starts when its turn comes; Ctrl+Q shows \
                 the queue."
            ),
        );
    }
}

/// Starts a download. `quiet`: it came from the queue, so nothing is said and
/// focus stays where it is. False if it could not start.
fn begin(hwnd: HWND, app: &mut App, mut q: Queued, quiet: bool) -> bool {
    if ytdlp::find().is_none() {
        q.settings.fetch_ytdlp = message::ask(
            hwnd,
            "VidFetch downloads videos with yt-dlp, a free program that is not on this computer \
             yet. Download it now from its official page on GitHub? It is about 18 MB, is checked \
             against its published checksum, and is kept in VidFetch's own folder.",
            "&Download",
            "&Not now",
            true,
        );
        if !q.settings.fetch_ytdlp {
            // Everything waiting needs it too.
            app.queue.clear();
            refill_queue(app);
            save_queue(app);
            say(hwnd, "Not started: downloading needs yt-dlp.");
            focus(hwnd, IDC_LIST);
            return false;
        }
    }
    // The job takes its own copy; the queue's window and the checks above
    // want this one kept.
    app.job = Some(job::start(hwnd, q.settings()));
    app.quiet = quiet;
    app.ending = false;
    app.current = Some(q);
    refill_queue(app);
    save_queue(app);
    if quiet {
        set_text(hwnd, IDC_STATUS, "Starting the next download in the queue.");
    } else if !app.online.is_empty() {
        // Escape goes back from the list shown; focus stays on it.
        say(
            hwnd,
            "Started downloading. To stop it, go back to your downloads and press Escape.",
        );
    } else {
        say(hwnd, "Started. Press Escape to stop.");
        if in_front(hwnd) {
            focus(hwnd, IDC_STATUS);
        }
    }
    true
}

/// A job has ended, or a window in front has closed: start what is waiting,
/// or, once all is done and nothing is in front, show what was kept to show.
fn settle(hwnd: HWND, app: &mut App) {
    if app.job.is_some() {
        return;
    }
    if !app.queue.is_empty() {
        // Asking for yt-dlp needs the window; everything else carries on
        // behind whatever is in front.
        if ytdlp::find().is_none() && !in_front(hwnd) {
            return;
        }
        if let Some(q) = app.queue.pop_front() {
            begin(hwnd, app, q, true);
        }
        return;
    }
    if !in_front(hwnd) {
        return;
    }
    let problems = std::mem::take(&mut app.problems);
    if !problems.is_empty() {
        message::problem(hwnd, &problems.join("\n\n"));
    }
    let reports = std::mem::take(&mut app.reports);
    if !reports.is_empty() {
        message::report(hwnd, &reports.join("\n\n"));
    }
    if std::mem::take(&mut app.ending) {
        focus(hwnd, IDC_LIST);
    }
}

/// The job running now has ended; `focus_after`: focus goes to the list once
/// the queue is done too.
fn ended(hwnd: HWND, app: &mut App, focus_after: bool) {
    app.job = None;
    app.current = None;
    app.quiet = false;
    app.ending = focus_after;
    refill_queue(app);
    save_queue(app);
    settle(hwnd, app);
}

fn stop(hwnd: HWND) {
    let Some(app) = (unsafe { app(hwnd) }) else {
        return;
    };
    match &app.job {
        Some(flag) => {
            flag.store(true, Ordering::Relaxed);
            let said = match app.queue.len() {
                0 => "Stopping.".to_string(),
                n => format!(
                    "Stopping this download. {} in the queue will carry on; Ctrl+Q shows the \
                     queue.",
                    count(n)
                ),
            };
            say(hwnd, &said);
        }
        None => say(hwnd, "Nothing is downloading, so there is nothing to stop."),
    }
}

/// News from the job.
fn job_event(hwnd: HWND, app: &mut App, event: Event) {
    match event {
        Event::Step(text) if app.quiet => set_text(hwnd, IDC_STATUS, &text),
        Event::Step(text) => say(hwnd, &text),
        // The queue goes after what is happening, so that comes first in the box.
        Event::Progress(text) => {
            set_text(hwnd, IDC_STATUS, &format!("{text}{}", more_waiting(app)))
        }
        Event::Done {
            mut said,
            path,
            report,
        } => {
            // Enter plays what the list shows, which is not the download
            // while a list from YouTube is shown.
            if !app.online.is_empty() {
                said = said.replace(
                    " Press Enter to play it.",
                    " It is in your downloads, where Escape goes back.",
                );
            }
            app.select = Some(path);
            refresh(hwnd, app);
            say(hwnd, &format!("{said}{}", more_waiting(app)));
            app.reports.extend(report);
            ended(hwnd, app, true);
        }
        Event::Failed(why) => {
            refresh(hwnd, app);
            app.problems.push(why);
            if app.queue.is_empty() && in_front(hwnd) {
                // The problem's own window says it at once.
                set_text(hwnd, IDC_STATUS, "Stopped by a problem.");
            } else {
                say(
                    hwnd,
                    &format!(
                        "A download stopped with a problem; VidFetch says what it was once the \
                         queue is done.{}",
                        more_waiting(app)
                    ),
                );
            }
            ended(hwnd, app, true);
        }
        Event::Said(text) => {
            say(hwnd, &format!("{text}{}", more_waiting(app)));
            ended(hwnd, app, false);
        }
        Event::Stopped(text) => {
            refresh(hwnd, app);
            say(hwnd, &format!("{text}{}", more_waiting(app)));
            ended(hwnd, app, true);
        }
    }
}

/// The Queue window (Ctrl+Q): what is running, and what waits, which can be
/// taken out. It reads the queue from the main window as it stands, as
/// downloads keep ending and starting while it is open.
fn show_queue(hwnd: HWND) {
    unsafe {
        let Ok(instance) = GetModuleHandleW(None) else {
            return;
        };
        DialogBoxParamW(
            Some(instance.into()),
            PCWSTR(IDD_QUEUE as usize as *const u16),
            Some(hwnd),
            Some(queue_proc),
            LPARAM(hwnd.0 as isize),
        );
    }
    focus(hwnd, IDC_LIST);
}

/// Shows the queue as it stands in its window, if that is open, keeping the
/// place in the list. Only what differs is touched: a screen reader reads a
/// rebuilt list out again.
fn refill_queue(app: &App) {
    let Some(w) = app.queue_window else { return };
    let now = app.current.as_ref().map_or_else(
        || {
            if app.job.is_some() {
                "Updating yt-dlp.".to_string()
            } else {
                "Nothing.".to_string()
            }
        },
        Queued::label,
    );
    if get_text(w, IDC_Q_NOW) != now {
        set_text(w, IDC_Q_NOW, &now);
    }
    unsafe {
        let Ok(lb) = GetDlgItem(Some(w), IDC_Q_LIST) else {
            return;
        };
        let shown = SendMessageW(lb, LB_GETCOUNT, None, None).0.max(0) as usize;
        let item = |i: usize| {
            let n = SendMessageW(lb, LB_GETTEXTLEN, Some(WPARAM(i)), None)
                .0
                .max(0) as usize;
            let mut buf = vec![0u16; n + 1];
            SendMessageW(
                lb,
                LB_GETTEXT,
                Some(WPARAM(i)),
                Some(LPARAM(buf.as_mut_ptr() as isize)),
            );
            String::from_utf16_lossy(&buf[..n])
        };
        let labels: Vec<String> = app.queue.iter().map(Queued::label).collect();
        let at = downloads::chosen(lb).unwrap_or(0);
        if shown == labels.len() {
            // A title come in: that item alone changes.
            for (i, l) in labels.iter().enumerate().filter(|(i, l)| item(*i) != **l) {
                let t = wide(l);
                SendMessageW(lb, LB_DELETESTRING, Some(WPARAM(i)), None);
                SendMessageW(
                    lb,
                    LB_INSERTSTRING,
                    Some(WPARAM(i)),
                    Some(LPARAM(t.as_ptr() as isize)),
                );
                if i == at {
                    SendMessageW(lb, LB_SETCURSEL, Some(WPARAM(i)), None);
                }
            }
            return;
        }
        SendMessageW(lb, LB_RESETCONTENT, None, None);
        for l in &labels {
            let t = wide(l);
            SendMessageW(lb, LB_ADDSTRING, None, Some(LPARAM(t.as_ptr() as isize)));
        }
        if !app.queue.is_empty() {
            let at = at.min(app.queue.len() - 1);
            SendMessageW(lb, LB_SETCURSEL, Some(WPARAM(at)), None);
        }
    }
}

/// Takes the chosen item out of the queue; says what happened, from the window
/// in front.
fn remove_queued(w: HWND, app: &mut App) {
    let chosen = unsafe { GetDlgItem(Some(w), IDC_Q_LIST) }
        .ok()
        .and_then(downloads::chosen);
    let said = match chosen.and_then(|i| app.queue.remove(i)) {
        Some(q) => format!(
            "Removed: {}. {} waiting.",
            q.label(),
            count(app.queue.len())
        ),
        None => "Nothing is waiting, so there is nothing to remove.".to_string(),
    };
    refill_queue(app);
    save_queue(app);
    crate::announce::say(w, &said);
}

unsafe extern "system" fn queue_proc(
    w: HWND,
    message: u32,
    wparam: WPARAM,
    lparam: LPARAM,
) -> isize {
    if message == WM_INITDIALOG {
        SetWindowLongPtrW(w, GWLP_USERDATA, lparam.0);
    }
    let main = HWND(GetWindowLongPtrW(w, GWLP_USERDATA) as *mut _);
    let Some(app) = app(main) else { return 0 };
    match message {
        WM_INITDIALOG => {
            app.queue_window = Some(w);
            refill_queue(app);
            let said = match app.queue.len() {
                0 => "Nothing is waiting.".to_string(),
                n => format!("{} waiting.", count(n)),
            };
            focus(w, IDC_Q_LIST);
            crate::announce::say(w, &said);
            0
        }
        WM_VKEYTOITEM => {
            if (wparam.0 & 0xFFFF) as u16 != VK_DELETE.0 {
                return -1;
            }
            remove_queued(w, app);
            -2
        }
        WM_COMMAND => {
            let id = (wparam.0 & 0xFFFF) as i32;
            let code = ((wparam.0 >> 16) & 0xFFFF) as u32;
            match (id, code) {
                (IDC_Q_NOW, EN_SETFOCUS) => caret_to_start(w, IDC_Q_NOW),
                (IDC_Q_REMOVE, _) => remove_queued(w, app),
                (IDC_Q_ALL, _) => {
                    let said = empty_queue(main, w);
                    crate::announce::say(w, &said);
                }
                (id, _) if id == IDOK.0 || id == IDCANCEL.0 => {
                    app.queue_window = None;
                    let _ = EndDialog(w, id as isize);
                }
                _ => return 0,
            }
            1
        }
        _ => 0,
    }
}

/// Asks from `owner`, then empties the queue of the main window `main`; what
/// to say. The one running carries on.
fn empty_queue(main: HWND, owner: HWND) -> String {
    let n = unsafe { app(main) }.map_or(0, |a| a.queue.len());
    if n == 0 {
        return "Nothing is waiting in the queue, so there is nothing to remove.".into();
    }
    if !message::ask(
        owner,
        &format!(
            "Remove all {} waiting from the queue? The one running now carries on.",
            count(n)
        ),
        "&Remove them",
        "&Keep them",
        false,
    ) {
        return "The queue was kept as it was.".into();
    }
    // Taken again from the app: downloads moved on behind the question.
    if let Some(app) = unsafe { app(main) } {
        app.queue.clear();
        refill_queue(app);
        save_queue(app);
    }
    "The queue is empty.".into()
}

fn submenu(menu: HMENU, pos: i32) -> HMENU {
    unsafe { GetSubMenu(menu, pos) }
}

/// A menu is about to open: tick what is chosen, and fill the lists of
/// choices. Nothing is greyed out.
fn init_menu(hwnd: HWND, app: &mut App, menu: HMENU) {
    let bar = unsafe { GetMenu(hwnd) };
    let options = submenu(bar, OPTIONS_MENU);
    let cfg = &app.cfg;
    if menu == submenu(options, FORMAT_MENU) {
        let items: Vec<String> = FORMATS.iter().map(|f| f.1.to_string()).collect();
        let at = FORMATS.iter().position(|f| f.0 == cfg.format);
        fill_radio(menu, IDM_FORMAT, &items, at);
    } else if menu == submenu(options, QUALITY_MENU) {
        let items: Vec<String> = QUALITIES.iter().map(|q| q.1.to_string()).collect();
        let at = QUALITIES.iter().position(|q| q.0 == cfg.lines);
        fill_radio(menu, IDM_QUALITY, &items, at);
    } else if menu == submenu(options, SPEED_MENU) {
        let items: Vec<String> = SPEEDS.iter().map(|&x| speed_item(x)).collect();
        let at = SPEEDS.iter().position(|&x| (x - cfg.speed).abs() < 0.01);
        fill_radio(menu, IDM_SPEED, &items, at);
    }
    let tick = |id: i32, on: bool| unsafe {
        CheckMenuItem(
            menu,
            id as u32,
            (MF_BYCOMMAND | if on { MF_CHECKED } else { MF_UNCHECKED }).0,
        );
    };
    tick(IDM_NUMBER, cfg.number);
    tick(IDM_CAPTIONS, cfg.captions);
    tick(IDM_PLAY_NEXT, cfg.play_next);
}

/// A speed as the Options menu says it: "Normal", "1.25 times as fast".
fn speed_item(x: f64) -> String {
    let words = listen::speed_words(x);
    let mut c = words.chars();
    c.next()
        .map(|f| f.to_uppercase().collect::<String>() + c.as_str())
        .unwrap_or_default()
}

/// Fills `menu` with `items`, numbered from `first`, the chosen one marked
/// with a radio dot.
fn fill_radio(menu: HMENU, first: i32, items: &[String], chosen: Option<usize>) {
    unsafe {
        while GetMenuItemCount(Some(menu)) > 0 {
            if DeleteMenu(menu, 0, MF_BYPOSITION).is_err() {
                break;
            }
        }
        for (i, item) in items.iter().enumerate() {
            let t = wide(item);
            let _ = AppendMenuW(
                menu,
                MF_STRING,
                (first + i as i32) as usize,
                PCWSTR(t.as_ptr()),
            );
        }
        if let Some(at) = chosen {
            let last = first + items.len() as i32 - 1;
            let _ = CheckMenuRadioItem(
                menu,
                first as u32,
                last as u32,
                (first + at as i32) as u32,
                MF_BYCOMMAND.0,
            );
        }
    }
}

/// The shortcuts shown in the menus, in step with vidfetch.rc. Enter,
/// Escape and F1 are the dialog's own.
fn shortcut(key: u16) -> Option<i32> {
    let down = |k: VIRTUAL_KEY| unsafe { GetKeyState(k.0 as i32) } < 0;
    if key == VK_F2.0 && !down(VK_MENU) && !down(VK_CONTROL) && !down(VK_SHIFT) {
        return Some(IDM_RENAME);
    }
    if down(VK_MENU) || !down(VK_CONTROL) {
        return None;
    }
    if down(VK_SHIFT) {
        return match key as u8 {
            b'L' => Some(IDM_CHANNELS),
            b'M' => Some(IDM_MOVE),
            _ => None,
        };
    }
    match key as u8 {
        b'N' => Some(IDM_ADD),
        b'U' => Some(IDM_UPDATE),
        b'Q' => Some(IDM_QUEUE),
        b'F' => Some(IDM_SEARCH),
        b'L' => Some(IDM_NEW_VIDEOS),
        b'D' => Some(IDM_DOWNLOAD),
        b'I' => Some(IDM_DESCRIBE),
        b'H' => Some(IDM_HISTORY),
        _ => None,
    }
}

/// Commands from the menus and their shortcuts. Every choice is saved at once
/// and said out loud, as the menu closes without a word.
fn command(hwnd: HWND, app: &mut App, id: i32) -> bool {
    let in_list = |first: i32, len: usize| (first..first + len as i32).contains(&id);
    if !app.online.is_empty() {
        if let Some(done) = online::command(hwnd, app, id) {
            return done;
        }
    } else if online::download_command(hwnd, app, id) {
        return true;
    }
    let said = match id {
        IDM_MOVE => {
            category::move_chosen(hwnd, app);
            return true;
        }
        IDM_HISTORY => {
            online::recently_played(hwnd, app);
            return true;
        }
        IDM_CAPTIONS_TEXT => {
            if let Some(e) = chosen(hwnd, app) {
                read_captions(hwnd, &e);
            }
            return true;
        }
        IDM_NEW_CATEGORY => {
            if let Some(name) = category::new_category(hwnd, app) {
                say(
                    hwnd,
                    &format!(
                        "Made the category {name}. Choose it in the Category to show box, Alt+C, \
                         to show it and download into it; Ctrl+Shift+M moves a download into it."
                    ),
                );
            }
            focus(hwnd, IDC_LIST);
            return true;
        }
        IDM_RENAME_CATEGORY => {
            category::rename(hwnd, app);
            return true;
        }
        IDM_DELETE_CATEGORY => {
            category::delete(hwnd, app);
            return true;
        }
        IDM_PLAY_NEXT => {
            app.cfg.play_next = !app.cfg.play_next;
            if app.cfg.play_next {
                "At the end of one, the Play window now goes on to the next in the list."
                    .to_string()
            } else {
                "At the end of one, the Play window now stops.".to_string()
            }
        }
        id if in_list(IDM_SPEED, SPEEDS.len()) => {
            app.cfg.speed = SPEEDS[(id - IDM_SPEED) as usize];
            format!(
                "Playing speed: {}. Shift with the Up and Down arrows changes it while playing.",
                listen::speed_words(app.cfg.speed)
            )
        }
        IDM_ADD => {
            add(hwnd, app, "");
            return true;
        }
        IDM_FOLDER => {
            choose_folder(hwnd, app);
            return true;
        }
        IDM_EXPLORER => {
            let folder = app.cfg.output_folder();
            let _ = std::fs::create_dir_all(&folder);
            downloads::open_folder(&folder);
            return true;
        }
        IDM_UPDATE_YTDLP => {
            if app.job.is_some() {
                say(
                    hwnd,
                    "Already downloading. Update yt-dlp once this download has finished.",
                );
            } else {
                app.job = Some(job::update_ytdlp(hwnd));
                app.quiet = false;
            }
            return true;
        }
        IDM_EXIT => {
            unsafe {
                let _ = PostMessageW(Some(hwnd), WM_CLOSE, WPARAM(0), LPARAM(0));
            }
            return true;
        }
        IDM_STOP => {
            stop(hwnd);
            return true;
        }
        IDM_QUEUE => {
            show_queue(hwnd);
            return true;
        }
        IDM_CLEAR_QUEUE => {
            let said = empty_queue(hwnd, hwnd);
            say(hwnd, &said);
            return true;
        }
        IDM_OPEN => {
            if let Some(e) = chosen(hwnd, app) {
                if e.is_playlist() {
                    open_playlist(hwnd, app, &e);
                } else if let Some(at) = downloads::chosen(list(hwnd)) {
                    play(hwnd, at);
                }
            }
            return true;
        }
        IDM_EXTERNAL => {
            if let Some(e) = chosen(hwnd, app) {
                let said = downloads::open(hwnd, &e);
                say(hwnd, &said);
            }
            return true;
        }
        IDM_BACK => {
            if !back(hwnd, app) {
                say(
                    hwnd,
                    "The list already shows all downloads. Enter on a playlist shows its videos.",
                );
            }
            return true;
        }
        IDM_UPDATE => {
            // Among a playlist's videos, the playlist itself.
            let here = app.inside.clone().map(|folder| library::Entry {
                category: String::new(),
                name: folder
                    .file_name()
                    .unwrap_or_default()
                    .to_string_lossy()
                    .into_owned(),
                kind: Kind::Playlist {
                    videos: 0,
                    url: library::read_record(&folder)
                        .map(|r| r.url)
                        .unwrap_or_default(),
                },
                made: std::time::SystemTime::now(),
                path: folder,
            });
            if let Some(e) = here.or_else(|| chosen(hwnd, app)) {
                match &e.kind {
                    Kind::Playlist { url, .. } if !url.is_empty() => {
                        let folder = e.path.parent().map(PathBuf::from).unwrap_or_default();
                        submit(hwnd, app, url.clone(), true, Some(e.path.clone()), folder);
                    }
                    Kind::Playlist { .. } => say(
                        hwnd,
                        &format!(
                            "VidFetch does not know where the playlist {} came from. Download \
                             it again with Ctrl+N.",
                            e.name
                        ),
                    ),
                    _ => say(
                        hwnd,
                        &format!(
                            "{} is a single download, not a playlist, so there is nothing new to \
                             fetch. Choose a playlist in the list to update it.",
                            e.name
                        ),
                    ),
                }
            }
            return true;
        }
        IDM_RENAME => {
            if let Some(e) = chosen(hwnd, app).filter(|e| !busy_with(hwnd, app, e)) {
                if let Some((path, said)) = downloads::rename(hwnd, &e) {
                    if let Some(record) = Config::past_path() {
                        library::moved_folder(&record, &e.path, &path);
                    }
                    crate::places::moved(&e.path, &path);
                    app.select = Some(path);
                    refresh(hwnd, app);
                    say(hwnd, &said);
                }
                focus(hwnd, IDC_LIST);
            }
            return true;
        }
        IDM_SHOW => {
            if let Some(e) = chosen(hwnd, app) {
                downloads::show_in_explorer(&e.path);
            }
            return true;
        }
        IDM_DELETE => {
            if let Some(e) = chosen(hwnd, app).filter(|e| !busy_with(hwnd, app, e)) {
                let mut said = downloads::delete(hwnd, &e);
                refresh(hwnd, app);
                if app.entries.is_empty() {
                    said += if app.inside.is_some() {
                        " No videos are left in this playlist's folder: Escape goes back to all \
                         downloads."
                    } else {
                        " The list is empty now: press Ctrl+N to download something."
                    };
                }
                say(hwnd, &said);
                focus(hwnd, IDC_LIST);
            }
            return true;
        }
        IDM_HELP => {
            help(hwnd);
            return true;
        }
        IDM_ABOUT => {
            message::about(hwnd, &about());
            return true;
        }
        IDM_NUMBER => {
            app.cfg.number = !app.cfg.number;
            if app.cfg.number {
                "A playlist's files now start with their place in it, such as 03, so they sort \
                 in the playlist's order."
                    .to_string()
            } else {
                "A playlist's files are now named by their titles alone.".to_string()
            }
        }
        IDM_CAPTIONS => {
            app.cfg.captions = !app.cfg.captions;
            if app.cfg.captions {
                "Each download's captions are now saved beside it as an SRT file, when it has any."
                    .to_string()
            } else {
                "Captions are no longer saved.".to_string()
            }
        }
        id if in_list(IDM_FORMAT, FORMATS.len()) => {
            app.cfg.format = FORMATS[(id - IDM_FORMAT) as usize].0;
            format!("Downloads are now saved as: {}.", app.cfg.format_label())
        }
        id if in_list(IDM_QUALITY, QUALITIES.len()) => {
            app.cfg.lines = QUALITIES[(id - IDM_QUALITY) as usize].0;
            format!(
                "Picture quality: {}. Sound only downloads are not affected.",
                app.cfg.quality_label()
            )
        }
        _ => return false,
    };
    app.cfg.save();
    say(hwnd, &said);
    true
}

fn choose_folder(hwnd: HWND, app: &mut App) {
    let current = app.cfg.output_folder();
    if let Some(folder) = pick_folder(hwnd, Some(&current), "Choose the folder for downloads") {
        let loose = library::list(&folder).len();
        if loose > 0
            && !message::ask(
                hwnd,
                &format!(
                    "{} holds {} directly in it: sound or video files, or playlists. VidFetch \
                     keeps downloads in category folders, so it would move them into a folder \
                     called Uncategorised there. Move them, or keep the output folder as it was?",
                    folder.display(),
                    count(loose)
                ),
                "&Move them",
                "&Keep the output folder as it was",
                false,
            )
        {
            say(hwnd, "The output folder was not changed.");
            focus(hwnd, IDC_LIST);
            return;
        }
        app.cfg.output_folder = Some(folder.clone());
        app.cfg.category = None;
        app.cfg.save();
        let moved = category::tidy(app);
        category::fill(hwnd, app);
        app.online.clear();
        app.inside = None;
        name_list(hwnd, app);
        refresh(hwnd, app);
        let moved = if moved > 0 {
            format!(" {} moved into Uncategorised.", count(moved))
        } else {
            String::new()
        };
        say(
            hwnd,
            &format!(
                "Downloads will be saved in {}. The list now shows the {} there.{moved}",
                folder.display(),
                count(app.entries.len())
            ),
        );
    }
    focus(hwnd, IDC_LIST);
}

/// A folder picker. None when cancelled, which is not a problem to report.
pub fn pick_folder(owner: HWND, start: Option<&Path>, title: &str) -> Option<PathBuf> {
    unsafe {
        let dialog: IFileOpenDialog =
            CoCreateInstance(&FileOpenDialog, None, CLSCTX_INPROC_SERVER).ok()?;
        let options = dialog.GetOptions().ok()?;
        dialog
            .SetOptions(options | FOS_PICKFOLDERS | FOS_FORCEFILESYSTEM)
            .ok()?;
        let title = wide(title);
        let _ = dialog.SetTitle(PCWSTR(title.as_ptr()));
        if let Some(start) = start.filter(|p| p.is_dir()) {
            let path = wide(&start.display().to_string());
            if let Ok(item) =
                SHCreateItemFromParsingName::<_, _, IShellItem>(PCWSTR(path.as_ptr()), None)
            {
                let _ = dialog.SetFolder(&item);
            }
        }
        dialog.Show(Some(owner)).ok()?;
        let item = dialog.GetResult().ok()?;
        let name: PWSTR = item.GetDisplayName(SIGDN_FILESYSPATH).ok()?;
        let path = name.to_string().ok();
        CoTaskMemFree(Some(name.0 as _));
        path.map(PathBuf::from)
    }
}

fn about() -> String {
    format!(
        "VidFetch, version {}.\n\nDownloads videos and playlists from YouTube and many other \
         sites, with a screen reader first. Free and open source, under the MIT licence.\n\n\
         By Laurent Cadet de Fontenay, with assistance from Claude. Downloads by yt-dlp \
         (Unlicense), fetched from its own page on GitHub; picture and sound put together by \
         Windows' own Media Foundation.\n\nThe help (F1) has the details.",
        env!("CARGO_PKG_VERSION")
    )
}

/// Say it in the Status box and out loud, for things that happen without
/// focus moving: a choice from a menu, a download starting or ending.
fn say(hwnd: HWND, text: &str) {
    set_text(hwnd, IDC_STATUS, text);
    if !HUSH.with(|h| h.get()) {
        crate::announce::say(hwnd, text);
    }
}

const HELP: &str = include_str!("help.html");

/// F1, or Help in the Help menu: the help page in the default browser, where
/// a screen reader can move from heading to heading.
fn help(hwnd: HWND) {
    let written = Config::dir().and_then(|dir| {
        std::fs::create_dir_all(&dir).ok()?;
        let path = dir.join("help.html");
        std::fs::write(
            &path,
            HELP.replace("{{VERSION}}", env!("CARGO_PKG_VERSION")),
        )
        .ok()?;
        Some(path)
    });
    let Some(path) = written else {
        message::problem(
            hwnd,
            "The help page could not be written to the settings folder, so it cannot be opened.",
        );
        return;
    };
    // A file:/// address goes to the default browser; a bare path can bring
    // up an "Open with" chooser instead, which ShellExecute calls success.
    let url = file_url(&path);
    let r = unsafe {
        windows::Win32::UI::Shell::ShellExecuteW(
            Some(hwnd),
            &windows::core::HSTRING::from("open"),
            &windows::core::HSTRING::from(url.as_str()),
            PCWSTR::null(),
            PCWSTR::null(),
            SW_SHOWNORMAL,
        )
    };
    if r.0 as isize <= 32 {
        message::problem(
            hwnd,
            &format!(
                "Windows could not open the help page in a browser. It is saved at {}; you can \
                 open it from File Explorer.",
                path.display()
            ),
        );
    }
}

/// `C:\Users\Ann Lee\help.html` as `file:///C:/Users/Ann%20Lee/help.html`.
fn file_url(path: &Path) -> String {
    let mut url = String::from("file:///");
    for b in path.to_string_lossy().replace('\\', "/").bytes() {
        match b {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'/' | b':' | b'.' | b'-' | b'_' | b'~' => {
                url.push(b as char)
            }
            _ => url += &format!("%{b:02X}"),
        }
    }
    url
}

// ---------------------------------------------------------------- control helpers

pub fn set_text(hwnd: HWND, id: i32, text: &str) {
    let t = wide(&text.replace("\r\n", "\n").replace('\n', "\r\n"));
    unsafe {
        let _ = SetDlgItemTextW(hwnd, id, PCWSTR(t.as_ptr()));
    }
    // Leave the caret where it is in the box being typed in.
    let typing = unsafe { GetDlgItem(Some(hwnd), id).is_ok_and(|c| GetFocus() == c) };
    if !typing {
        caret_to_start(hwnd, id);
    }
}

pub fn get_text(hwnd: HWND, id: i32) -> String {
    unsafe {
        let Ok(control) = GetDlgItem(Some(hwnd), id) else {
            return String::new();
        };
        let len = GetWindowTextLengthW(control).max(0) as usize;
        let mut buf = vec![0u16; len + 1];
        let n = GetWindowTextW(control, &mut buf) as usize;
        String::from_utf16_lossy(&buf[..n]).replace("\r\n", "\n")
    }
}

/// Put the caret at the start, so review with the arrow keys begins at the top.
pub fn caret_to_start(hwnd: HWND, id: i32) {
    unsafe {
        if let Ok(edit) = GetDlgItem(Some(hwnd), id) {
            SendMessageW(edit, EM_SETSEL, Some(WPARAM(0)), Some(LPARAM(0)));
            SendMessageW(edit, EM_SCROLLCARET, None, None);
        }
    }
}

/// Move focus the way a dialog expects (WM_NEXTDLGCTL), so the dialog manager
/// also moves the default-button highlight. A bare SetFocus leaves it behind.
pub fn focus(hwnd: HWND, id: i32) {
    unsafe {
        if let Ok(control) = GetDlgItem(Some(hwnd), id) {
            SendMessageW(
                hwnd,
                WM_NEXTDLGCTL,
                Some(WPARAM(control.0 as usize)),
                Some(LPARAM(1)),
            );
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Menu titles and control labels both answer Alt+letter in the main
    /// window, so no letter may be used twice.
    #[test]
    fn main_window_alt_letters_are_unique() {
        let rc = include_str!("../vidfetch.rc");
        let block = |start: &str| {
            let b = &rc[rc.find(start).unwrap()..];
            b[..b.find("\nEND").unwrap()].to_string()
        };
        let (main, bar) = (block("IDD_MAIN DIALOGEX"), block("IDR_MENU MENU"));
        let letter = |line: &str| {
            let q = line.split('"').nth(1)?;
            let at = q.find('&')?;
            q[at + 1..].chars().next().map(|c| c.to_ascii_lowercase())
        };
        let mut letters: Vec<char> = main
            .lines()
            .filter(|l| {
                let l = l.trim_start();
                l.starts_with("LTEXT")
                    || l.starts_with("DEFPUSHBUTTON")
                    || l.starts_with("PUSHBUTTON")
            })
            // The Enter button's other words keep its letter, P.
            .chain([
                "\"Show the &playlist's videos\"",
                "\"O&pen the channel's videos\"",
            ])
            .filter_map(letter)
            .collect();
        // Top-level menus only: indented by exactly four spaces.
        letters.extend(
            bar.lines()
                .filter(|l| l.starts_with("    POPUP"))
                .filter_map(letter),
        );
        assert_eq!(letters.len(), 18, "{letters:?}");
        let mut sorted = letters.clone();
        sorted.sort();
        sorted.dedup();
        // The Enter button counted three times, as its words change.
        assert_eq!(sorted.len(), letters.len() - 2, "{letters:?}");
    }

    /// Within each menu, every item's letter is unique.
    #[test]
    fn menu_letters_are_unique_within_each_menu() {
        let rc = include_str!("../vidfetch.rc");
        let bar = &rc[rc.find("IDR_MENU MENU").unwrap()..];
        let bar = &bar[..bar.find("\nEND").unwrap()];
        let mut menu: Vec<char> = Vec::new();
        for line in bar.lines() {
            if line.starts_with("    POPUP") {
                menu.clear();
            } else if line.starts_with("        MENUITEM \"") || line.starts_with("        POPUP") {
                let q = line.split('"').nth(1).unwrap();
                if let Some(at) = q.find('&') {
                    let c = q[at + 1..].chars().next().unwrap().to_ascii_lowercase();
                    assert!(!menu.contains(&c), "{c} twice: {line}");
                    menu.push(c);
                }
            }
        }
    }

    /// The menu positions in ui.rs match the template.
    #[test]
    fn menu_positions_match_the_template() {
        let rc = include_str!("../vidfetch.rc");
        let bar = &rc[rc.find("IDR_MENU MENU").unwrap()..];
        let tops: Vec<&str> = bar.lines().filter(|l| l.starts_with("    POPUP")).collect();
        assert!(tops[OPTIONS_MENU as usize].contains("&Options"));
        let options = &bar[bar.find("\"&Options\"").unwrap()..];
        let options = &options[..options.find("\n    END").unwrap()];
        let items: Vec<&str> = options
            .lines()
            .filter(|l| l.starts_with("        MENUITEM") || l.starts_with("        POPUP"))
            .collect();
        assert!(items[FORMAT_MENU as usize].contains("&Download as"));
        assert!(items[QUALITY_MENU as usize].contains("Picture &quality"));
        assert!(items[SPEED_MENU as usize].contains("Playing &speed"));
    }

    #[test]
    fn help_addresses_are_percent_encoded() {
        assert_eq!(
            file_url(Path::new(r"C:\Users\Ann Lee\VidFetch\help.html")),
            "file:///C:/Users/Ann%20Lee/VidFetch/help.html"
        );
    }
}
