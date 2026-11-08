//! Speaking through the screen reader, for what happens without focus
//! moving: a job starting or finishing, a choice made in a menu. The same
//! text always goes into the Status box too.
//!
//! From NoteFetch's `announce.rs` (same author, MIT), `[proven]` there with
//! JAWS, NVDA and Narrator. Two ways, as they listen differently:
//!
//! - A UI Automation notification, for JAWS and Narrator, raised on a small
//!   provider of VidFetch's own hosted by the window. It is never handed out
//!   through WM_GETOBJECT, so the window is still read through MSAA.
//! - For NVDA, which never receives those from a plain dialog, a live region:
//!   a hidden static control in the window in front, and
//!   EVENT_OBJECT_LIVEREGIONCHANGED. NVDA reads the control when it gets to
//!   the event, so what is said close together is joined, on a short timer.

use std::cell::RefCell;

use windows::core::{implement, w, Error, IUnknown, Result, BSTR, GUID, PCWSTR};
use windows::Win32::Foundation::HWND;
use windows::Win32::System::Com::{
    CLSIDFromProgID, CoCreateInstance, IDispatch, CLSCTX_ALL, DISPATCH_METHOD, DISPPARAMS,
};
use windows::Win32::System::LibraryLoader::GetModuleHandleW;
use windows::Win32::System::Variant::VARIANT;
use windows::Win32::UI::Accessibility::{
    IRawElementProviderSimple, IRawElementProviderSimple_Impl, NotificationKind_Other,
    NotificationProcessing_ImportantAll, NotifyWinEvent, ProviderOptions,
    ProviderOptions_ServerSideProvider, ProviderOptions_UseComThreading, UiaClientsAreListening,
    UiaHostProviderFromHwnd, UiaRaiseNotificationEvent, UIA_PATTERN_ID, UIA_PROPERTY_ID,
};
use windows::Win32::UI::Input::KeyboardAndMouse::GetActiveWindow;
use windows::Win32::UI::WindowsAndMessaging::{
    CreateWindowExW, FindWindowW, GetDlgItem, KillTimer, SetTimer, SetWindowTextW, CHILDID_SELF,
    EVENT_OBJECT_LIVEREGIONCHANGED, HMENU, OBJID_CLIENT, WINDOW_EX_STYLE, WS_CHILD,
};

/// A provider with nothing of its own: Windows fills in everything from the
/// window that hosts it.
#[implement(IRawElementProviderSimple)]
struct Speaker {
    hwnd: HWND,
}

impl IRawElementProviderSimple_Impl for Speaker_Impl {
    fn ProviderOptions(&self) -> Result<ProviderOptions> {
        Ok(ProviderOptions_ServerSideProvider | ProviderOptions_UseComThreading)
    }

    fn GetPatternProvider(&self, _pattern: UIA_PATTERN_ID) -> Result<IUnknown> {
        // No patterns: an empty error is S_OK with no object.
        Err(Error::empty())
    }

    fn GetPropertyValue(&self, _property: UIA_PROPERTY_ID) -> Result<VARIANT> {
        Ok(VARIANT::default())
    }

    fn HostRawElementProvider(&self) -> Result<IRawElementProviderSimple> {
        unsafe { UiaHostProviderFromHwnd(self.hwnd) }
    }
}

thread_local! {
    /// The last window spoken from, and its provider.
    static SPEAKER: RefCell<Option<(HWND, IRawElementProviderSimple)>> = const { RefCell::new(None) };
    /// What NVDA is about to be told, and the window it came from.
    static WAITING: RefCell<(Vec<String>, Option<HWND>)> = const { RefCell::new((Vec::new(), None)) };
}

/// How long NVDA's words are gathered, in milliseconds.
const GATHER_MS: u32 = 150;
/// The hidden live region's control ID, in any VidFetch window.
const LIVE_REGION: i32 = 1990;

/// Whether NVDA is running: its own hidden main window.
fn nvda_running() -> bool {
    unsafe { FindWindowW(w!("wxWindowClassNR"), w!("NVDA")) }.is_ok_and(|h| !h.is_invalid())
}

