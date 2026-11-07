//! Media Foundation, which is part of Windows, in place of FFmpeg: it puts
//! YouTube's separate picture and sound into one MP4, takes the sound alone
//! into an M4A, and makes MP3s. From Video Scribe's mux.rs and media.rs.
//!
//! Picture and AAC sound are copied as they are, never decoded or encoded
//! again, so nothing is lost and it is quick. Only an MP3 is encoded. Every
//! file is written under a temporary name and renamed when whole, so a
//! stopped download never leaves half a file under the real name.

use std::path::Path;
use std::sync::atomic::{AtomicBool, Ordering};

use windows::core::{GUID, HSTRING};
use windows::Win32::Media::MediaFoundation::*;
use windows::Win32::System::Com::{CoInitializeEx, CoUninitialize, COINIT_MULTITHREADED};

const ALL: u32 = MF_SOURCE_READER_ALL_STREAMS.0 as u32;
const VIDEO: u32 = MF_SOURCE_READER_FIRST_VIDEO_STREAM.0 as u32;
const AUDIO: u32 = MF_SOURCE_READER_FIRST_AUDIO_STREAM.0 as u32;
const EOS: u32 = MF_SOURCE_READERF_ENDOFSTREAM.0 as u32;
/// Media Foundation counts time in 100-nanosecond units.
const HNS: f64 = 10_000_000.0;
/// How far the sound is written ahead of the pictures, so the two are
/// interleaved in the file rather than one after the other.
const AHEAD: f64 = 0.5;
/// An MP3 at CD rate, 192 kbps: as good as YouTube's own sound allows.
const MP3_RATE: u32 = 44_100;
const MP3_KBPS: u32 = 192;

/// COM and Media Foundation for the life of a worker thread.
pub struct Session;

impl Session {
    pub fn start() -> Result<Session, String> {
        unsafe {
            let _ = CoInitializeEx(None, COINIT_MULTITHREADED);
            MFStartup(MF_VERSION, MFSTARTUP_FULL)
                .map_err(|e| format!("Windows' media support would not start: {}", e.message()))?;
        }
        Ok(Session)
    }
}

impl Drop for Session {
    fn drop(&mut self) {
        unsafe {
            let _ = MFShutdown();
            CoUninitialize();
        }
    }
}

fn shown(path: &Path) -> String {
    path.file_name()
        .map_or_else(String::new, |n| n.to_string_lossy().into_owned())
}

/// Writes `out` through `write` into a temporary file beside it, renamed to
/// `out` when whole. False if stopped; the temporary file is then gone.
fn into_place(
    out: &Path,
    write: impl FnOnce(&Path) -> windows::core::Result<bool>,
) -> Result<bool, String> {
    let part = out.with_extension(format!(
        "{}.part",
        out.extension().unwrap_or_default().to_string_lossy()
    ));
    let name = shown(out);
    let result = write(&part).map_err(|e| {
        format!(
            "Windows could not make {name}: {}",
            e.message().trim().trim_end_matches('.')
        )
    });
    match result {
        Ok(true) => std::fs::rename(&part, out).map(|_| true).map_err(|e| {
            let _ = std::fs::remove_file(&part);
            format!("{name} could not be saved: {e}")
        }),
        other => {
            let _ = std::fs::remove_file(&part);
            other
        }
    }
}

unsafe fn reader(path: &Path) -> windows::core::Result<IMFSourceReader> {
    let r = MFCreateSourceReaderFromURL(&HSTRING::from(path.as_os_str()), None)?;
    r.SetStreamSelection(ALL, false)?;
    Ok(r)
}

/// One sample from `stream`: its time in seconds, or None at the end.
unsafe fn next(
    r: &IMFSourceReader,
    stream: u32,
) -> windows::core::Result<Option<(f64, IMFSample)>> {
    loop {
        let mut flags = 0u32;
        let mut stamp = 0i64;
        let mut sample = None;
        r.ReadSample(
            stream,
            0,
            None,
            Some(&mut flags),
            Some(&mut stamp),
            Some(&mut sample),
        )?;
        if let Some(s) = sample {
            return Ok(Some((stamp as f64 / HNS, s)));
        }
        if flags & EOS != 0 {
            return Ok(None);
        }
        // A gap in the stream: read on.
    }
}

/// How long the source is, in seconds, or nought if it does not say.
unsafe fn secs_of(r: &IMFSourceReader) -> f64 {
    r.GetPresentationAttribute(MF_SOURCE_READER_MEDIASOURCE.0 as u32, &MF_PD_DURATION)
        .ok()
        .and_then(|p| u64::try_from(&p).ok())
        .map_or(0.0, |d| d as f64 / HNS)
}

