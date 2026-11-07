//! The list of downloads in the main window, and what is done to one from
//! the list itself: opening it, renaming it, moving it to the Recycle Bin,
//! and its context menu. Video Scribe's videos.rs, after NoteFetch's list.

use std::path::Path;
use std::time::SystemTime;

use windows::core::{HSTRING, PCWSTR};
use windows::Win32::Foundation::{FILETIME, HWND, LPARAM, POINT, RECT, SYSTEMTIME, WPARAM};
use windows::Win32::Graphics::Gdi::ClientToScreen;
use windows::Win32::System::Time::{FileTimeToSystemTime, SystemTimeToTzSpecificLocalTime};
use windows::Win32::UI::Shell::{
    ILCreateFromPathW, ILFree, SHFileOperationW, SHOpenFolderAndSelectItems, ShellExecuteW,
    FOF_ALLOWUNDO, FOF_NOCONFIRMATION, FOF_NOERRORUI, FOF_SILENT, FO_DELETE, SHFILEOPSTRUCTW,
};
use windows::Win32::UI::WindowsAndMessaging::*;

use crate::library::{self, Entry};
use crate::message;
use crate::wide;

/// When, in words with the month in letters and no colon, as NoteFetch's
/// names have it: "1 Nov 2026, 2130".
pub fn when(t: SystemTime) -> String {
    let Ok(since) = t.duration_since(SystemTime::UNIX_EPOCH) else {
        return String::new();
    };
    // 100-nanosecond steps since 1601.
    let ticks = since.as_nanos() as u64 / 100 + 116_444_736_000_000_000;
    let ft = FILETIME {
        dwLowDateTime: ticks as u32,
        dwHighDateTime: (ticks >> 32) as u32,
    };
    let (mut utc, mut local) = (SYSTEMTIME::default(), SYSTEMTIME::default());
    unsafe {
        if FileTimeToSystemTime(&ft, &mut utc).is_err()
            || SystemTimeToTzSpecificLocalTime(None, &utc, &mut local).is_err()
        {
            return String::new();
        }
    }
    const MO: [&str; 12] = [
        "Jan", "Feb", "Mar", "Apr", "May", "Jun", "Jul", "Aug", "Sep", "Oct", "Nov", "Dec",
    ];
    format!(
        "{} {} {}, {:02}{:02}",
        local.wDay,
        MO[(local.wMonth.clamp(1, 12) - 1) as usize],
        local.wYear,
        local.wHour,
        local.wMinute
    )
}

pub fn item_text(e: &Entry) -> String {
    format!("{}: {}, {}", e.name, e.holds(), when(e.made))
}

/// Fills the list box `lb` with `entries`, choosing the one at `path`, else
/// the one at `at`, else the last.
pub fn fill(lb: HWND, entries: &[Entry], path: Option<&Path>, at: usize) {
    unsafe {
        SendMessageW(lb, LB_RESETCONTENT, None, None);
        for e in entries {
            let t = wide(&item_text(e));
            SendMessageW(lb, LB_ADDSTRING, None, Some(LPARAM(t.as_ptr() as isize)));
        }
        if entries.is_empty() {
            return;
        }
        let i = path
            .and_then(|p| entries.iter().position(|e| e.path == p))
            .unwrap_or(at.min(entries.len() - 1));
        SendMessageW(lb, LB_SETCURSEL, Some(WPARAM(i)), None);
    }
}

/// Which item of the list box is chosen.
pub fn chosen(lb: HWND) -> Option<usize> {
    usize::try_from(unsafe { SendMessageW(lb, LB_GETCURSEL, None, None) }.0).ok()
}

/// Opens a file in the program Windows plays it with, or a playlist's
/// folder in File Explorer. What to say.
pub fn open(owner: HWND, e: &Entry) -> String {
    let r = unsafe {
        ShellExecuteW(
            Some(owner),
            &HSTRING::from("open"),
            &HSTRING::from(e.path.as_os_str()),
            PCWSTR::null(),
            PCWSTR::null(),
            SW_SHOWNORMAL,
        )
    };
    if r.0 as isize <= 32 {
        message::problem(
            owner,
            &format!(
                "Windows could not open {}. If no program plays {} files on this computer, \
                 choose one in Windows Settings, Apps, Default apps.",
                e.name,
                e.path
                    .extension()
                    .unwrap_or_default()
                    .to_string_lossy()
                    .to_uppercase()
            ),
        );
        return "Not opened.".into();
    }
    if e.is_playlist() {
        format!("Opening the folder {} in File Explorer.", e.name)
    } else {
        format!("Opening {}.", e.name)
    }
}