/// Stops what JAWS is saying, through the programming interface it installs.
/// Does nothing when JAWS is not running. An experiment for the quiet Play
/// window (milestone 6 test, 2.1): other screen readers are left alone.
pub fn hush() {
    unsafe {
        if FindWindowW(w!("JFWUI2"), None).is_err() {
            return;
        }
        let Ok(clsid) = CLSIDFromProgID(w!("FreedomSci.JawsApi")) else {
            return;
        };
        let Ok(api) = CoCreateInstance::<_, IDispatch>(&clsid, None, CLSCTX_ALL) else {
            return;
        };
        let name = w!("StopSpeech");
        let mut id = 0;
        if api
            .GetIDsOfNames(&GUID::zeroed(), &name, 1, 0x0400, &mut id)
            .is_ok()
        {
            let none = DISPPARAMS::default();
            let _ = api.Invoke(
                id,
                &GUID::zeroed(),
                0x0400,
                DISPATCH_METHOD,
                &none,
                None,
                None,
                None,
            );
        }
    }
}

/// Ask the screen reader to say `text`. False if nothing is listening.
pub fn say(hwnd: HWND, text: &str) -> bool {
    if nvda_running() {
        WAITING.with(|w| {
            let mut w = w.borrow_mut();
            w.0.push(text.to_string());
            w.1 = Some(hwnd);
        });
        unsafe { SetTimer(None, 0, GATHER_MS, Some(tell_nvda)) };
        return true;
    }
    unsafe {
        if !UiaClientsAreListening().as_bool() {
            return false;
        }
    }
    let provider = SPEAKER.with(|cell| {
        let mut slot = cell.borrow_mut();
        match &*slot {
            Some((h, p)) if *h == hwnd => p.clone(),
            _ => {
                let p: IRawElementProviderSimple = Speaker { hwnd }.into();
                *slot = Some((hwnd, p.clone()));
                p
            }
        }
    });
    unsafe {
        UiaRaiseNotificationEvent(
            &provider,
            NotificationKind_Other,
            NotificationProcessing_ImportantAll,
            &BSTR::from(text),
            &BSTR::from("VidFetch"),
        )
        .is_ok()
    }
}

unsafe extern "system" fn tell_nvda(_: HWND, _: u32, id: usize, _: u32) {
    let _ = KillTimer(None, id);
    let (said, hwnd) = WAITING.with(|w| {
        let mut w = w.borrow_mut();
        (std::mem::take(&mut w.0), w.1.take())
    });
    if let (false, Some(hwnd)) = (said.is_empty(), hwnd) {
        say_live(hwnd, &said.join(" "));
    }
}

/// Puts `text` in the hidden live region of the VidFetch window that is
/// active, or of `main` if none is: NVDA ignores windows behind the one in
/// front. The control is the window's last child, so it labels nothing, and
/// is never shown or reached with Tab.
fn say_live(main: HWND, text: &str) {
    let t = crate::wide(text);
    unsafe {
        let active = GetActiveWindow();
        let host = if active.is_invalid() { main } else { active };
        let region = match GetDlgItem(Some(host), LIVE_REGION) {
            Ok(r) if !r.is_invalid() => r,
            _ => {
                let instance = GetModuleHandleW(None).ok().map(Into::into);
                let Ok(r) = CreateWindowExW(
                    WINDOW_EX_STYLE(0),
                    w!("STATIC"),
                    w!(""),
                    WS_CHILD,
                    0,
                    0,
                    0,
                    0,
                    Some(host),
                    Some(HMENU(LIVE_REGION as usize as *mut _)),
                    instance,
                    None,
                ) else {
                    return;
                };
                r
            }
        };
        let _ = SetWindowTextW(region, PCWSTR(t.as_ptr()));
        NotifyWinEvent(
            EVENT_OBJECT_LIVEREGIONCHANGED,
            region,
            OBJID_CLIENT.0,
            CHILDID_SELF as i32,
        );
    }
}
