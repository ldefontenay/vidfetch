//! Tags in each download, so that a media player or File Explorer can tell
//! what it is: written through the Windows property system, which writes
//! MP3's ID3 tags and MP4's own. From Video Scribe's tags.rs.

use std::path::Path;

use windows::core::{HSTRING, PCWSTR, PWSTR};
use windows::Win32::Foundation::PROPERTYKEY;
use windows::Win32::System::Com::StructuredStorage::*;
use windows::Win32::System::Variant::*;
use windows::Win32::UI::Shell::PropertiesSystem::*;

/// What goes in a download's tags.
#[derive(Default)]
pub struct Tags {
    pub title: String,
    /// The channel or person who put it online.
    pub artist: Option<String>,
    /// The playlist it came from.
    pub album: Option<String>,
    /// Its place in the playlist.
    pub track: Option<u32>,
    pub year: Option<u32>,
    /// The address it was downloaded from, for Explorer's Comments.
    pub comment: String,
}

enum Value<'a> {
    Text(&'a str),
    List(&'a str),
    Number(u32),
}

/// Writes `tags` into the file at `path`. A property the file's handler
/// refuses is left out, not a failure: MP3 and MP4 take them all.
pub fn write(path: &Path, tags: &Tags) -> Result<(), String> {
    let explain = |e: windows::core::Error| e.message().trim().to_string();
    unsafe {
        let store: IPropertyStore = SHGetPropertyStoreFromParsingName(
            &HSTRING::from(path.as_os_str()),
            None,
            GPS_READWRITE,
        )
        .map_err(explain)?;
        let mut props = vec![
            ("System.Title", Value::Text(&tags.title)),
            ("System.Comment", Value::Text(&tags.comment)),
        ];
        if let Some(a) = &tags.artist {
            props.push(("System.Music.Artist", Value::List(a)));
        }
        if let Some(a) = &tags.album {
            props.push(("System.Music.AlbumTitle", Value::Text(a)));
        }
        if let Some(n) = tags.track {
            props.push(("System.Music.TrackNumber", Value::Number(n)));
        }
        if let Some(y) = tags.year {
            props.push(("System.Media.Year", Value::Number(y)));
        }
        for (name, value) in props {
            let mut key = PROPERTYKEY::default();
            if PSGetPropertyKeyFromName(&HSTRING::from(name), &mut key).is_err() {
                continue;
            }
            let mut pv = match value {
                Value::Text(s) => {
                    let mut pv = PROPVARIANT::default();
                    let s: PWSTR =
                        windows::Win32::UI::Shell::SHStrDupW(&HSTRING::from(s)).map_err(explain)?;
                    let inner = &mut *pv.Anonymous.Anonymous;
                    inner.vt = VT_LPWSTR;
                    inner.Anonymous.pwszVal = s;
                    pv
                }
                Value::List(s) => {
                    let h = HSTRING::from(s);
                    InitPropVariantFromStringVector(Some(&[PCWSTR(h.as_ptr())])).map_err(explain)?
                }
                Value::Number(n) => {
                    let mut pv = PROPVARIANT::default();
                    let inner = &mut *pv.Anonymous.Anonymous;
                    inner.vt = VT_UI4;
                    inner.Anonymous.ulVal = n;
                    pv
                }
            };
            let _ = store.SetValue(&key, &pv);
            let _ = PropVariantClear(&mut pv);
        }
        store.Commit().map_err(explain)
    }
}
