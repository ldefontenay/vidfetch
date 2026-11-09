//! The Play window, opened by Enter on a download (asked for in the
//! milestone 2 test, 2.4), or on a video found online, which plays straight
//! from YouTube: its sound, with Play or pause, Back and Forward 10 seconds,
//! and the arrow keys for 2 seconds. Video Scribe's listen.rs (MIT, same
//! author), whose user asked for it here. A video's picture is not shown;
//! Open in the default player, in the list's context menu, does that.
//!
//! Every move is said out loud, as the window has nothing to see: where
//! playing is now, and whether it plays. Except a skip while playing: the
//! sound says where it is, and a time over it is clutter (Video Scribe's
//! round 2 and 3 tests); T says it on request. The Where playing is box
//! holds the same, for reading at any time, and is kept current even while
//! it has focus (milestone 4 test, 2.1).
//!
//! Added for VidFetch (log entry 3, ideas 1 to 4): it carries on where the
//! user stopped last time; Up and Down set the volume, Shift with them the
//! speed; Page Down and Page Up go to the next or previous in the list, as
//! does the end of one when the user asked for that. A YouTube video's
//! chapters, fetched as it starts, are Ctrl with Page Down and Page Up.
//!
//! The window stays open from one to the next, so only the next one's name
//! is said (milestone 4 test, 2.5), and a video online plays as soon as
//! yt-dlp has its address, with "Please wait" until then (3.3).

use std::cell::Cell;
use std::path::PathBuf;
use std::sync::mpsc::{Receiver, TryRecvError};
use std::sync::{Arc, Mutex};

use windows::core::PCWSTR;
use windows::Win32::Foundation::{HWND, LPARAM, LRESULT, WPARAM};
use windows::Win32::System::LibraryLoader::GetModuleHandleW;
use windows::Win32::System::Threading::GetCurrentThreadId;
use windows::Win32::UI::Input::KeyboardAndMouse::{
    GetKeyState, VK_CONTROL, VK_DOWN, VK_END, VK_HOME, VK_LEFT, VK_MENU, VK_NEXT, VK_PRIOR,
    VK_RIGHT, VK_SHIFT, VK_UP,
};
use windows::Win32::UI::WindowsAndMessaging::*;

use crate::chapters::{self, Chapter};
use crate::player::{Player, NO_SOUND, SPEED_STEP};
use crate::ui::{focus, set_text};
use crate::{places, wide};

const IDD_PLAY: u16 = 110;
const IDC_L_WHERE: i32 = 1060;
const IDC_L_PLAY: i32 = 1061;
const IDC_L_BACK: i32 = 1062;
const IDC_L_FORWARD: i32 = 1063;
const IDC_L_KEYS: i32 = 1064;

/// What the keys do, in a box to read, not said as the window opens.
const KEYS: &str = "Left and Right arrows: move 2 seconds back or forward.\r\n\
                    Home: start of video. End: move to 10 seconds before the end.\r\n\
                    T: time elapsed.\r\n\
                    Up and Down arrows: adjust volume.\r\n\
                    Shift with Up and Down arrows: adjust speed.\r\n\
                    Page Down and Page Up: skip to next or previous video in the list.\r\n\
                    Ctrl Page Down and Page Up: jump to next or previous chapter.\r\n\
                    Ctrl+D: download video, when listening online.\r\n\
                    Tab out of this help box to make sure these keys work reliably.\r\n\
                    Escape: close player.";

/// The timer that quietens the window's opening, how often, and how many times.
const HUSH_TIMER: usize = 77;
const HUSH_MS: u32 = 220;
const HUSHES: u32 = 4;

/// Seconds a button skips, and an arrow key.
const SKIP: f64 = 10.0;
const STEP: f64 = 2.0;
/// Percent of volume an arrow key moves (milestone 4 test, 2.2: "increments
/// of 5%").
const LOUDER: i32 = 5;
/// The key that says where playing is, and the one that downloads.
const VK_T: u16 = b'T' as u16;
const VK_D: u16 = b'D' as u16;

/// How often the window checks on the player: for the end, for a sound
/// that has come, and to keep the Where playing is box current.
const TICK_MS: u32 = 250;
const TIMER: usize = 1;

