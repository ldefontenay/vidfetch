#![windows_subsystem = "windows"]

//! VidFetch: downloads videos and playlists from YouTube and other sites,
//! with a screen reader first. Built from Video Scribe's parts; see PLAN.md.

mod addvideo;
mod announce;
mod captions;
mod channels;
mod chapters;
mod config;
mod downloads;
mod history;
mod import;
mod job;
mod library;
mod listen;
mod media;
mod message;
mod namebox;
mod net;
mod newer;
mod places;
mod player;
mod queue;
mod searchbox;
mod stretch;
mod tags;
mod ui;
mod youtube;
mod ytdlp;

use windows::Win32::System::Com::{CoInitializeEx, COINIT_APARTMENTTHREADED};

fn main() {
    // The UI thread is a single-threaded apartment: the folder picker is a
    // COM object that insists on it.
    unsafe {
        let _ = CoInitializeEx(None, COINIT_APARTMENTTHREADED);
    }
    let first_run = !config::Config::exists();
    ui::run(config::Config::load(), first_run);
}

/// A string as a null-terminated UTF-16 buffer for Win32.
pub fn wide(text: &str) -> Vec<u16> {
    text.encode_utf16().chain(std::iter::once(0)).collect()
}
