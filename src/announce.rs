//! Speaking through the screen reader with UI Automation notification events,
//! for what happens without focus moving: a job starting or finishing, a
//! choice made in a menu. The same text always goes into the Status box too.
//!
//! `[proven]` in NoteFetch (milestone 3, 18 October 2026) and Photo Scribe: JAWS speaks these
//! from a plain Win32 dialog, after its own speech.

use windows::core::{w, BSTR, GUID};
use windows::Win32::Foundation::HWND;
use windows::Win32::System::Com::{
    CLSIDFromProgID, CoCreateInstance, IDispatch, CLSCTX_ALL, DISPATCH_METHOD, DISPPARAMS,
};
use windows::Win32::UI::Accessibility::{
    NotificationKind_Other, NotificationProcessing_ImportantAll, UiaClientsAreListening,
    UiaHostProviderFromHwnd, UiaRaiseNotificationEvent,
};
use windows::Win32::UI::WindowsAndMessaging::FindWindowW;

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
    unsafe {
        if !UiaClientsAreListening().as_bool() {
            return false;
        }
        let Ok(provider) = UiaHostProviderFromHwnd(hwnd) else {
            return false;
        };
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
