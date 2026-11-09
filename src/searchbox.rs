//! The Search YouTube window: what to look for, and how (log entry 6, B1).
//! Every box says what it is by itself; the length and Shorts apply to
//! videos, and their labels say so.

use windows::core::PCWSTR;
use windows::Win32::Foundation::{HWND, LPARAM, WPARAM};
use windows::Win32::System::LibraryLoader::GetModuleHandleW;
use windows::Win32::UI::WindowsAndMessaging::*;

use crate::message;
use crate::ui::{focus, get_text, set_text};
use crate::youtube::{Query, SearchFor};

const BM_GETCHECK: u32 = 0x00F0;
const BM_SETCHECK: u32 = 0x00F1;

const IDD_SEARCH: u16 = 112;
const IDC_S_TEXT: i32 = 1130;
const IDC_S_WHAT: i32 = 1131;
const IDC_S_SORT: i32 = 1132;
const IDC_S_LENGTH: i32 = 1133;
const IDC_S_SHORTS: i32 = 1134;

const WHAT: [(&str, SearchFor); 3] = [
    ("Videos", SearchFor::Videos),
    ("Channels", SearchFor::Channels),
    ("Playlists", SearchFor::Playlists),
];
const SORT: [&str; 2] = ["Most relevant first", "Newest first"];
/// Each length's number in YouTube's filter.
const LENGTH: [(&str, u8); 4] = [
    ("Any length", 0),
    ("Under 4 minutes", 1),
    ("4 to 20 minutes", 3),
    ("Over 20 minutes", 2),
];

/// What to search for, starting from `last`; None if the user cancelled.
pub fn ask(owner: HWND, last: &Query) -> Option<Query> {
    let mut query = last.clone();
    let ok = unsafe {
        let instance = GetModuleHandleW(None).ok()?;
        DialogBoxParamW(
            Some(instance.into()),
            PCWSTR(IDD_SEARCH as usize as *const u16),
            Some(owner),
            Some(proc),
            LPARAM(&mut query as *mut Query as isize),
        ) == IDOK.0 as isize
    };
    ok.then_some(query)
}

/// Fills a box of choices and chooses one.
unsafe fn fill(hwnd: HWND, id: i32, items: &[&str], at: usize) {
    let Ok(combo) = GetDlgItem(Some(hwnd), id) else {
        return;
    };
    for item in items {
        let w = crate::wide(item);
        SendMessageW(combo, CB_ADDSTRING, None, Some(LPARAM(w.as_ptr() as isize)));
    }
    SendMessageW(combo, CB_SETCURSEL, Some(WPARAM(at)), None);
}

unsafe fn chosen(hwnd: HWND, id: i32) -> usize {
    let Ok(combo) = GetDlgItem(Some(hwnd), id) else {
        return 0;
    };
    SendMessageW(combo, CB_GETCURSEL, None, None).0.max(0) as usize
}

unsafe extern "system" fn proc(hwnd: HWND, message: u32, wparam: WPARAM, lparam: LPARAM) -> isize {
    match message {
        WM_INITDIALOG => {
            SetWindowLongPtrW(hwnd, GWLP_USERDATA, lparam.0);
            let q = &*(lparam.0 as *const Query);
            set_text(hwnd, IDC_S_TEXT, &q.text);
            let names: Vec<&str> = WHAT.iter().map(|w| w.0).collect();
            fill(
                hwnd,
                IDC_S_WHAT,
                &names,
                WHAT.iter().position(|w| w.1 == q.what).unwrap_or(0),
            );
            fill(hwnd, IDC_S_SORT, &SORT, q.newest as usize);
            let names: Vec<&str> = LENGTH.iter().map(|l| l.0).collect();
            fill(
                hwnd,
                IDC_S_LENGTH,
                &names,
                LENGTH.iter().position(|l| l.1 == q.length).unwrap_or(0),
            );
            if let Ok(check) = GetDlgItem(Some(hwnd), IDC_S_SHORTS) {
                SendMessageW(check, BM_SETCHECK, Some(WPARAM(q.no_shorts as usize)), None);
            }
            focus(hwnd, IDC_S_TEXT);
            SendDlgItemMessageW(hwnd, IDC_S_TEXT, 0x00B1, WPARAM(0), LPARAM(-1));
            0
        }
        WM_COMMAND => {
            let id = (wparam.0 & 0xFFFF) as i32;
            if id == IDCANCEL.0 {
                let _ = EndDialog(hwnd, IDCANCEL.0 as isize);
                return 1;
            }
            if id != IDOK.0 {
                return 0;
            }
            let text = get_text(hwnd, IDC_S_TEXT).trim().to_string();
            if text.is_empty() {
                message::problem(hwnd, "Type what to search for, such as Blender open movie.");
                focus(hwnd, IDC_S_TEXT);
                return 1;
            }
            let q = &mut *(GetWindowLongPtrW(hwnd, GWLP_USERDATA) as *mut Query);
            q.text = text;
            q.what = WHAT[chosen(hwnd, IDC_S_WHAT).min(2)].1;
            q.newest = chosen(hwnd, IDC_S_SORT) == 1;
            q.length = LENGTH[chosen(hwnd, IDC_S_LENGTH).min(3)].1;
            q.no_shorts = GetDlgItem(Some(hwnd), IDC_S_SHORTS)
                .map(|c| SendMessageW(c, BM_GETCHECK, None, None).0 == 1)
                .unwrap_or(false);
            let _ = EndDialog(hwnd, IDOK.0 as isize);
            1
        }
        _ => 0,
    }
}
