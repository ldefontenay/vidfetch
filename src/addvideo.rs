//! The New download window (File, New download, Ctrl+N): a web address, how
//! to save it, and what will happen. Video Scribe's Add a video window, with
//! the choices that matter for a download in it rather than only in the
//! Options menu, so they can be changed for the download about to start.
//!
//! The Category box says where the download goes, the category the list
//! shows chosen to begin with (log entry 3, Q7). "Follow the channel instead"
//! follows the channel of the address, a channel's own or a video's (Q4).

use windows::core::PCWSTR;
use windows::Win32::Foundation::{HGLOBAL, HWND, LPARAM, WPARAM};
use windows::Win32::System::DataExchange::{CloseClipboard, GetClipboardData, OpenClipboard};
use windows::Win32::System::LibraryLoader::GetModuleHandleW;
use windows::Win32::System::Memory::{GlobalLock, GlobalUnlock};
use windows::Win32::UI::WindowsAndMessaging::*;

use crate::config::{Config, Format, FORMATS, QUALITIES};
use crate::message;
use crate::ui::{caret_to_start, focus, get_text, set_text};
use crate::{wide, ytdlp};

const IDD_ADD: u16 = 108;
const IDC_A_SOURCE: i32 = 1090;
const IDC_A_PLAN: i32 = 1092;
const IDC_A_FORMAT: i32 = 1093;
const IDC_A_QUALITY: i32 = 1094;
const IDC_A_CAPTIONS: i32 = 1095;
const IDC_A_FOLLOW: i32 = 1096;
const IDC_A_CATEGORY: i32 = 1097;

const EN_SETFOCUS: u32 = 0x0100;
const EN_CHANGE: u32 = 0x0300;
const CBN_SELCHANGE: u32 = 1;
const EM_SETSEL: u32 = 0x00B1;
const BM_GETCHECK: u32 = 0x00F0;
const BM_SETCHECK: u32 = 0x00F1;
const CF_UNICODETEXT: u32 = 13;

/// What the user chose to do with the address.
pub enum Chosen {
    /// Download it into the category named.
    Download { url: String, category: String },
    /// Follow the channel the address belongs to.
    Follow(String),
}

struct State<'a> {
    cfg: &'a mut Config,
    /// What the address box starts with.
    text: String,
    /// A download is running, so this one will wait its turn.
    busy: bool,
    /// Uncategorised, then the user's categories; and the one chosen.
    categories: Vec<String>,
    category: usize,
    chosen: Option<Chosen>,
}

/// Asks for an address, starting with `text`, or with a web address on the
/// clipboard when `text` is empty. What to do with it, or None when the user
/// cancelled. Choices changed here are kept in `cfg`, except the category,
/// which is for this download. `busy`: a download is running, so this one
/// will go in the queue. `categories`: the user's own.
pub fn show(
    owner: HWND,
    cfg: &mut Config,
    text: &str,
    busy: bool,
    categories: &[String],
) -> Option<Chosen> {
    let text = if text.is_empty() {
        clipboard()
            .filter(|c| ytdlp::parse_address(c).is_ok() && !c.contains('\n'))
            .unwrap_or_default()
    } else {
        text.to_string()
    };
    let all: Vec<String> = std::iter::once(crate::library::UNCATEGORISED.to_string())
        .chain(categories.iter().cloned())
        .collect();
    let category = cfg
        .category
        .as_deref()
        .and_then(|c| all.iter().position(|x| x.eq_ignore_ascii_case(c)))
        .unwrap_or(0);
    let mut state = State {
        cfg,
        text,
        busy,
        categories: all,
        category,
        chosen: None,
    };
    unsafe {
        let instance = GetModuleHandleW(None).ok()?;
        DialogBoxParamW(
            Some(instance.into()),
            PCWSTR(IDD_ADD as usize as *const u16),
            Some(owner),
            Some(proc),
            LPARAM(&mut state as *mut State as isize),
        );
    }
    state.chosen
}

/// The text on the clipboard, if there is any.
pub fn clipboard() -> Option<String> {
    unsafe {
        OpenClipboard(None).ok()?;
        let text = (|| {
            let h = GetClipboardData(CF_UNICODETEXT).ok()?;
            let g = HGLOBAL(h.0);
            let p = GlobalLock(g) as *const u16;
            if p.is_null() {
                return None;
            }
            let mut n = 0;
            while *p.add(n) != 0 && n < 4096 {
                n += 1;
            }
            let s = String::from_utf16_lossy(std::slice::from_raw_parts(p, n));
            let _ = GlobalUnlock(g);
            Some(s.trim().to_string())
        })();
        let _ = CloseClipboard();
        text
    }
}

unsafe fn state<'a>(hwnd: HWND) -> Option<&'a mut State<'a>> {
    (GetWindowLongPtrW(hwnd, GWLP_USERDATA) as *mut State).as_mut()
}