/// File Explorer at the folder holding `path`, with `path` chosen in it.
pub fn show_in_explorer(path: &Path) {
    unsafe {
        let item = ILCreateFromPathW(&HSTRING::from(path.as_os_str()));
        if item.is_null() {
            open_folder(path.parent().unwrap_or(path));
            return;
        }
        if SHOpenFolderAndSelectItems(item, None, 0).is_err() {
            open_folder(path.parent().unwrap_or(path));
        }
        ILFree(Some(item));
    }
}

/// File Explorer at `folder`.
pub fn open_folder(folder: &Path) {
    unsafe {
        ShellExecuteW(
            None,
            &HSTRING::from("open"),
            &HSTRING::from(folder.as_os_str()),
            PCWSTR::null(),
            PCWSTR::null(),
            SW_SHOWNORMAL,
        );
    }
}

/// Asks for a new name, and renames the download. The new path and what to
/// say, or None when the user cancelled or was told why nothing changed.
pub fn rename(owner: HWND, e: &Entry) -> Option<(std::path::PathBuf, String)> {
    let (title, label) = if e.is_playlist() {
        (
            "Rename the playlist's folder - VidFetch",
            "&New name for the playlist's folder:",
        )
    } else {
        (
            "Rename the download - VidFetch",
            "&New name for the file, without its ending such as .mp4:",
        )
    };
    let typed = crate::namebox::ask(owner, title, label, &e.name, library::bad_name)?;
    match library::rename(e, &typed) {
        Ok(path) => Some((path, format!("Renamed to {typed}."))),
        Err(why) => {
            message::problem(owner, &why);
            None
        }
    }
}

/// Asks, then moves the download to the Recycle Bin. What to say.
pub fn delete(owner: HWND, e: &Entry) -> String {
    let what = if e.is_playlist() {
        format!(
            "Move the folder of the playlist {}, with everything in it, to the Recycle Bin? It \
             can be restored from there.",
            e.name
        )
    } else {
        format!(
            "Move {} to the Recycle Bin? It can be restored from there.",
            e.path.file_name().unwrap_or_default().to_string_lossy()
        )
    };
    if !message::ask(owner, &what, "&Delete", "&Keep it", false) {
        return "Nothing was deleted.".into();
    }
    // Each path ends with a null, and the list with another. Its captions go
    // with it.
    let mut from = wide(&e.path.display().to_string());
    if !e.is_playlist() {
        for c in library::captions_of(&e.path) {
            from.extend(wide(&c.display().to_string()));
        }
    }
    from.push(0);
    let mut op = SHFILEOPSTRUCTW {
        hwnd: owner,
        wFunc: FO_DELETE,
        pFrom: PCWSTR(from.as_ptr()),
        fFlags: (FOF_ALLOWUNDO | FOF_NOCONFIRMATION | FOF_SILENT | FOF_NOERRORUI).0 as u16,
        ..Default::default()
    };
    let result = unsafe { SHFileOperationW(&mut op) };
    if result != 0 || op.fAnyOperationsAborted.as_bool() || e.path.exists() {
        message::problem(
            owner,
            &format!(
                "{} could not be moved to the Recycle Bin. It may be open in another program, \
                 such as a media player: close it and try again.",
                e.name
            ),
        );
        return "Nothing was deleted.".into();
    }
    format!("{} was moved to the Recycle Bin.", e.name)
}

/// A context menu of `items` (a command and its words; 0 for a line), the
/// first in bold as what Enter does, at the mouse or, from the keyboard,
/// under the chosen download. The command chosen, or None.
pub fn context_menu(
    owner: HWND,
    lb: HWND,
    at_mouse: Option<(i32, i32)>,
    items: &[(i32, &str)],
) -> Option<i32> {
    unsafe {
        let menu = CreatePopupMenu().ok()?;
        for (id, text) in items {
            if *id == 0 {
                let _ = AppendMenuW(menu, MF_SEPARATOR, 0, PCWSTR::null());
            } else {
                let t = wide(text);
                let _ = AppendMenuW(menu, MF_STRING, *id as usize, PCWSTR(t.as_ptr()));
            }
        }
        let _ = SetMenuDefaultItem(menu, items[0].0 as u32, 0);
        let (x, y) = match at_mouse {
            Some(p) => p,
            None => {
                let mut r = RECT::default();
                let i = chosen(lb).unwrap_or(0);
                SendMessageW(
                    lb,
                    LB_GETITEMRECT,
                    Some(WPARAM(i)),
                    Some(LPARAM(&mut r as *mut _ as isize)),
                );
                let mut p = POINT {
                    x: r.left + 16,
                    y: r.bottom,
                };
                let _ = ClientToScreen(lb, &mut p);
                (p.x, p.y)
            }
        };
        let id = TrackPopupMenu(
            menu,
            TPM_RETURNCMD | TPM_RIGHTBUTTON,
            x,
            y,
            None,
            owner,
            None,
        );
        let _ = DestroyMenu(menu);
        (id.0 != 0).then_some(id.0)
    }
}
