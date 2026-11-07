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
//! holds the same, for reading at any time.
//!
//! Added for VidFetch (log entry 3, ideas 1 to 4): it carries on where the
//! user stopped last time; Up and Down set the volume, Shift with them the
//! speed; Page Down and Page Up go to the next or previous in the list, as
//! does the end of one when the user asked for that. A YouTube video's
//! chapters, fetched as it starts, are Ctrl with Page Down and Page Up.

use std::cell::Cell;
use std::sync::{Arc, Mutex};

use windows::core::PCWSTR;
use windows::Win32::Foundation::{HWND, LPARAM, LRESULT, WPARAM};
use windows::Win32::System::LibraryLoader::GetModuleHandleW;
use windows::Win32::System::Threading::GetCurrentThreadId;
use windows::Win32::UI::Input::KeyboardAndMouse::{
    GetFocus, GetKeyState, VK_CONTROL, VK_DOWN, VK_END, VK_HOME, VK_LEFT, VK_MENU, VK_NEXT,
    VK_PRIOR, VK_RIGHT, VK_SHIFT, VK_UP,
};
use windows::Win32::UI::WindowsAndMessaging::*;

use crate::chapters::{self, Chapter};
use crate::player::{Player, SPEED_STEP};
use crate::ui::{focus, set_text};
use crate::{places, wide};

const IDD_PLAY: u16 = 110;
const IDC_L_WHERE: i32 = 1060;
const IDC_L_PLAY: i32 = 1061;
const IDC_L_BACK: i32 = 1062;
const IDC_L_FORWARD: i32 = 1063;

/// Seconds a button skips, and an arrow key.
const SKIP: f64 = 10.0;
const STEP: f64 = 2.0;
/// Percent of volume an arrow key moves.
const LOUDER: i32 = 10;
/// The key that says where playing is, and the one that downloads.
const VK_T: u16 = b'T' as u16;
const VK_D: u16 = b'D' as u16;

/// How often the window checks on the player: for the end, and to keep the
/// Where playing is box current.
const TICK_MS: u32 = 250;
const TIMER: usize = 1;

/// How the window was left.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Outcome {
    Closed,
    Next,
    Previous,
}

/// What to play, and what the window may do besides.
pub struct Listen<'a> {
    /// A file, or a web address.
    pub source: &'a str,
    /// What is playing, for the title.
    pub name: &'a str,
    /// Where it is remembered (places.rs).
    pub key: String,
    /// Playing speed, kept between windows.
    pub speed: f64,
    pub has_next: bool,
    pub has_previous: bool,
    /// At the end, go on to the next.
    pub play_next: bool,
    /// Ctrl+D: downloads what is playing, if it is online; what to say.
    pub download: Option<&'a mut dyn FnMut() -> String>,
    /// The YouTube video it is, if known, for its chapters.
    pub video: Option<String>,
}

/// A video's chapters: None while they are being fetched.
type Chapters = Arc<Mutex<Option<Vec<Chapter>>>>;

struct State<'a, 'b> {
    player: Player,
    listen: &'a mut Listen<'b>,
    /// What the Where playing is box last said, to change it only when needed.
    shown: String,
    /// Where to carry on from, if the user stopped there before.
    from: Option<f64>,
    outcome: Outcome,
    chapters: Chapters,
}

thread_local! {
    /// The open window, for the keyboard hook.
    static OPEN: Cell<Option<HWND>> = const { Cell::new(None) };
}