/// Where the sound is.
pub enum Source {
    /// A file, or a web address.
    Ready(String),
    /// Being found on a thread: its address, or why not, when it comes.
    Coming(Receiver<Result<String, String>>),
}

/// One thing to play.
pub struct Track {
    pub source: Source,
    /// What is playing, for the title.
    pub name: String,
    /// Where it is remembered (places.rs).
    pub key: String,
    /// The YouTube video it is, if known, for its chapters.
    pub video: Option<String>,
    /// Its web page, if it is online, to fetch its sound whole if need be.
    pub page: Option<String>,
    pub has_next: bool,
    pub has_previous: bool,
    /// Ctrl+D: downloads it, if it is online; what to say.
    pub download: Option<Box<dyn FnMut() -> String>>,
}

/// What to play, and what the window may do besides.
pub struct Listen<'a> {
    pub track: Track,
    /// Playing speed, kept between windows.
    pub speed: f64,
    /// At the end, go on to the next.
    pub play_next: bool,
    /// Every silence cut to a second.
    pub cut_silence: bool,
    /// Page Down (true) and Page Up: the next or previous in the list, which
    /// the list chooses as it goes.
    pub step: Option<&'a mut dyn FnMut(bool) -> Option<Track>>,
}

/// A video's chapters: None while they are being fetched.
type Chapters = Arc<Mutex<Option<Vec<Chapter>>>>;

struct State<'a, 'b> {
    /// None until the sound has come.
    player: Option<Player>,
    listen: &'a mut Listen<'b>,
    /// What the Where playing is box last said, to change it only when needed.
    shown: String,
    chapters: Chapters,
    /// Say the name as it starts: it came by Page Down, or at the end of
    /// the one before.
    stepped: bool,
    /// How many times the opening has been quietened.
    hushed: u32,
    /// The file played instead, after a source gave no sound.
    whole: bool,
}

thread_local! {
    /// The open window, for the keyboard hook.
    static OPEN: Cell<Option<HWND>> = const { Cell::new(None) };
}

/// Opens the window and plays, from where the user stopped last time if they
/// did, and on through the list as the user goes. Why the first could not
/// play, if it could not.
pub fn show(owner: HWND, listen: &mut Listen) -> Result<(), String> {
    let player = match &listen.track.source {
        Source::Ready(s) => Some(Player::open(s, listen.speed, listen.cut_silence)?),
        Source::Coming(_) => None,
    };
    let mut state = State {
        player,
        chapters: chapters_of(listen.track.video.clone()),
        listen,
        shown: String::new(),
        stepped: false,
        hushed: 0,
        whole: false,
    };
    unsafe {
        let hook = SetWindowsHookExW(WH_MSGFILTER, Some(keys), None, GetCurrentThreadId()).ok();
        if let Ok(instance) = GetModuleHandleW(None) {
            DialogBoxParamW(
                Some(instance.into()),
                PCWSTR(IDD_PLAY as usize as *const u16),
                Some(owner),
                Some(proc),
                LPARAM(&mut state as *mut State as isize),
            );
        }
        if let Some(h) = hook {
            let _ = UnhookWindowsHookEx(h);
        }
    }
    OPEN.with(|o| o.set(None));
    keep_place(&mut state);
    Ok(())
}

/// Remembers where the one playing is, and the speed.
fn keep_place(s: &mut State) {
    if let Some(p) = &s.player {
        let at = if p.finished() { 0.0 } else { p.at() };
        places::set(&s.listen.track.key, at, p.secs());
        s.listen.speed = p.speed();
    }
}

/// Fetches a video's chapters on a thread.
fn chapters_of(video: Option<String>) -> Chapters {
    let chapters: Chapters = Arc::new(Mutex::new(None));
    match video {
        Some(id) => {
            let theirs = chapters.clone();
            std::thread::spawn(move || {
                let stop = std::sync::atomic::AtomicBool::new(false);
                let found = crate::youtube::details(&id, &stop)
                    .map(|d| chapters::from_description(&d.description))
                    .unwrap_or_default();
                *theirs.lock().unwrap() = Some(found);
            });
        }
        None => *chapters.lock().unwrap() = Some(Vec::new()),
    }
    chapters
}

