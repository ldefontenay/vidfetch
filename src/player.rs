//! Playing a download's sound, with pause and skipping: Media Foundation
//! decodes from wherever playing starts, and Windows' waveOut plays it a tenth
//! of a second at a time, so a long film is never held in memory and skipping
//! starts again at once. Video Scribe's player.rs (MIT, same author); it takes
//! a web address as readily as a file.
//!
//! The player has a thread of its own, as Media Foundation must not start on
//! the window's thread. The window sends it commands and reads where it is.
//!
//! It can play faster or slower without changing the voice (stretch.rs), and
//! sets its own volume in Windows' volume mixer, which Windows keeps for next
//! time (log entry 3, ideas 2 and 4). It can cut every silence down to a
//! second (log entry 4, Q2: "Nice option to have, but should be off by
//! default").

use std::collections::VecDeque;
use std::sync::atomic::{AtomicBool, AtomicU32, AtomicU64, Ordering};
use std::sync::mpsc::{channel, Receiver, RecvTimeoutError, Sender};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use windows::Win32::Media::Audio::*;
use windows::Win32::Media::{MMTIME, TIME_SAMPLES};

use crate::media;
use crate::stretch::Stretch;

const RATE: u32 = 48_000;
const CHANNELS: u32 = 2;
/// Frames in one buffer, a tenth of a second, and buffers queued at once: a
/// second ahead, enough that decoding never makes it stutter.
const BLOCK: usize = 4_800;
const BUFFERS: usize = 10;

enum Command {
    Play,
    Pause,
    Seek(f64),
    Speed(f64),
    /// Percent.
    Volume(u32),
    Quit,
}

/// Why playing failed when the source gave no sound at all, though it said
/// how long it was: a YouTube sound file whose edit list stops Media
/// Foundation reading it (milestone 4 test, section 3). The Play window then
/// fetches the file whole and plays it without the edit list.
pub const NO_SOUND: &str = "It gave no sound at all.";

/// Below this, a sample is silence: about -45 dB.
const QUIET: i16 = 184;
/// How much of a silence is kept when silences are cut, in frames: a second.
const KEPT_QUIET: usize = RATE as usize;

/// The slowest and fastest speeds, and the step between them.
pub const SLOWEST: f64 = 0.5;
pub const FASTEST: f64 = 3.0;
pub const SPEED_STEP: f64 = 0.25;

/// What the window can read at any time.
#[derive(Default)]
struct Shared {
    /// Where playing is, in milliseconds.
    at_ms: AtomicU64,
    playing: AtomicBool,
    /// Played to the end, and not yet told.
    ended: AtomicBool,
    /// Stopped at the end, so playing again starts from the beginning.
    at_end: AtomicBool,
    /// The volume in percent, as Windows has it.
    volume: AtomicU32,
    /// Why it stopped, if it failed.
    failed: Mutex<Option<String>>,
}

impl Shared {
    fn set_at(&self, secs: f64) {
        self.at_ms
            .store((secs.max(0.0) * 1000.0) as u64, Ordering::Relaxed);
    }
}

pub struct Player {
    send: Sender<Command>,
    shared: Arc<Shared>,
    thread: Option<std::thread::JoinHandle<()>>,
    secs: f64,
    speed: f64,
}

impl Player {
    /// A player for `source`, a file or a web address, paused at the start,
    /// to play at `speed`; `cut_silence`: every silence cut to a second.
    pub fn open(source: &str, speed: f64, cut_silence: bool) -> Result<Player, String> {
        let secs = {
            let s = source.to_string();
            std::thread::spawn(move || {
                let _session = media::Session::start()?;
                media::length(&s)
            })
            .join()
            .map_err(|_| "The sound could not be read.".to_string())??
        };
        let (send, receive) = channel();
        let shared = Arc::new(Shared::default());
        shared.volume.store(100, Ordering::Relaxed);
        let source = source.to_string();
        let theirs = shared.clone();
        let speed = speed.clamp(SLOWEST, FASTEST);
        let thread = std::thread::spawn(move || {
            if let Err(why) = run(&source, speed, cut_silence, secs, receive, &theirs) {
                theirs.playing.store(false, Ordering::Relaxed);
                *theirs.failed.lock().unwrap() = Some(why);
            }
        });
        Ok(Player {
            send,
            shared,
            thread: Some(thread),
            secs,
            speed,
        })
    }