/// Opens the window and plays, from where the user stopped last time if
/// they did. How it was left, or why it could not play.
pub fn show(owner: HWND, listen: &mut Listen) -> Result<Outcome, String> {
    let player = Player::open(listen.source, listen.speed)?;
    let from = places::get(&listen.key).filter(|&at| player.secs() <= 0.0 || at < player.secs());
    let chapters: Chapters = Arc::new(Mutex::new(None));
    match listen.video.clone() {
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
    let mut state = State {
        player,
        listen,
        shown: String::new(),
        from,
        outcome: Outcome::Closed,
        chapters,
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
    let s = &mut state;
    let at = if s.player.finished() {
        0.0
    } else {
        s.player.at()
    };
    places::set(&s.listen.key, at, s.player.secs());
    s.listen.speed = s.player.speed();
    Ok(state.outcome)
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

fn how(s: &State) -> String {
    let playing = if s.player.playing() {
        "playing"
    } else {
        "paused"
    };
    if (s.player.speed() - 1.0).abs() < 0.01 {
        playing.to_string()
    } else {
        format!("{playing}, {}", speed_words(s.player.speed()))
    }
}

/// Keeps the Where playing is box current, but not while it has focus, so
/// a screen reader reading it is not interrupted.
fn show_where(hwnd: HWND, s: &mut State, even_with_focus: bool) {
    let text = format!("{}, {}", where_text(s.player.at(), s.player.secs()), how(s));
    if text == s.shown {
        return;
    }
    let has_focus = unsafe { GetDlgItem(Some(hwnd), IDC_L_WHERE).is_ok_and(|c| GetFocus() == c) };
    if has_focus && !even_with_focus {
        return;
    }
    set_text(hwnd, IDC_L_WHERE, &text);
    s.shown = text;
}

fn toggle(hwnd: HWND, s: &mut State) {
    let at = where_text(s.player.at(), s.player.secs());
    if s.player.playing() {
        s.player.pause();
        say(hwnd, &format!("Paused at {at}."));
    } else {
        s.player.play();
        say(hwnd, &format!("Playing from {at}."));
    }
    show_where(hwnd, s, false);
}

/// Moves by `by` seconds. Paused, says the time it moved to; playing,
/// nothing, as the sound says where it is.
fn skip(hwnd: HWND, s: &mut State, by: f64) {
    let to = s.player.seek(s.player.at() + by);
    if !s.player.playing() {
        say(hwnd, &clock(to));
    }
    show_where(hwnd, s, false);
}

/// The chapter playing, as said: "chapter 2 of 5, The plan".
fn chapter_words(s: &State) -> Option<String> {
    let all = s.chapters.lock().unwrap();
    let list = all.as_ref()?;
    let i = chapters::at(list, s.player.at())?;
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
            if s.listen.video.is_some() {
                "This video has no chapters."
            } else {
                "VidFetch does not know which YouTube video this is, so it has no chapters."
            },
        );
        return;
    }
    let now = s.player.at();
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
    s.player.seek(list[to].at);
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
    show_where(hwnd, s, false);
}

/// Leaves for the next or the previous in the list, if there is one.
fn go(hwnd: HWND, s: &mut State, next: bool) {
    let there = if next {
        s.listen.has_next
    } else {
        s.listen.has_previous
    };
    if !there {
        say(
            hwnd,
            if next {
                "This is the last one in the list."
            } else {
                "This is the first one in the list."
            },
        );
        return;
    }
    s.outcome = if next {
        Outcome::Next
    } else {
        Outcome::Previous
    };
    close(hwnd, s);
}

fn close(hwnd: HWND, s: &mut State) {
    unsafe {
        let _ = KillTimer(Some(hwnd), TIMER);
        s.player.pause();
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
            let in_box = GetDlgItem(Some(dialog), IDC_L_WHERE).is_ok_and(|c| c == msg.hwnd);
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
    match key {
        k if k == VK_LEFT.0 && !shift && !ctrl => skip(dialog, s, -STEP),
        k if k == VK_RIGHT.0 && !shift && !ctrl => skip(dialog, s, STEP),
        k if k == VK_T && !shift && !ctrl => {
            let at = where_text(s.player.at(), s.player.secs());
            let chapter = chapter_words(s)
                .map(|c| format!(", {c}"))
                .unwrap_or_default();
            say(dialog, &format!("{at}, {}{chapter}.", how(s)));
        }
        k if k == VK_HOME.0 && !shift && !ctrl => {
            // Said even while playing: it is a long way to go.
            s.player.seek(0.0);
            say(dialog, "At the start.");
            show_where(dialog, s, false);
        }
        k if k == VK_END.0 && !shift && !ctrl && s.player.secs() > 0.0 => {
            let to = s.player.seek(s.player.secs() - 10.0);
            say(dialog, &format!("10 seconds from the end, {}.", clock(to)));
            show_where(dialog, s, false);
        }
        k if (k == VK_UP.0 || k == VK_DOWN.0) && shift && !ctrl => {
            let by = if k == VK_UP.0 {
                SPEED_STEP
            } else {
                -SPEED_STEP
            };
            let now = s.player.speed();
            let new = s.player.set_speed(now + by);
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
            show_where(dialog, s, false);
        }
        k if (k == VK_UP.0 || k == VK_DOWN.0) && !shift && !ctrl => {
            let by = if k == VK_UP.0 { LOUDER } else { -LOUDER };
            let v = s.player.set_volume(s.player.volume() as i32 + by);
            say(dialog, &format!("Volume {v} percent."));
        }
        k if k == VK_NEXT.0 && ctrl && !shift => chapter(dialog, s, true),
        k if k == VK_PRIOR.0 && ctrl && !shift => chapter(dialog, s, false),
        k if k == VK_NEXT.0 && !shift && !ctrl => go(dialog, s, true),
        k if k == VK_PRIOR.0 && !shift && !ctrl => go(dialog, s, false),
        k if k == VK_D && ctrl && !shift => {
            let said = match s.listen.download.as_mut() {
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
            let title = wide(&format!("Playing {} - VidFetch", s.listen.name));
            let _ = SetWindowTextW(hwnd, PCWSTR(title.as_ptr()));
            if let Some(at) = s.from {
                s.player.seek(at);
            }
            s.player.play();
            show_where(hwnd, s, true);
            SetTimer(Some(hwnd), TIMER, TICK_MS, None);
            focus(hwnd, IDC_L_PLAY);
            let long = if s.player.secs() > 0.0 {
                format!(" {} long.", clock(s.player.secs()))
            } else {
                String::new()
            };
            let speed = if (s.player.speed() - 1.0).abs() < 0.01 {
                String::new()
            } else {
                format!(" {}.", capital(&speed_words(s.player.speed())))
            };
            let said = match s.from {
                Some(at) => format!(
                    "Carrying on from {}, where you stopped.{long}{speed} Home goes to the start.",
                    clock(at)
                ),
                None => format!("Playing from the start.{long}{speed}"),
            };
            say(hwnd, &said);
            0
        }
        WM_TIMER => {
            let s = state(hwnd);
            if let Some(why) = s.player.take_failure() {
                say(hwnd, &format!("Playing stopped: {why}"));
            } else if s.player.take_ended() {
                if s.listen.play_next && s.listen.has_next {
                    s.outcome = Outcome::Next;
                    close(hwnd, s);
                    return 0;
                }
                say(
                    hwnd,
                    if s.listen.has_next {
                        "Reached the end. Page Down plays the next one; Play or pause starts \
                         again; Escape closes."
                    } else {
                        "Reached the end. Play or pause starts again from the beginning; Escape \
                         closes."
                    },
                );
            }
            show_where(hwnd, s, false);
            0
        }
        WM_COMMAND => {
            let id = (wparam.0 & 0xFFFF) as i32;
            let code = ((wparam.0 >> 16) & 0xFFFF) as u32;
            let s = state(hwnd);
            match id {
                IDC_L_PLAY => toggle(hwnd, s),
                IDC_L_BACK => skip(hwnd, s, -SKIP),
                IDC_L_FORWARD => skip(hwnd, s, SKIP),
                // Current as it gets focus (EN_SETFOCUS).
                IDC_L_WHERE if code == 0x0100 => show_where(hwnd, s, true),
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
