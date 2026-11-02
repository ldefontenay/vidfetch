//! The main window: a modeless dialog from the IDD_MAIN template, so every
//! control is a real Win32 control that a screen reader already knows. Video
//! Scribe's main window, without the describing.

use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

use windows::core::{PCWSTR, PWSTR};
use windows::Win32::Foundation::{HWND, LPARAM, WPARAM};
use windows::Win32::System::Com::{CoCreateInstance, CoTaskMemFree, CLSCTX_INPROC_SERVER};
use windows::Win32::System::LibraryLoader::GetModuleHandleW;
use windows::Win32::UI::Input::KeyboardAndMouse::{
    GetFocus, GetKeyState, VIRTUAL_KEY, VK_CONTROL, VK_DELETE, VK_ESCAPE, VK_F2, VK_MENU, VK_SHIFT,
};
use windows::Win32::UI::Shell::{
    FileOpenDialog, IFileOpenDialog, IShellItem, SHCreateItemFromParsingName, FOS_FORCEFILESYSTEM,
    FOS_PICKFOLDERS, SIGDN_FILESYSPATH,
};
use windows::Win32::UI::WindowsAndMessaging::*;

use crate::config::{Config, FORMATS, QUALITIES};
use crate::downloads;
use crate::job::{self, Event};
use crate::library::{self, Kind};
use crate::message;
use crate::wide;
use crate::ytdlp;

const IDD_MAIN: u16 = 100;
const IDC_STATUS: i32 = 1004;
const IDC_LIST: i32 = 1005;
const IDC_OPEN: i32 = 1006;
const IDC_ADD: i32 = 1009;
const IDC_RENAME: i32 = 1010;
const IDC_SHOW: i32 = 1011;
const IDC_DELETE: i32 = 1012;
const IDC_UPDATE: i32 = 1013;

// Menu commands, in step with vidfetch.rc.
const IDM_FOLDER: i32 = 2002;
const IDM_EXPLORER: i32 = 2003;
const IDM_EXIT: i32 = 2004;
const IDM_UPDATE_YTDLP: i32 = 2005;
const IDM_STOP: i32 = 2011;
const IDM_NUMBER: i32 = 2012;
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

// Where each menu sits, in step with vidfetch.rc.
const OPTIONS_MENU: i32 = 2;
const FORMAT_MENU: i32 = 0;
const QUALITY_MENU: i32 = 1;

const EM_SETSEL: u32 = 0x00B1;
const EM_SCROLLCARET: u32 = 0x00B7;
const EN_SETFOCUS: u32 = 0x0100;
const LBN_DBLCLK: u32 = 2;

const WM_WELCOME: u32 = WM_APP + 13;

const WELCOME: &str = "Welcome to VidFetch. It downloads videos and whole playlists from YouTube \
and many other sites, as video or as sound only.\n\n\
Copy the address of a video or a playlist in your browser, with Alt+D then Ctrl+C, then press \
Ctrl+N here: the address is already filled in, so Enter starts the download. Each download takes \
its place in the list of downloads, where Enter opens it and the Applications key opens a menu of \
what else can be done. A playlist gets a folder of its own; Ctrl+U on it later fetches only its \
new videos.\n\n\
The Options menu (Alt+O) chooses video or sound only, and the picture quality. F1 opens the help.";

struct App {
    cfg: Config,
    /// No settings file yet: say hello and explain.
    welcome: bool,
    /// The job running now, by its stop flag.
    job: Option<Arc<AtomicBool>>,
    /// The playlist folder the job running now is filling, which must stay
    /// as it is until it is done.
    running: Option<PathBuf>,
    /// The downloads in the list, most recent first.
    entries: Vec<library::Entry>,
    /// The file or folder to choose in the list next time it is filled.
    select: Option<PathBuf>,
}