    pub fn speed(&self) -> f64 {
        self.speed
    }

    /// Plays at `speed`, kept within what sounds right; the speed it now has.
    /// What is already queued, under a second, plays out at the old speed.
    pub fn set_speed(&mut self, speed: f64) -> f64 {
        self.speed = ((speed / SPEED_STEP).round() * SPEED_STEP).clamp(SLOWEST, FASTEST);
        let _ = self.send.send(Command::Speed(self.speed));
        self.speed
    }

    /// The volume in percent.
    pub fn volume(&self) -> u32 {
        self.shared.volume.load(Ordering::Relaxed)
    }

    /// Sets the volume in percent, from 0 to 100; the volume it now has.
    pub fn set_volume(&self, percent: i32) -> u32 {
        let v = percent.clamp(0, 100) as u32;
        self.shared.volume.store(v, Ordering::Relaxed);
        let _ = self.send.send(Command::Volume(v));
        v
    }

    /// The sound's length in seconds; nought when the source does not say.
    pub fn secs(&self) -> f64 {
        self.secs
    }

    /// The length, or as far as anything can go when it is not known.
    fn end(&self) -> f64 {
        if self.secs > 0.0 {
            self.secs
        } else {
            f64::MAX
        }
    }

    /// Where playing is, in seconds.
    pub fn at(&self) -> f64 {
        (self.shared.at_ms.load(Ordering::Relaxed) as f64 / 1000.0).min(self.end())
    }

    pub fn playing(&self) -> bool {
        self.shared.playing.load(Ordering::Relaxed)
    }

    /// True once, after playing reaches the end.
    pub fn take_ended(&self) -> bool {
        self.shared.ended.swap(false, Ordering::Relaxed)
    }

    /// Played to the end, and not moved since.
    pub fn finished(&self) -> bool {
        self.shared.at_end.load(Ordering::Relaxed)
    }

    pub fn take_failure(&self) -> Option<String> {
        self.shared.failed.lock().unwrap().take()
    }

    pub fn play(&self) {
        // From the start again once it has played to the end.
        if self.shared.at_end.load(Ordering::Relaxed)
            || (self.secs > 0.0 && self.at() >= self.secs - 0.05)
        {
            self.seek(0.0);
        }
        self.shared.playing.store(true, Ordering::Relaxed);
        let _ = self.send.send(Command::Play);
    }

    pub fn pause(&self) {
        self.shared.playing.store(false, Ordering::Relaxed);
        let _ = self.send.send(Command::Pause);
    }

    /// Moves to `secs`, kept within the sound, playing on if it was playing.
    /// Where it lands.
    pub fn seek(&self, secs: f64) -> f64 {
        let to = secs.clamp(0.0, self.end());
        self.shared.at_end.store(false, Ordering::Relaxed);
        self.shared.set_at(to);
        let _ = self.send.send(Command::Seek(to));
        to
    }
}

impl Drop for Player {
    fn drop(&mut self) {
        let _ = self.send.send(Command::Quit);
        if let Some(t) = self.thread.take() {
            let _ = t.join();
        }
    }
}

/// Puts back this app's volume on the output if it is at nought, as Video
/// Scribe does: Windows keeps an app's volume for each device. The volume
/// then, in percent.
fn unmute(device: HWAVEOUT) -> u32 {
    let mut volume = 0u32;
    unsafe {
        if waveOutGetVolume(Some(device), &mut volume) == 0 && volume == 0 {
            waveOutSetVolume(Some(device), u32::MAX);
            volume = u32::MAX;
        }
    }
    ((volume & 0xFFFF) * 100 + 0x7FFF) / 0xFFFF
}

/// Sets this app's volume, both sides alike.
fn set_volume(device: HWAVEOUT, percent: u32) {
    let v = percent.min(100) * 0xFFFF / 100;
    unsafe {
        waveOutSetVolume(Some(device), (v << 16) | v);
    }
}

/// The output device and its buffers. The buffers' memory stays put while
/// Windows plays from it: each is allocated once and never resized.
struct Out {
    device: HWAVEOUT,
    /// The volume it opened with, in percent.
    volume: u32,
    /// Tests play silence, and so does the app when VF_SILENT is set.
    silent: bool,
    headers: Vec<WAVEHDR>,
    data: Vec<Vec<u8>>,
    /// Which buffers Windows has and has not finished with.
    queued: Vec<bool>,
    /// The source said how long it is.
    secs_known: bool,
}