/// Seconds as "1:23" or "1:02:05", whole seconds: tenths are too many to
/// listen to.
pub fn clock(t: f64) -> String {
    let s = t.max(0.0).floor() as u64;
    if s >= 3600 {
        format!("{}:{:02}:{:02}", s / 3600, s / 60 % 60, s % 60)
    } else {
        format!("{}:{:02}", s / 60, s % 60)
    }
}

/// "1:23 of 5:11", or "1:23" when the length is not known.
fn where_text(at: f64, secs: f64) -> String {
    if secs > 0.0 {
        format!("{} of {}", clock(at), clock(secs))
    } else {
        clock(at)
    }
}

/// "normal speed", or "1.25 times as fast".
pub fn speed_words(speed: f64) -> String {
    if (speed - 1.0).abs() < 0.01 {
        "normal speed".into()
    } else if speed < 1.0 {
        format!("{} times, slower", trim(speed))
    } else {
        format!("{} times as fast", trim(speed))
    }
}

fn trim(x: f64) -> String {
    let t = format!("{x:.2}");
    t.trim_end_matches('0').trim_end_matches('.').to_string()
}

unsafe fn state<'a>(hwnd: HWND) -> &'a mut State<'a, 'a> {
    &mut *(GetWindowLongPtrW(hwnd, GWLP_USERDATA) as *mut State)
}

fn say(hwnd: HWND, text: &str) {
    crate::announce::say(hwnd, text);
}

fn how(p: &Player) -> String {
    let playing = if p.playing() { "playing" } else { "paused" };
    if (p.speed() - 1.0).abs() < 0.01 {
        playing.to_string()
    } else {
        format!("{playing}, {}", speed_words(p.speed()))
    }
}

/// What the Where playing is box says while the sound is still coming.
fn waiting(s: &State) -> String {
    format!(
        "Waiting for the sound of {} from YouTube.",
        s.listen.track.name
    )
}

/// Keeps the Where playing is box current, even while it has focus: a
/// screen reader reads the new text when asked, without being interrupted.
fn show_where(hwnd: HWND, s: &mut State) {
    let text = match &s.player {
        Some(p) => format!("{}, {}", where_text(p.at(), p.secs()), how(p)),
        None => waiting(s),
    };
    if text != s.shown {
        set_text(hwnd, IDC_L_WHERE, &text);
        s.shown = text;
    }
}

/// Starts the player just opened, from where the user stopped before. Nothing
/// is said (milestone 6 test, 2.1: "absolutely nothing announced"), except
/// the name of what Page Down or Page Up moved to.
fn start(hwnd: HWND, s: &mut State) {
    let Some(p) = &s.player else { return };
    let from = places::get(&s.listen.track.key).filter(|&at| p.secs() <= 0.0 || at < p.secs());
    if let Some(at) = from {
        p.seek(at);
    }
    p.play();
    show_where(hwnd, s);
    if s.stepped {
        say(hwnd, &format!("{}.", s.listen.track.name));
    }
}

/// JAWS reads a window as it opens and nothing tells it not to, so what it is
/// reading is stopped, a few times as it may start late; then, if the sound
/// is still being found, "Please wait", the only thing said.
fn hush_opening(hwnd: HWND, s: &mut State) {
    crate::announce::hush();
    s.hushed += 1;
    if s.hushed >= HUSHES {
        unsafe {
            let _ = KillTimer(Some(hwnd), HUSH_TIMER);
        }
        if s.player.is_none() {
            say(hwnd, "Please wait.");
        }
    }
}

fn toggle(hwnd: HWND, s: &mut State) {
    let Some(p) = &s.player else {
        say(hwnd, "Please wait: it has not started yet.");
        return;
    };
    let at = where_text(p.at(), p.secs());
    if p.playing() {
        p.pause();
        say(hwnd, &format!("Paused at {at}."));
    } else {
        p.play();
        say(hwnd, &format!("Playing from {at}."));
    }
    show_where(hwnd, s);
}