pub fn run(cfg: Config, welcome: bool) {
    let app = Box::new(App {
        cfg,
        welcome,
        job: None,
        running: None,
        entries: Vec::new(),
        select: None,
    });
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
                refresh(hwnd, app);
                let ready = match app.entries.len() {
                    0 => "Ready. Nothing downloaded yet: press Ctrl+N to download a video or a \
                          playlist."
                        .to_string(),
                    1 => "Ready. 1 download. Enter opens it; Ctrl+N downloads another.".to_string(),
                    n => format!(
                        "Ready. {n} downloads. Enter opens the chosen one; Ctrl+N downloads \
                         another."
                    ),
                };
                set_text(hwnd, IDC_STATUS, &ready);
                focus(hwnd, IDC_LIST);
            }
            if app(hwnd).is_some_and(|a| a.welcome) {
                let _ = PostMessageW(Some(hwnd), WM_WELCOME, WPARAM(0), LPARAM(0));
            }
            0 // zero: we set the focus ourselves
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
                (IDC_LIST, LBN_DBLCLK) => {
                    command(hwnd, app, IDM_OPEN);
                }
                (IDC_LIST, _) => return 0,
                // The buttons do what their menu items do. Enter is Open
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
                (IDC_RENAME, _) => {
                    command(hwnd, app, IDM_RENAME);
                }
                (IDC_SHOW, _) => {
                    command(hwnd, app, IDM_SHOW);
                }
                (IDC_DELETE, _) => {
                    command(hwnd, app, IDM_DELETE);
                }
                (id, _) if id == IDCANCEL.0 => stop(hwnd),
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
        // Delete in the list (LBS_WANTKEYBOARDINPUT); F2 is a shortcut
        // everywhere in the window.
        WM_VKEYTOITEM => {
            let key = (wparam.0 & 0xFFFF) as u16;
            if key != VK_DELETE.0 {
                return -1;
            }
            if let Some(app) = app(hwnd) {
                command(hwnd, app, IDM_DELETE);
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
                stop(hwnd);
                return 1;
            }
            if let Some(app) = app(hwnd) {
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

/// Fills the list of downloads again from the output folder, keeping the
/// one chosen, or choosing the one a job just saved.
fn refresh(hwnd: HWND, app: &mut App) {
    let lb = list(hwnd);
    let at = downloads::chosen(lb);
    let path = app
        .select
        .take()
        .or_else(|| at.and_then(|i| app.entries.get(i)).map(|e| e.path.clone()));
    app.entries = library::list(&app.cfg.output_folder());
    downloads::fill(lb, &app.entries, path.as_deref(), at.unwrap_or(0));
}

/// The download chosen in the list; when there is none, that is said.
fn chosen(hwnd: HWND, app: &App) -> Option<library::Entry> {
    let e = downloads::chosen(list(hwnd))
        .and_then(|i| app.entries.get(i))
        .cloned();
    if e.is_none() {
        say(
            hwnd,
            "There is nothing in the list of downloads. Press Ctrl+N to download a video or a \
             playlist.",
        );
    }
    e
}

/// The running job is filling this playlist's folder, so it must stay as it is.
fn busy_with(hwnd: HWND, app: &App, e: &library::Entry) -> bool {
    let busy = app.job.is_some() && app.running.as_deref() == Some(e.path.as_path());
    if busy {
        say(
            hwnd,
            "VidFetch is downloading into this playlist's folder now. Wait for it to finish, or \
             press Escape to stop it first.",
        );
    }
    busy
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
    let Some(e) = chosen(hwnd, app) else {
        return;
    };
    let mut items = vec![(IDM_OPEN, "&Open\tEnter")];
    if e.is_playlist() {
        items.push((IDM_UPDATE, "&Update the playlist\tCtrl+U"));
    }
    items.extend([
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
    if app.job.is_some() {
        say(
            hwnd,
            "Already downloading. Press Escape to stop, or wait for it to finish.",
        );
        return;
    }
    let before = app.cfg.clone();
    let chosen = crate::addvideo::show(hwnd, &mut app.cfg, text);
    if app.cfg != before {
        app.cfg.save();
    }
    match chosen {
        Some(url) => {
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
            start(hwnd, app, url, whole, None);
        }
        None => {
            say(hwnd, "Nothing downloaded.");
            focus(hwnd, IDC_LIST);
        }
    }
}

/// Downloads `url`; `whole` takes the whole playlist a video is in, and
/// `into` is the playlist's folder when it is being updated.
fn start(hwnd: HWND, app: &mut App, url: String, whole: bool, into: Option<PathBuf>) {
    if app.job.is_some() {
        say(
            hwnd,
            "Already downloading. Press Escape to stop, or wait for it to finish.",
        );
        return;
    }
    let mut fetch_ytdlp = false;
    if ytdlp::find().is_none() {
        fetch_ytdlp = message::ask(
            hwnd,
            "VidFetch downloads videos with yt-dlp, a free program that is not on this computer \
             yet. Download it now from its official page on GitHub? It is about 18 MB, is checked \
             against its published checksum, and is kept in VidFetch's own folder.",
            "&Download",
            "&Not now",
            true,
        );
        if !fetch_ytdlp {
            say(hwnd, "Not started: downloading needs yt-dlp.");
            focus(hwnd, IDC_LIST);
            return;
        }
    }
    app.running = into;
    let settings = job::Settings {
        url,
        output_folder: app.cfg.output_folder(),
        format: app.cfg.format,
        lines: app.cfg.lines,
        number: app.cfg.number,
        whole_list: whole,
        fetch_ytdlp,
    };
    app.job = Some(job::start(hwnd, settings));
    say(hwnd, "Started. Press Escape to stop.");
    focus(hwnd, IDC_STATUS);
}

fn stop(hwnd: HWND) {
    let Some(app) = (unsafe { app(hwnd) }) else {
        return;
    };
    match &app.job {
        Some(flag) => {
            flag.store(true, Ordering::Relaxed);
            say(hwnd, "Stopping.");
        }
        None => say(hwnd, "Nothing is downloading, so there is nothing to stop."),
    }
}

/// News from the job.
fn job_event(hwnd: HWND, app: &mut App, event: Event) {
    match event {
        Event::Step(text) => say(hwnd, &text),
        Event::Progress(text) => set_text(hwnd, IDC_STATUS, &text),
        Event::Done { said, path, report } => {
            app.job = None;
            app.running = None;
            app.select = Some(path);
            refresh(hwnd, app);
            say(hwnd, &said);
            if let Some(r) = report {
                message::report(hwnd, &r);
            }
            focus(hwnd, IDC_LIST);
        }
        Event::Failed(why) => {
            app.job = None;
            app.running = None;
            refresh(hwnd, app);
            set_text(hwnd, IDC_STATUS, "Stopped by a problem.");
            message::problem(hwnd, &why);
            focus(hwnd, IDC_LIST);
        }
        Event::Said(text) => {
            app.job = None;
            app.running = None;
            say(hwnd, &text);
        }
        Event::Stopped(text) => {
            app.job = None;
            app.running = None;
            refresh(hwnd, app);
            say(hwnd, &text);
            focus(hwnd, IDC_LIST);
        }
    }
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
    }
    unsafe {
        CheckMenuItem(
            menu,
            IDM_NUMBER as u32,
            (MF_BYCOMMAND | if cfg.number { MF_CHECKED } else { MF_UNCHECKED }).0,
        );
    }
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
    if down(VK_MENU) || !down(VK_CONTROL) || down(VK_SHIFT) {
        return None;
    }
    match key as u8 {
        b'N' => Some(IDM_ADD),
        b'U' => Some(IDM_UPDATE),
        _ => None,
    }
}

/// Commands from the menus and their shortcuts. Every choice is saved at once
/// and said out loud, as the menu closes without a word.
fn command(hwnd: HWND, app: &mut App, id: i32) -> bool {
    let in_list = |first: i32, len: usize| (first..first + len as i32).contains(&id);
    let said = match id {
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
        IDM_OPEN => {
            if let Some(e) = chosen(hwnd, app) {
                let said = downloads::open(hwnd, &e);
                say(hwnd, &said);
            }
            return true;
        }
        IDM_UPDATE => {
            if let Some(e) = chosen(hwnd, app) {
                match &e.kind {
                    Kind::Playlist { url, .. } if !url.is_empty() => {
                        start(hwnd, app, url.clone(), true, Some(e.path.clone()));
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
                    said += " The list is empty now: press Ctrl+N to download something.";
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
        app.cfg.output_folder = Some(folder.clone());
        app.cfg.save();
        refresh(hwnd, app);
        say(
            hwnd,
            &format!(
                "Downloads will be saved in {}. The list now shows the {} downloads there.",
                folder.display(),
                app.entries.len()
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
    crate::announce::say(hwnd, text);
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
            .filter_map(letter)
            .collect();
        // Top-level menus only: indented by exactly four spaces.
        letters.extend(
            bar.lines()
                .filter(|l| l.starts_with("    POPUP"))
                .filter_map(letter),
        );
        assert_eq!(letters.len(), 12, "{letters:?}");
        let mut sorted = letters.clone();
        sorted.sort();
        sorted.dedup();
        assert_eq!(sorted.len(), letters.len(), "{letters:?}");
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
    }

    #[test]
    fn help_addresses_are_percent_encoded() {
        assert_eq!(
            file_url(Path::new(r"C:\Users\Ann Lee\VidFetch\help.html")),
            "file:///C:/Users/Ann%20Lee/VidFetch/help.html"
        );
    }
}