const HEADER: u32 = std::mem::size_of::<WAVEHDR>() as u32;

impl Out {
    fn open() -> Result<Out, String> {
        let format = WAVEFORMATEX {
            wFormatTag: WAVE_FORMAT_PCM as u16,
            nChannels: CHANNELS as u16,
            nSamplesPerSec: RATE,
            nAvgBytesPerSec: RATE * CHANNELS * 2,
            nBlockAlign: (CHANNELS * 2) as u16,
            wBitsPerSample: 16,
            cbSize: 0,
        };
        let mut device = HWAVEOUT::default();
        let r = unsafe {
            waveOutOpen(
                Some(&mut device),
                WAVE_MAPPER,
                &format,
                None,
                None,
                CALLBACK_NULL,
            )
        };
        if r != 0 {
            return Err(format!(
                "Windows could not open the sound output (error {r}). Check that a speaker or \
                 headphones are connected."
            ));
        }
        let volume = unmute(device);
        Ok(Out {
            device,
            volume,
            silent: cfg!(test) || std::env::var_os("VF_SILENT").is_some(),
            headers: vec![WAVEHDR::default(); BUFFERS],
            data: (0..BUFFERS)
                .map(|_| vec![0u8; BLOCK * CHANNELS as usize * 2])
                .collect(),
            queued: vec![false; BUFFERS],
            secs_known: false,
        })
    }

    /// Takes back the buffers Windows has finished with. Whether any are
    /// still queued.
    fn reap(&mut self) -> bool {
        for i in 0..BUFFERS {
            if self.queued[i] && self.headers[i].dwFlags & WHDR_DONE != 0 {
                unsafe {
                    waveOutUnprepareHeader(self.device, &mut self.headers[i], HEADER);
                }
                self.queued[i] = false;
            }
        }
        self.queued.iter().any(|&q| q)
    }

    fn free(&mut self) -> Option<usize> {
        self.reap();
        self.queued.iter().position(|&q| !q)
    }

    fn write(&mut self, i: usize, samples: &[i16]) {
        let bytes = &mut self.data[i];
        for (k, s) in samples.iter().enumerate() {
            let s = if self.silent { 0 } else { *s };
            bytes[k * 2..k * 2 + 2].copy_from_slice(&s.to_le_bytes());
        }
        let h = &mut self.headers[i];
        *h = WAVEHDR::default();
        h.lpData = windows::core::PSTR(bytes.as_mut_ptr());
        h.dwBufferLength = (samples.len() * 2) as u32;
        unsafe {
            waveOutPrepareHeader(self.device, h, HEADER);
            waveOutWrite(self.device, h, HEADER);
        }
        self.queued[i] = true;
    }

    fn pause(&self) {
        unsafe {
            waveOutPause(self.device);
        }
    }

    fn resume(&self) {
        unsafe {
            waveOutRestart(self.device);
        }
    }

    /// Stops at once and takes back every buffer; the count of what was
    /// played starts again at nought.
    fn reset(&mut self) {
        unsafe {
            waveOutReset(self.device);
        }
        self.reap();
    }

    /// Frames played since the last reset.
    fn played(&self) -> u64 {
        let mut t = MMTIME {
            wType: TIME_SAMPLES,
            ..Default::default()
        };
        unsafe {
            waveOutGetPosition(self.device, &mut t, std::mem::size_of::<MMTIME>() as u32);
            t.u.sample as u64
        }
    }
}

impl Drop for Out {
    fn drop(&mut self) {
        self.reset();
        unsafe {
            waveOutClose(self.device);
        }
    }
}

/// Playing from one place: the device, where it started, and whether it is
/// paused. Answers commands between buffers.
struct Run<'a> {
    out: Out,
    commands: &'a Receiver<Command>,
    shared: &'a Shared,
    base: f64,
    paused: bool,
    /// A command that ends this run: a seek, or quitting.
    next: Option<Command>,
    stretch: Stretch,
    /// Output waiting to fill a buffer.
    pending: Vec<i16>,
    /// Frames of the sound the output so far stands for, from `base`.
    used: f64,
    /// Frames handed to the device since the last reset.
    written: u64,
    /// Where in the sound each buffer ends: (frames written, seconds). At a
    /// speed other than normal, a frame played is not a frame of the sound.
    marks: VecDeque<(u64, f64)>,
    /// The last mark already played.
    anchor: (u64, f64),
    /// Silences are cut to a second, and how long the one now has lasted, in
    /// frames.
    cut_silence: bool,
    quiet: usize,
    /// What is kept of each piece decoded.
    kept: Vec<i16>,
}