/// Moves by `by` seconds. Paused, says the time it moved to; playing,
/// nothing, as the sound says where it is.
fn skip(hwnd: HWND, s: &mut State, by: f64) {
    let Some(p) = &s.player else {
        say(hwnd, "Please wait: it has not started yet.");
        return;
    };
    let to = p.seek(p.at() + by);
    if !p.playing() {
        say(hwnd, &clock(to));
    }
    show_where(hwnd, s);
}

/// The chapter playing, as said: "chapter 2 of 5, The plan".
fn chapter_words(s: &State) -> Option<String> {
    let all = s.chapters.lock().unwrap();
    let list = all.as_ref()?;
    let i = chapters::at(list, s.player.as_ref()?.at())?;
    Some(format!(
        "chapter {} of {}, {}",
        i + 1,
        list.len(),
        list[i].name
    ))
}

/// Goes to the next chapter, or back to the start of this one or the one
/// before, as players do: back within 3 seconds of a chapter's start goes to
/// the one before.
fn chapter(hwnd: HWND, s: &mut State, next: bool) {
    let Some(now) = s.player.as_ref().map(|p| p.at()) else {
        say(hwnd, "Please wait: it has not started yet.");
        return;
    };
    let list = s.chapters.lock().unwrap().clone();
    let Some(list) = list else {
        say(
            hwnd,
            "The chapters are still being fetched. Try again in a moment.",
        );
        return;
    };
    if list.is_empty() {
        say(
            hwnd,
            if s.listen.track.video.is_some() {
                "This video has no chapters."
            } else {
                "VidFetch does not know which YouTube video this is, so it has no chapters."
            },
        );
        return;
    }
    let i = chapters::at(&list, now).unwrap_or(0);
    let to = if next {
        if i + 1 >= list.len() {
            say(hwnd, "This is the last chapter.");
            return;
        }
        i + 1
    } else if now - list[i].at < 3.0 {
        i.saturating_sub(1)
    } else {
        i
    };
    if let Some(p) = &s.player {
        p.seek(list[to].at);
    }
    say(
        hwnd,
        &format!(
            "Chapter {} of {}, {}, at {}.",
            to + 1,
            list.len(),
            list[to].name,
            clock(list[to].at)
        ),
    );
    show_where(hwnd, s);
}

/// Goes on to the next or the previous in the list, if there is one, in this
/// same window.
fn go(hwnd: HWND, s: &mut State, next: bool) {
    let there = if next {
        s.listen.track.has_next
    } else {
        s.listen.track.has_previous
    };
    let found = if there {
        s.listen.step.as_mut().and_then(|step| step(next))
    } else {
        None
    };
    let Some(track) = found else {
        say(
            hwnd,
            if next {
                "This is the last one in the list."
            } else {
                "This is the first one in the list."
            },
        );
        return;
    };
    keep_place(s);
    // Stopped at once: the old sound must not run on under the new name.
    s.player = None;
    s.listen.track = track;
    s.chapters = chapters_of(s.listen.track.video.clone());
    s.stepped = true;
    s.whole = false;
    title(hwnd, s);
    open_ready(hwnd, s);
}

/// The window's title: what is playing.
fn title(hwnd: HWND, s: &State) {
    let t = wide(&format!("Playing {} - VidFetch", s.listen.track.name));
    unsafe {
        let _ = SetWindowTextW(hwnd, PCWSTR(t.as_ptr()));
    }
}

/// Opens the player when the sound is ready; when it is still coming, says
/// the name, and the timer opens it once it comes.
fn open_ready(hwnd: HWND, s: &mut State) {
    let source = match &s.listen.track.source {
        Source::Ready(src) => src.clone(),
        Source::Coming(_) => {
            if s.stepped {
                say(hwnd, &format!("{}.", s.listen.track.name));
                // Said once: when it comes, only where it carries on from.
                s.stepped = false;
            }
            show_where(hwnd, s);
            return;
        }
    };
    match Player::open(&source, s.listen.speed, s.listen.cut_silence) {
        Ok(p) => {
            s.player = Some(p);
            start(hwnd, s);
            s.stepped = false;
        }
        Err(why) => failed(hwnd, s, &why),
    }
}