unsafe fn mp4_writer(part: &Path) -> windows::core::Result<IMFSinkWriter> {
    // The name ends in .part, so the container is said outright.
    let mut attr = None;
    MFCreateAttributes(&mut attr, 1)?;
    let attr = attr.ok_or_else(windows::core::Error::empty)?;
    attr.SetGUID(&MF_TRANSCODE_CONTAINERTYPE, &MFTranscodeContainerType_MPEG4)?;
    MFCreateSinkWriterFromURL(&HSTRING::from(part.as_os_str()), None, &attr)
}

/// Writes `out`, an MP4 (or, without `picture`, an M4A): the picture of
/// `picture`, with `shift` the seconds to add to a time on its own clock to
/// get Media Foundation's (`ytdlp::drop_edit_list`), and the sound of
/// `sound`, both copied as they are. False if stopped.
pub fn join(
    picture: Option<(&Path, f64)>,
    sound: &Path,
    out: &Path,
    stop: &AtomicBool,
    progress: &mut dyn FnMut(f64),
) -> Result<bool, String> {
    into_place(out, |part| unsafe {
        join_into(picture, sound, part, stop, progress)
    })
}

unsafe fn join_into(
    picture: Option<(&Path, f64)>,
    sound: &Path,
    part: &Path,
    stop: &AtomicBool,
    progress: &mut dyn FnMut(f64),
) -> windows::core::Result<bool> {
    let writer = mp4_writer(part)?;
    let pictures = match picture {
        Some((p, _)) => {
            let r = reader(p)?;
            r.SetStreamSelection(VIDEO, true)?;
            let native = r.GetNativeMediaType(VIDEO, 0)?;
            r.SetCurrentMediaType(VIDEO, None, &native)?;
            let v = writer.AddStream(&native)?;
            writer.SetInputMediaType(v, &native, None)?;
            Some((r, v))
        }
        None => None,
    };
    let sounds = reader(sound)?;
    sounds.SetStreamSelection(AUDIO, true)?;
    let native = sounds.GetNativeMediaType(AUDIO, 0)?;
    sounds.SetCurrentMediaType(AUDIO, None, &native)?;
    let a = writer.AddStream(&native)?;
    writer.SetInputMediaType(a, &native, None)?;
    writer.BeginWriting()?;
    let secs = secs_of(&sounds);
    let shift = picture.map_or(0.0, |p| p.1);

    // On the picture's own clock a frame is at its Media Foundation time less
    // `shift`. Should that put the first frame before nought, everything
    // moves later by `lead`, the sound with it, so the two stay together.
    let mut frame = match &pictures {
        Some((r, _)) => next(r, VIDEO)?,
        None => None,
    };
    let lead = frame.as_ref().map_or(0.0, |(t, _)| (shift - t).max(0.0));
    let mut sound_next = next(&sounds, AUDIO)?;
    if let Some((r, v)) = &pictures {
        while let Some((t, sample)) = frame.take() {
            if stop.load(Ordering::Relaxed) {
                return Ok(false);
            }
            let at = t - shift + lead;
            while let Some((st, s)) = sound_next.take() {
                if st + lead > at + AHEAD {
                    sound_next = Some((st, s));
                    break;
                }
                s.SetSampleTime(((st + lead) * HNS) as i64)?;
                writer.WriteSample(a, &s)?;
                sound_next = next(&sounds, AUDIO)?;
            }
            sample.SetSampleTime((at * HNS) as i64)?;
            writer.WriteSample(*v, &sample)?;
            if secs > 0.0 {
                progress((at / secs).clamp(0.0, 1.0));
            }
            frame = next(r, VIDEO)?;
        }
    }
    while let Some((st, s)) = sound_next.take() {
        if stop.load(Ordering::Relaxed) {
            return Ok(false);
        }
        s.SetSampleTime(((st + lead) * HNS) as i64)?;
        writer.WriteSample(a, &s)?;
        if secs > 0.0 {
            progress((st / secs).clamp(0.0, 1.0));
        }
        sound_next = next(&sounds, AUDIO)?;
    }
    writer.Finalize()?;
    Ok(true)
}

/// Whether the sound of `path` is AAC, which an M4A holds as it is.
pub fn sound_is_aac(path: &Path) -> bool {
    unsafe {
        let Ok(r) = reader(path) else { return false };
        if r.SetStreamSelection(AUDIO, true).is_err() {
            return false;
        }
        r.GetNativeMediaType(AUDIO, 0)
            .and_then(|t| t.GetGUID(&MF_MT_SUBTYPE))
            .is_ok_and(|g: GUID| g == MFAudioFormat_AAC)
    }
}