impl Run<'_> {
    /// Starts again from `base`, with nothing queued.
    fn restart(&mut self) {
        self.out.reset();
        self.stretch.clear();
        self.pending.clear();
        self.used = 0.0;
        self.written = 0;
        self.marks.clear();
        self.anchor = (0, self.base);
        self.quiet = 0;
    }

    /// The frames of `piece` to play, with each silence cut to a second. The
    /// frames cut still count as sound used, so where playing is stays true.
    fn keep(&mut self, piece: &[i16]) -> usize {
        let per = CHANNELS as usize;
        self.kept.clear();
        let mut cut = 0;
        for frame in piece.chunks_exact(per) {
            if self.cut_silence && frame.iter().all(|s| s.unsigned_abs() < QUIET as u16) {
                self.quiet += 1;
                if self.quiet > KEPT_QUIET {
                    cut += 1;
                    continue;
                }
            } else {
                self.quiet = 0;
            }
            self.kept.extend_from_slice(frame);
        }
        cut
    }

    /// Where in the sound playing is, in seconds.
    fn position(&mut self) -> f64 {
        let played = self.out.played();
        while let Some(&(frames, secs)) = self.marks.front() {
            if frames > played {
                let (f0, s0) = self.anchor;
                let part = (played - f0.min(played)) as f64 / (frames - f0).max(1) as f64;
                return s0 + part * (secs - s0);
            }
            self.anchor = (frames, secs);
            self.marks.pop_front();
        }
        self.anchor.1
    }

    /// Sends full buffers of what is pending to the device. False if a
    /// command ends the run.
    fn send(&mut self, all: bool) -> bool {
        let per = CHANNELS as usize;
        while self.pending.len() >= BLOCK * per || (all && !self.pending.is_empty()) {
            let n = self.pending.len().min(BLOCK * per);
            let Some(i) = self.room() else {
                return false;
            };
            self.out.write(i, &self.pending[..n]);
            self.pending.drain(..n);
            self.written += (n / per) as u64;
            // What is still pending stands for the end of what was used.
            let ahead = (self.pending.len() / per) as f64 * self.stretch.speed();
            let secs = self.base + (self.used - ahead).max(0.0) / RATE as f64;
            self.marks.push_back((self.written, secs));
            // Kept current even when decoding only just keeps up.
            if !self.paused {
                let at = self.position();
                self.shared.set_at(at);
            }
        }
        true
    }

    /// Waits up to 15 ms for a command and acts on it. False once a command
    /// ends the run.
    fn listen(&mut self) -> bool {
        if !self.paused {
            let at = self.position();
            self.shared.set_at(at);
        }
        match self.commands.recv_timeout(Duration::from_millis(15)) {
            Ok(Command::Pause) => {
                if !self.paused {
                    self.out.pause();
                    self.paused = true;
                    let at = self.position();
                    self.shared.set_at(at);
                }
            }
            Ok(Command::Play) => {
                if self.paused {
                    self.out.resume();
                    self.paused = false;
                }
            }
            Ok(Command::Speed(s)) => self.stretch.set_speed(s),
            Ok(Command::Volume(v)) => set_volume(self.out.device, v),
            Ok(c) => self.next = Some(c),
            Err(RecvTimeoutError::Timeout) => {}
            Err(RecvTimeoutError::Disconnected) => self.next = Some(Command::Quit),
        }
        self.next.is_none()
    }

    /// A free buffer, waiting for one; None if a command ends the run.
    fn room(&mut self) -> Option<usize> {
        loop {
            if !self.paused {
                if let Some(i) = self.out.free() {
                    return Some(i);
                }
            }
            if !self.listen() {
                return None;
            }
        }
    }
}

/// Why decoding was broken off.
const BROKEN_OFF: &str = "\u{1}broken off";