/// Says why the one chosen cannot play, and shows it in the Where playing
/// is box to read again.
fn failed(hwnd: HWND, s: &mut State, why: &str) {
    s.player = None;
    let text = format!("{} could not be played: {why}", s.listen.track.name);
    set_text(hwnd, IDC_L_WHERE, &text);
    s.shown = text.clone();
    let next = if s.listen.track.has_next {
        " Page Down goes to the next; Escape closes."
    } else {
        " Escape closes."
    };
    say(hwnd, &format!("{text}{next}"));
}

/// Whether a stream that failed can be fetched whole instead, once: YouTube
/// sometimes refuses an address it gave a moment ago (milestone 7 test, 1.3).
fn can_fetch(s: &State) -> bool {
    !s.whole
        && s.listen.track.page.is_some()
        && matches!(&s.listen.track.source, Source::Ready(a) if a.starts_with("http"))
}

/// The sound came, or a source gave none: checked each tick.
fn tick(hwnd: HWND, s: &mut State) {
    let came = match &s.listen.track.source {
        Source::Coming(rx) => match rx.try_recv() {
            Ok(r) => Some(r),
            Err(TryRecvError::Empty) => None,
            Err(TryRecvError::Disconnected) => Some(Err("it was not found.".into())),
        },
        Source::Ready(_) => None,
    };
    match came {
        Some(Ok(address)) => {
            let source =
                std::mem::replace(&mut s.listen.track.source, Source::Ready(address.clone()));
            match Player::open(&address, s.listen.speed, s.listen.cut_silence) {
                Ok(p) => {
                    // Started as one that came, so the name is not said again.
                    s.listen.track.source = source;
                    s.player = Some(p);
                    start(hwnd, s);
                    s.listen.track.source = Source::Ready(address);
                }
                Err(_) if can_fetch(s) => fetch_whole(hwnd, s),
                Err(why) => failed(hwnd, s, &why),
            }
            return;
        }
        Some(Err(why)) => {
            s.listen.track.source = Source::Ready(String::new());
            failed(hwnd, s, &why);
            return;
        }
        None => {}
    }
    let Some(p) = &s.player else { return };
    if let Some(why) = p.take_failure() {
        if (why == NO_SOUND && !s.whole) || can_fetch(s) {
            fetch_whole(hwnd, s);
        } else {
            failed(hwnd, s, &why);
        }
        return;
    }
    if p.take_ended() {
        if s.listen.play_next && s.listen.track.has_next {
            go(hwnd, s, true);
            return;
        }
        say(
            hwnd,
            if s.listen.track.has_next {
                "Reached the end. Page Down plays the next one; Play or pause starts again; \
                 Escape closes."
            } else {
                "Reached the end. Play or pause starts again from the beginning; Escape closes."
            },
        );
    }
    show_where(hwnd, s);
}

/// A sound Media Foundation reads nothing from, because of its edit list:
/// fetched whole, or copied, into VidFetch's own folder without one, and
/// played from there.
fn fetch_whole(hwnd: HWND, s: &mut State) {
    let Source::Ready(source) = &s.listen.track.source else {
        return;
    };
    let source = source.clone();
    let page = s.listen.track.page.clone();
    s.player = None;
    s.whole = true;
    say(
        hwnd,
        "This one would not stream, so its sound is being fetched first. That can take a little \
         while for a long video. Please wait.",
    );
    let name: String = s
        .listen
        .track
        .key
        .chars()
        .map(|c| if c.is_ascii_alphanumeric() { c } else { '_' })
        .rev()
        .take(40)
        .collect();
    let (tx, rx) = std::sync::mpsc::channel();
    std::thread::spawn(move || {
        let _ = tx.send(whole_copy(&source, page.as_deref(), &name));
    });
    s.listen.track.source = Source::Coming(rx);
    show_where(hwnd, s);
}