/// Writes `out`, an MP3 of the sound of `sound`. False if stopped.
pub fn mp3(
    sound: &Path,
    out: &Path,
    stop: &AtomicBool,
    progress: &mut dyn FnMut(f64),
) -> Result<bool, String> {
    into_place(out, |part| unsafe { mp3_into(sound, part, stop, progress) })
}

unsafe fn pcm(channels: u32) -> windows::core::Result<IMFMediaType> {
    pcm_at(MP3_RATE, channels)
}

/// 16-bit samples at `rate`, which Media Foundation converts to.
unsafe fn pcm_at(rate: u32, channels: u32) -> windows::core::Result<IMFMediaType> {
    let t = MFCreateMediaType()?;
    t.SetGUID(&MF_MT_MAJOR_TYPE, &MFMediaType_Audio)?;
    t.SetGUID(&MF_MT_SUBTYPE, &MFAudioFormat_PCM)?;
    t.SetUINT32(&MF_MT_AUDIO_SAMPLES_PER_SECOND, rate)?;
    t.SetUINT32(&MF_MT_AUDIO_NUM_CHANNELS, channels)?;
    t.SetUINT32(&MF_MT_AUDIO_BITS_PER_SAMPLE, 16)?;
    t.SetUINT32(&MF_MT_AUDIO_BLOCK_ALIGNMENT, 2 * channels)?;
    t.SetUINT32(&MF_MT_AUDIO_AVG_BYTES_PER_SECOND, 2 * channels * rate)?;
    t.SetUINT32(&MF_MT_ALL_SAMPLES_INDEPENDENT, 1)?;
    Ok(t)
}

unsafe fn mp3_into(
    sound: &Path,
    part: &Path,
    stop: &AtomicBool,
    progress: &mut dyn FnMut(f64),
) -> windows::core::Result<bool> {
    // Decoded to 16-bit stereo at the MP3's rate: Media Foundation converts.
    let r = reader(sound)?;
    r.SetStreamSelection(AUDIO, true)?;
    let channels = 2;
    r.SetCurrentMediaType(AUDIO, None, &pcm(channels)?)?;
    let secs = secs_of(&r);

    let mut attr = None;
    MFCreateAttributes(&mut attr, 1)?;
    let attr = attr.ok_or_else(windows::core::Error::empty)?;
    attr.SetGUID(&MF_TRANSCODE_CONTAINERTYPE, &MFTranscodeContainerType_MP3)?;
    let writer = MFCreateSinkWriterFromURL(&HSTRING::from(part.as_os_str()), None, &attr)?;
    let mp3 = MFCreateMediaType()?;
    mp3.SetGUID(&MF_MT_MAJOR_TYPE, &MFMediaType_Audio)?;
    mp3.SetGUID(&MF_MT_SUBTYPE, &MFAudioFormat_MP3)?;
    mp3.SetUINT32(&MF_MT_AUDIO_SAMPLES_PER_SECOND, MP3_RATE)?;
    mp3.SetUINT32(&MF_MT_AUDIO_NUM_CHANNELS, channels)?;
    mp3.SetUINT32(&MF_MT_AUDIO_AVG_BYTES_PER_SECOND, MP3_KBPS * 1000 / 8)?;
    let s = writer.AddStream(&mp3)?;
    writer.SetInputMediaType(s, &pcm(channels)?, None)?;
    writer.BeginWriting()?;
    while let Some((t, sample)) = next(&r, AUDIO)? {
        if stop.load(Ordering::Relaxed) {
            return Ok(false);
        }
        writer.WriteSample(s, &sample)?;
        if secs > 0.0 {
            progress((t / secs).clamp(0.0, 1.0));
        }
    }
    writer.Finalize()?;
    Ok(true)
}

/// Takes each decoded piece of sound, with the time it starts at.
pub type Take<'a> = dyn FnMut(f64, &[i16]) -> Result<(), String> + 'a;

/// What Windows said about `source`, in words.
fn unreadable(e: windows::core::Error, source: &str) -> String {
    let name = Path::new(source)
        .file_name()
        .map_or_else(|| source.to_string(), |n| n.to_string_lossy().into_owned());
    match e.code().0 as u32 {
        0xC00D36C4 | 0xC00D36C3 | 0xC00D36B4 | 0xC00D36E6 => format!(
            "Windows cannot play the sound of {name}: it is not a kind of file Windows can \
             read, or it is damaged. Try Open in the default player, in its context menu."
        ),
        0x80070002 | 0x80070003 => format!("{name} could not be found."),
        _ => format!(
            "Windows could not play {name}: {}",
            e.message().trim().trim_end_matches('.')
        ),
    }
}