/// The player's thread: paused until told to play, then decoding and queueing
/// from where it is.
fn run(
    source: &str,
    speed: f64,
    cut_silence: bool,
    secs: f64,
    commands: Receiver<Command>,
    shared: &Shared,
) -> Result<(), String> {
    let _session = media::Session::start()?;
    let mut out = Out::open()?;
    out.secs_known = secs > 1.0;
    shared.volume.store(out.volume, Ordering::Relaxed);
    let mut run = Run {
        out,
        commands: &commands,
        shared,
        base: 0.0,
        paused: false,
        next: None,
        stretch: Stretch::new(CHANNELS as usize, speed),
        pending: Vec::with_capacity(BLOCK * CHANNELS as usize * 2),
        used: 0.0,
        written: 0,
        marks: VecDeque::new(),
        anchor: (0, 0.0),
        cut_silence,
        quiet: 0,
        kept: Vec::with_capacity(BLOCK * CHANNELS as usize),
    };
    let mut playing = false;
    // Whether any sound came at all, to tell a source that gives none.
    let mut heard = false;
    let per = CHANNELS as usize;
    loop {
        if !playing {
            match commands.recv() {
                Ok(Command::Play) => playing = true,
                Ok(Command::Pause) => {}
                Ok(Command::Seek(to)) => run.base = to,
                Ok(Command::Speed(s)) => run.stretch.set_speed(s),
                Ok(Command::Volume(v)) => set_volume(run.out.device, v),
                Ok(Command::Quit) | Err(_) => return Ok(()),
            }
            continue;
        }
        shared.ended.store(false, Ordering::Relaxed);
        shared.at_end.store(false, Ordering::Relaxed);
        run.restart();
        // A reset device may still be paused from before.
        run.out.resume();
        run.paused = false;
        run.next = None;
        let first = (run.base * RATE as f64).round() as i64;
        let stop = AtomicBool::new(false);
        let decoded = media::sound_from(source, run.base, RATE, CHANNELS, &stop, &mut |at, s| {
            let start = (at * RATE as f64).round() as i64;
            // Seeking lands near the place, not on it.
            let skip = (first - start).clamp(0, (s.len() / per) as i64) as usize;
            heard |= !s.is_empty();
            let cut = run.keep(&s[skip * per..]);
            run.used += cut as f64;
            let kept = std::mem::take(&mut run.kept);
            run.stretch.push(&kept);
            run.kept = kept;
            let before = run.pending.len();
            run.stretch.pull(&mut run.pending);
            run.used += ((run.pending.len() - before) / per) as f64 * run.stretch.speed();
            if run.send(false) {
                Ok(())
            } else {
                Err(BROKEN_OFF.into())
            }
        });
        match decoded {
            Err(why) if why != BROKEN_OFF => return Err(why),
            Err(_) => {}
            Ok(_) if !heard && run.out.secs_known && run.base + 1.0 < secs => {
                return Err(NO_SOUND.into())
            }
            Ok(_) => {
                // The last of it, then play out what is queued.
                run.send(true);
                while run.next.is_none() && (run.paused || run.out.reap()) {
                    run.listen();
                }
                if run.next.is_none() {
                    let end = run.position();
                    run.out.reset();
                    run.base = end;
                    shared.set_at(end);
                    playing = false;
                    shared.playing.store(false, Ordering::Relaxed);
                    shared.at_end.store(true, Ordering::Relaxed);
                    shared.ended.store(true, Ordering::Relaxed);
                    continue;
                }
            }
        }
        // Paused when moved: it stays paused at the new place.
        if run.paused {
            playing = false;
        }
        match run.next.take() {
            Some(Command::Seek(to)) => {
                run.out.reset();
                run.base = to;
                shared.set_at(to);
            }
            Some(Command::Quit) | None => return Ok(()),
            Some(_) => {}
        }
    }
}

#[cfg(test)]
mod on_a_file {
    use super::*;