/// The sound of `page`, or the file `source`, saved whole without its edit
/// list; the copy's path. Kept with the downloads in progress, so playing it
/// again today needs no fetching.
fn whole_copy(source: &str, page: Option<&str>, name: &str) -> Result<String, String> {
    let dir = crate::ytdlp::local_dir().join("downloads");
    let _ = std::fs::create_dir_all(&dir);
    let to: PathBuf = dir.join(format!("listen-{name}.m4a"));
    if to.is_file() {
        return Ok(to.display().to_string());
    }
    let part = to.with_extension("part.m4a");
    if let Some(page) = page {
        let exe = crate::ytdlp::find().ok_or("yt-dlp is not on this computer.")?;
        let stop = std::sync::atomic::AtomicBool::new(false);
        crate::ytdlp::fetch_sound(&exe, page, &part, &stop)?;
        crate::ytdlp::drop_edit_list(&part)?;
        std::fs::rename(&part, &to).map_err(|e| format!("it could not be saved: {e}"))?;
        return Ok(to.display().to_string());
    } else {
        std::fs::copy(source, &to).map_err(|e| format!("it could not be copied: {e}"))?;
    }
    crate::ytdlp::drop_edit_list(&to)?;
    Ok(to.display().to_string())
}

fn close(hwnd: HWND, s: &mut State) {
    unsafe {
        let _ = KillTimer(Some(hwnd), TIMER);
        if let Some(p) = &s.player {
            p.pause();
        }
        let _ = EndDialog(hwnd, 1);
    }
}

/// The keys of the window, anywhere in it except the Where playing is box,
/// whose text the arrows read. A dialog's own buttons do nothing with them,
/// so nothing is lost.
unsafe extern "system" fn keys(code: i32, wparam: WPARAM, lparam: LPARAM) -> LRESULT {
    if code == MSGF_DIALOGBOX as i32 {
        let msg = &*(lparam.0 as *const MSG);
        if let Some(dialog) = OPEN.with(|o| o.get()) {
            let ours = msg.hwnd == dialog || IsChild(dialog, msg.hwnd).as_bool();
            let in_box = [IDC_L_WHERE, IDC_L_KEYS]
                .iter()
                .any(|&id| GetDlgItem(Some(dialog), id).is_ok_and(|c| c == msg.hwnd));
            let key = msg.wParam.0 as u16;
            if msg.message == WM_KEYDOWN && ours && !in_box && handle(dialog, key) {
                return LRESULT(1);
            }
        }
    }
    CallNextHookEx(None, code, wparam, lparam)
}

/// A key pressed in the window; true if it was one of ours.
unsafe fn handle(dialog: HWND, key: u16) -> bool {
    let down = |k: u16| GetKeyState(k as i32) < 0;
    let (shift, ctrl, alt) = (down(VK_SHIFT.0), down(VK_CONTROL.0), down(VK_MENU.0));
    if alt {
        return false;
    }
    let s = state(dialog);
    let wait = |s: &State| {
        if s.player.is_none() {
            say(dialog, &waiting(s));
            true
        } else {
            false
        }
    };
    match key {
        k if k == VK_LEFT.0 && !shift && !ctrl => skip(dialog, s, -STEP),
        k if k == VK_RIGHT.0 && !shift && !ctrl => skip(dialog, s, STEP),
        k if k == VK_T && !shift && !ctrl => {
            if !wait(s) {
                let p = s.player.as_ref().unwrap();
                let at = where_text(p.at(), p.secs());
                let chapter = chapter_words(s)
                    .map(|c| format!(", {c}"))
                    .unwrap_or_default();
                say(dialog, &format!("{at}, {}{chapter}.", how(p)));
            }
        }
        k if k == VK_HOME.0 && !shift && !ctrl => {
            if !wait(s) {
                // Said even while playing: it is a long way to go.
                s.player.as_ref().unwrap().seek(0.0);
                say(dialog, "At the start.");
                show_where(dialog, s);
            }
        }
        k if k == VK_END.0 && !shift && !ctrl => {
            if !wait(s) {
                let p = s.player.as_ref().unwrap();
                if p.secs() > 0.0 {
                    let to = p.seek(p.secs() - 10.0);
                    say(dialog, &format!("10 seconds from the end, {}.", clock(to)));
                }
                show_where(dialog, s);
            }
        }
        k if (k == VK_UP.0 || k == VK_DOWN.0) && shift && !ctrl => {
            let by = if k == VK_UP.0 {
                SPEED_STEP
            } else {
                -SPEED_STEP
            };
            let now = s.listen.speed;
            let new = match s.player.as_mut() {
                Some(p) => p.set_speed(now + by),
                None => {
                    let v = ((now + by) / SPEED_STEP).round() * SPEED_STEP;
                    v.clamp(crate::player::SLOWEST, crate::player::FASTEST)
                }
            };
            s.listen.speed = new;
            let said = if (new - now).abs() < 0.01 {
                format!(
                    "Already at {}, as {} as it goes.",
                    speed_words(new),
                    if by > 0.0 { "fast" } else { "slow" }
                )
            } else {
                format!("{}.", capital(&speed_words(new)))
            };
            say(dialog, &said);
            show_where(dialog, s);
        }
        k if (k == VK_UP.0 || k == VK_DOWN.0) && !shift && !ctrl => {
            if !wait(s) {
                let by = if k == VK_UP.0 { LOUDER } else { -LOUDER };
                let p = s.player.as_ref().unwrap();
                let v = p.set_volume(p.volume() as i32 + by);
                say(dialog, &format!("Volume {v} percent."));
            }
        }
        k if k == VK_NEXT.0 && ctrl && !shift => chapter(dialog, s, true),
        k if k == VK_PRIOR.0 && ctrl && !shift => chapter(dialog, s, false),
        k if k == VK_NEXT.0 && !shift && !ctrl => go(dialog, s, true),
        k if k == VK_PRIOR.0 && !shift && !ctrl => go(dialog, s, false),
        k if k == VK_D && ctrl && !shift => {
            let said = match s.listen.track.download.as_mut() {
                Some(d) => d(),
                None => "This is a download already.".to_string(),
            };
            say(dialog, &said);
        }
        _ => return false,
    }
    true
}