/// How long `source`, a file or a web address, plays, in seconds; nought if
/// it does not say. For the player, from Video Scribe's media.rs.
pub fn length(source: &str) -> Result<f64, String> {
    unsafe {
        let r = MFCreateSourceReaderFromURL(&HSTRING::from(source), None)
            .map_err(|e| unreadable(e, source))?;
        Ok(secs_of(&r))
    }
}

/// The sound of `source` from `from` seconds to the end, as 16-bit samples
/// at `rate` with `channels`, each piece handed over with the time it starts
/// at. Seeking lands near `from`, not on it. False if `stop` was set. Video
/// Scribe's media.rs, for the player.
pub fn sound_from(
    source: &str,
    from: f64,
    rate: u32,
    channels: u32,
    stop: &AtomicBool,
    take: &mut Take,
) -> Result<bool, String> {
    let fail = |e| unreadable(e, source);
    unsafe {
        let r = MFCreateSourceReaderFromURL(&HSTRING::from(source), None).map_err(fail)?;
        r.SetStreamSelection(ALL, false).map_err(fail)?;
        if r.SetStreamSelection(AUDIO, true).is_err() {
            return Err("It has no sound to play.".into());
        }
        r.SetCurrentMediaType(AUDIO, None, &pcm_at(rate, channels).map_err(fail)?)
            .map_err(fail)?;
        if from > 0.0 {
            r.SetCurrentPosition(
                &GUID::zeroed(),
                &windows::Win32::System::Com::StructuredStorage::PROPVARIANT::from(
                    (from * HNS) as i64,
                ),
            )
            .map_err(fail)?;
        }
        let mut piece: Vec<i16> = Vec::new();
        while let Some((at, sample)) = next(&r, AUDIO).map_err(fail)? {
            if stop.load(Ordering::Relaxed) {
                return Ok(false);
            }
            let buf = sample.ConvertToContiguousBuffer().map_err(fail)?;
            let mut p = std::ptr::null_mut();
            let mut len = 0u32;
            buf.Lock(&mut p, None, Some(&mut len)).map_err(fail)?;
            let bytes = std::slice::from_raw_parts(p, len as usize);
            piece.clear();
            piece.extend(
                bytes
                    .as_chunks::<2>()
                    .0
                    .iter()
                    .map(|b| i16::from_le_bytes(*b)),
            );
            let _ = buf.Unlock();
            take(at, &piece)?;
        }
        Ok(true)
    }
}

/// How long the file at `path` plays, in seconds.
#[cfg(test)]
pub fn duration(path: &Path) -> Option<f64> {
    unsafe {
        let r = MFCreateSourceReaderFromURL(&HSTRING::from(path.as_os_str()), None).ok()?;
        Some(secs_of(&r))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Needs VF_PICTURE, VF_SOUND and VF_OUT (an .mp4 to write):
    /// `cargo test -- --ignored joins`.
    #[test]
    #[ignore]
    fn joins_picture_and_sound_and_makes_an_mp3() {
        let var = |n: &str| std::path::PathBuf::from(std::env::var_os(n).expect(n));
        let _s = Session::start().unwrap();
        let stop = AtomicBool::new(false);
        let picture = var("VF_PICTURE");
        let shift = crate::ytdlp::drop_edit_list(&picture).unwrap();
        let out = var("VF_OUT");
        assert!(join(
            Some((&picture, shift)),
            &var("VF_SOUND"),
            &out,
            &stop,
            &mut |_| {}
        )
        .unwrap());
        let sound = duration(&var("VF_SOUND")).unwrap();
        let got = duration(&out).unwrap();
        assert!((sound - got).abs() < 0.5, "sound {sound} s, joined {got} s");
        let m4a = out.with_extension("m4a");
        assert!(join(None, &var("VF_SOUND"), &m4a, &stop, &mut |_| {}).unwrap());
        assert!(sound_is_aac(&m4a));
        let mp3 = out.with_extension("mp3");
        assert!(super::mp3(&var("VF_SOUND"), &mp3, &stop, &mut |_| {}).unwrap());
        let got = duration(&mp3).unwrap();
        assert!((sound - got).abs() < 0.5, "sound {sound} s, MP3 {got} s");
    }
}