/// What will happen with this address and these choices, saved in the
/// category `category`.
pub fn plan(cfg: &Config, address: Option<&str>, busy: bool, category: &str) -> String {
    let mut lines = Vec::new();
    match address {
        None => lines.push(
            "Paste the web address of a video or a playlist, such as one from YouTube, into the \
             web address box."
                .to_string(),
        ),
        Some(url) if crate::youtube::is_channel(url) => lines.push(
            "From YouTube: a channel. Start downloading fetches every one of its videos, into a \
             folder of its own; Follow the channel instead (Alt+F) keeps it in Your channels, \
             where Ctrl+L shows what is new."
                .to_string(),
        ),
        Some(url) => {
            let site = ytdlp::site(url);
            let list = ytdlp::list_id(url).filter(|l| !ytdlp::is_mix(l));
            let video = ytdlp::video_id(url);
            lines.push(match (ytdlp::is_youtube(url), video, list) {
                (true, Some(_), Some(_)) => format!(
                    "From {site}: a video in a playlist. When you start, VidFetch asks whether \
                     to download the whole playlist or just this video."
                ),
                (true, Some(_), None) => format!("From {site}: one video."),
                (true, None, _) => format!(
                    "From {site}: a playlist or a channel. Every video in it is downloaded, one \
                     after the other, into a folder of its own named after it. Downloading it \
                     again later fetches only the videos that are new."
                ),
                _ => format!(
                    "From {site}. If the address is a playlist or a channel, every video in it \
                     is downloaded, into a folder of its own named after it."
                ),
            });
        }
    }
    lines.push(match cfg.format {
        Format::Video => format!(
            "Saved as a video, an MP4, with the picture at most {} lines high.",
            cfg.lines
        ),
        Format::M4a => "Saved as the sound alone, an M4A, as the site sends it. Picture quality \
                        does not matter for sound."
            .to_string(),
        Format::Mp3 => "Saved as the sound alone, an MP3. Picture quality does not matter for \
                        sound."
            .to_string(),
    });
    lines.push(format!(
        "Saved in the category {category}, named after the title: the folder {}.",
        cfg.output_folder().join(category).display()
    ));
    if cfg.captions {
        lines.push(
            "Captions are saved beside it as an SRT file, when it has any: a person's if there \
             are some, else the site's automatic ones."
                .to_string(),
        );
    }
    lines.push(if busy {
        "Only the address goes to the site, through yt-dlp. Another download is running, so \
         pressing Enter puts this one in the queue, to start when its turn comes."
            .to_string()
    } else {
        "Only the address goes to the site, through yt-dlp. Press Enter to start.".to_string()
    });
    lines.join("\n\n")
}

fn update_plan(hwnd: HWND, s: &State) {
    let address = ytdlp::parse_address(&get_text(hwnd, IDC_A_SOURCE)).ok();
    let category = &s.categories[s.category];
    set_text(
        hwnd,
        IDC_A_PLAN,
        &plan(s.cfg, address.as_deref(), s.busy, category),
    );
}

unsafe fn fill(hwnd: HWND, id: i32, items: &[&str], at: usize) {
    for item in items {
        let t = wide(item);
        SendDlgItemMessageW(
            hwnd,
            id,
            CB_ADDSTRING,
            WPARAM(0),
            LPARAM(t.as_ptr() as isize),
        );
    }
    SendDlgItemMessageW(hwnd, id, CB_SETCURSEL, WPARAM(at), LPARAM(0));
}

unsafe fn combo_at(hwnd: HWND, id: i32) -> Option<usize> {
    usize::try_from(SendDlgItemMessageW(hwnd, id, CB_GETCURSEL, WPARAM(0), LPARAM(0)).0).ok()
}