fn capital(s: &str) -> String {
    let mut c = s.chars();
    c.next()
        .map(|f| f.to_uppercase().collect::<String>() + c.as_str())
        .unwrap_or_default()
}

unsafe extern "system" fn proc(hwnd: HWND, message: u32, wparam: WPARAM, lparam: LPARAM) -> isize {
    match message {
        WM_INITDIALOG => {
            SetWindowLongPtrW(hwnd, GWLP_USERDATA, lparam.0);
            OPEN.with(|o| o.set(Some(hwnd)));
            let s = state(hwnd);
            title(hwnd, s);
            set_text(hwnd, IDC_L_KEYS, KEYS);
            SetTimer(Some(hwnd), TIMER, TICK_MS, None);
            SetTimer(Some(hwnd), HUSH_TIMER, HUSH_MS, None);
            focus(hwnd, IDC_L_PLAY);
            if s.player.is_some() {
                start(hwnd, s);
            } else {
                show_where(hwnd, s);
            }
            0
        }
        WM_TIMER => {
            if wparam.0 == HUSH_TIMER {
                hush_opening(hwnd, state(hwnd));
            } else {
                tick(hwnd, state(hwnd));
            }
            0
        }
        WM_COMMAND => {
            let id = (wparam.0 & 0xFFFF) as i32;
            let s = state(hwnd);
            match id {
                IDC_L_PLAY => toggle(hwnd, s),
                IDC_L_BACK => skip(hwnd, s, -SKIP),
                IDC_L_FORWARD => skip(hwnd, s, SKIP),
                _ if id == IDCANCEL.0 => {
                    close(hwnd, s);
                    return 1;
                }
                _ => {}
            }
            0
        }
        _ => 0,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn where_playing_is_said_in_whole_seconds() {
        assert_eq!(where_text(83.7, 311.6), "1:23 of 5:11");
        assert_eq!(where_text(0.0, 3725.0), "0:00 of 1:02:05");
        assert_eq!(where_text(61.0, 0.0), "1:01");
    }

    #[test]
    fn speeds_are_said_plainly() {
        assert_eq!(speed_words(1.0), "normal speed");
        assert_eq!(speed_words(1.25), "1.25 times as fast");
        assert_eq!(speed_words(2.0), "2 times as fast");
        assert_eq!(speed_words(0.75), "0.75 times, slower");
    }
}