    /// Needs a sound file over 70 seconds long, or its web address:
    /// `VF_TEST_SOUND=<path> cargo test plays -- --ignored`. Plays at no volume.
    #[test]
    #[ignore]
    fn plays_pauses_and_skips() {
        let path = std::env::var("VF_TEST_SOUND").unwrap();
        let p = Player::open(&path, 1.0, false).unwrap();
        assert!(p.secs() > 70.0);
        let wait = |ms| std::thread::sleep(Duration::from_millis(ms));
        p.play();
        wait(1500);
        let a = p.at();
        assert!(a > 0.5 && a < 1.6, "{a}");
        p.pause();
        wait(300);
        let b = p.at();
        wait(500);
        assert!((p.at() - b).abs() < 0.05, "moved while paused");
        assert!(!p.playing());
        // Moved while paused: stays paused there.
        p.seek(60.0);
        wait(300);
        assert!((p.at() - 60.0).abs() < 0.05, "{}", p.at());
        p.play();
        wait(1000);
        let c = p.at();
        assert!(c > 60.3 && c < 61.1, "{c}");
        // Moved while playing: plays on from there.
        p.seek(30.0);
        wait(1000);
        let d = p.at();
        assert!(d > 30.3 && d < 31.1, "{d}");
        // To the end.
        p.seek(p.secs() - 0.5);
        wait(1500);
        assert!(p.take_ended());
        assert!(!p.playing());
        assert!(p.take_failure().is_none());
    }

    /// Twice as fast: the place moves twice as fast too.
    #[test]
    #[ignore]
    fn plays_faster() {
        let path = std::env::var("VF_TEST_SOUND").unwrap();
        let mut p = Player::open(&path, 1.0, false).unwrap();
        assert_eq!(p.set_speed(2.1), 2.0);
        p.seek(10.0);
        p.play();
        std::thread::sleep(Duration::from_millis(2000));
        let a = p.at();
        assert!(a > 12.8 && a < 14.3, "{a}");
        assert!(p.volume() <= 100);
    }

    /// A file that gives no sound, such as VF_TEST_NO_SOUND: said so.
    #[test]
    #[ignore]
    fn says_when_there_is_no_sound() {
        let path = std::env::var("VF_TEST_NO_SOUND").unwrap();
        let p = Player::open(&path, 1.0, false).unwrap();
        p.play();
        std::thread::sleep(Duration::from_millis(1500));
        assert_eq!(p.take_failure().as_deref(), Some(NO_SOUND));
    }

    /// Silences cut: a silence of 3 seconds plays as 1, and where playing is
    /// still moves on by all 3.
    #[test]
    fn silences_are_cut_to_a_second() {
        let commands = channel().1;
        let shared = Shared::default();
        let mut run = Run {
            out: match Out::open() {
                Ok(o) => o,
                // No sound device on this machine: nothing to try.
                Err(_) => return,
            },
            commands: &commands,
            shared: &shared,
            base: 0.0,
            paused: false,
            next: None,
            stretch: Stretch::new(2, 1.0),
            pending: Vec::new(),
            used: 0.0,
            written: 0,
            marks: VecDeque::new(),
            anchor: (0, 0.0),
            cut_silence: true,
            quiet: 0,
            kept: Vec::new(),
        };
        let quiet = vec![3i16; RATE as usize * 3 * 2];
        let cut = run.keep(&quiet);
        assert_eq!(cut, RATE as usize * 2);
        assert_eq!(run.kept.len(), RATE as usize * 2);
        let loud = vec![9000i16; 200];
        assert_eq!(run.keep(&loud), 0);
        assert_eq!(run.quiet, 0);
        run.cut_silence = false;
        assert_eq!(run.keep(&quiet), 0);
    }

    /// A web address, such as yt-dlp's `-g` gives: slower to start than a
    /// file. `VF_TEST_STREAM=<address> cargo test streams -- --ignored`.
    #[test]
    #[ignore]
    fn streams_from_a_web_address() {
        let url = std::env::var("VF_TEST_STREAM").unwrap();
        let p = Player::open(&url, 1.0, false).unwrap();
        let wait = |ms| std::thread::sleep(Duration::from_millis(ms));
        p.play();
        let started = std::time::Instant::now();
        while p.at() == 0.0 && started.elapsed() < Duration::from_secs(20) {
            wait(100);
        }
        println!("started after {:.1} s", started.elapsed().as_secs_f64());
        assert!(p.at() > 0.0, "never started");
        p.seek(300.0);
        let moved = std::time::Instant::now();
        while p.at() < 300.5 && moved.elapsed() < Duration::from_secs(20) {
            wait(100);
        }
        println!("moved after {:.1} s", moved.elapsed().as_secs_f64());
        assert!(p.at() > 300.5, "{}", p.at());
        assert!(p.take_failure().is_none());
    }
}