unsafe extern "system" fn proc(hwnd: HWND, message: u32, wparam: WPARAM, lparam: LPARAM) -> isize {
    match message {
        WM_INITDIALOG => {
            SetWindowLongPtrW(hwnd, GWLP_USERDATA, lparam.0);
            if let Some(s) = state(hwnd) {
                let formats: Vec<&str> = FORMATS.iter().map(|f| f.1).collect();
                let at = FORMATS.iter().position(|f| f.0 == s.cfg.format);
                fill(hwnd, IDC_A_FORMAT, &formats, at.unwrap_or(0));
                let qualities: Vec<&str> = QUALITIES.iter().map(|q| q.1).collect();
                let at = QUALITIES.iter().position(|q| q.0 == s.cfg.lines);
                fill(hwnd, IDC_A_QUALITY, &qualities, at.unwrap_or(2));
                SendDlgItemMessageW(
                    hwnd,
                    IDC_A_CAPTIONS,
                    BM_SETCHECK,
                    WPARAM(s.cfg.captions as usize),
                    LPARAM(0),
                );
                let names: Vec<&str> = s.categories.iter().map(String::as_str).collect();
                fill(hwnd, IDC_A_CATEGORY, &names, s.category);
                if s.busy {
                    set_text(hwnd, IDOK.0, "&Start when its turn comes");
                }
                set_text(hwnd, IDC_A_SOURCE, &s.text);
                update_plan(hwnd, s);
            }
            focus(hwnd, IDC_A_SOURCE);
            // The whole address chosen, so typing or pasting replaces it.
            SendDlgItemMessageW(hwnd, IDC_A_SOURCE, EM_SETSEL, WPARAM(0), LPARAM(-1));
            0
        }
        WM_COMMAND => {
            let id = (wparam.0 & 0xFFFF) as i32;
            let code = ((wparam.0 >> 16) & 0xFFFF) as u32;
            let Some(s) = state(hwnd) else { return 0 };
            match (id, code) {
                (IDC_A_SOURCE, EN_CHANGE) => update_plan(hwnd, s),
                (IDC_A_CATEGORY, CBN_SELCHANGE) => {
                    if let Some(i) =
                        combo_at(hwnd, IDC_A_CATEGORY).filter(|&i| i < s.categories.len())
                    {
                        s.category = i;
                    }
                    update_plan(hwnd, s);
                }
                (IDC_A_FOLLOW, _) => match ytdlp::parse_address(&get_text(hwnd, IDC_A_SOURCE)) {
                    Ok(url) if ytdlp::is_youtube(&url) => {
                        s.chosen = Some(Chosen::Follow(url));
                        let _ = EndDialog(hwnd, IDOK.0 as isize);
                    }
                    Ok(url) => {
                        message::problem(
                            hwnd,
                            &format!(
                                "VidFetch follows YouTube channels only, and {url} is not on \
                                     YouTube. Copy the address of a YouTube channel, or of one of \
                                     its videos."
                            ),
                        );
                        focus(hwnd, IDC_A_SOURCE);
                    }
                    Err(why) => {
                        message::problem(hwnd, &why);
                        focus(hwnd, IDC_A_SOURCE);
                    }
                },
                (IDC_A_PLAN, EN_SETFOCUS) => caret_to_start(hwnd, IDC_A_PLAN),
                (IDC_A_FORMAT, CBN_SELCHANGE) => {
                    if let Some(f) = combo_at(hwnd, IDC_A_FORMAT).and_then(|i| FORMATS.get(i)) {
                        s.cfg.format = f.0;
                    }
                    update_plan(hwnd, s);
                }
                (IDC_A_QUALITY, CBN_SELCHANGE) => {
                    if let Some(q) = combo_at(hwnd, IDC_A_QUALITY).and_then(|i| QUALITIES.get(i)) {
                        s.cfg.lines = q.0;
                    }
                    update_plan(hwnd, s);
                }
                (IDC_A_CAPTIONS, _) => {
                    s.cfg.captions = SendDlgItemMessageW(
                        hwnd,
                        IDC_A_CAPTIONS,
                        BM_GETCHECK,
                        WPARAM(0),
                        LPARAM(0),
                    )
                    .0 == 1;
                    update_plan(hwnd, s);
                }
                (id, _) if id == IDOK.0 => {
                    match ytdlp::parse_address(&get_text(hwnd, IDC_A_SOURCE)) {
                        Ok(url) => {
                            s.chosen = Some(Chosen::Download {
                                url,
                                category: s.categories[s.category].clone(),
                            });
                            let _ = EndDialog(hwnd, IDOK.0 as isize);
                        }
                        Err(why) => {
                            message::problem(hwnd, &why);
                            focus(hwnd, IDC_A_SOURCE);
                        }
                    }
                }
                (id, _) if id == IDCANCEL.0 => {
                    let _ = EndDialog(hwnd, IDCANCEL.0 as isize);
                }
                _ => return 0,
            }
            1
        }
        _ => 0,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_plan_says_what_the_address_is() {
        let cfg = Config::default();
        let p = |u: &str| plan(&cfg, Some(u), false, "Uncategorised");
        assert!(p("https://youtu.be/_cMxraX_5RE").contains("one video"));
        assert!(p("https://www.youtube.com/watch?v=_cMxraX_5RE&list=PLx").contains("asks whether"));
        // A mix is not a playlist anyone made: the video alone.
        assert!(
            p("https://www.youtube.com/watch?v=_cMxraX_5RE&list=RD_cMxraX_5RE")
                .contains("one video")
        );
        assert!(p("https://www.youtube.com/playlist?list=PLx").contains("a playlist"));
        assert!(plan(&cfg, None, false, "Talks").contains("Paste"));
        assert!(p("https://www.youtube.com/@BlenderStudio").contains("Follow the channel"));
        assert!(p("https://youtu.be/_cMxraX_5RE").contains("sound alone"));
        let video = Config {
            format: Format::Video,
            captions: true,
            ..Config::default()
        };
        let said = plan(&video, Some("https://youtu.be/_cMxraX_5RE"), true, "Talks");
        assert!(said.contains("category Talks"));
        assert!(said.contains("720 lines") && said.contains("SRT") && said.contains("queue"));
    }
}
