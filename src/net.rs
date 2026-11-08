//! HTTPS through WinHTTP, which is part of Windows: no TLS crate to ship.
//! Synchronous; call it from the worker thread, never the UI thread.
//!
//! A file download: yt-dlp, from its release on GitHub. From Video Scribe's
//! net.rs, as it came from Photo Scribe's.

use std::sync::atomic::{AtomicBool, Ordering};

use windows::core::{HSTRING, PCWSTR};
use windows::Win32::Networking::WinHttp::*;

const AGENT: &str = concat!(
    "VidFetch/",
    env!("CARGO_PKG_VERSION"),
    " (Windows app that downloads videos)"
);

/// Closes a WinHTTP handle when dropped.
struct Handle(*mut core::ffi::c_void);

impl Drop for Handle {
    fn drop(&mut self) {
        if !self.0.is_null() {
            unsafe {
                let _ = WinHttpCloseHandle(self.0);
            }
        }
    }
}

fn last_error(what: &str) -> String {
    let code = unsafe { windows::Win32::Foundation::GetLastError() }.0;
    let e = windows::core::Error::from_hresult(windows::core::HRESULT::from_win32(code));
    let plain = match code {
        12002 => "the server took too long to answer".to_string(),
        12007 => {
            "the server's name could not be found; is the internet connection working?".to_string()
        }
        12029 | 12030 => "the connection to the server failed or was dropped".to_string(),
        12175 => "the secure connection could not be set up".to_string(),
        _ => e.message().trim().to_string(),
    };
    format!("{what}: {plain} (WinHTTP error {code})")
}

/// A file from `https://{host}{path}`, following redirects, reporting (bytes
/// so far, total or 0) as it arrives. None if `stop` is set on the way.
pub fn download(
    host: &str,
    path: &str,
    stop: &AtomicBool,
    progress: &mut dyn FnMut(u64, u64),
) -> Result<Option<Vec<u8>>, String> {
    request(host, path, None, stop, progress)
}

/// JSON sent to `https://{host}{path}`, and the answer: YouTube's own
/// search and details (youtube.rs). None if `stop` is set on the way.
pub fn post_json(
    host: &str,
    path: &str,
    body: &str,
    stop: &AtomicBool,
) -> Result<Option<Vec<u8>>, String> {
    request(host, path, Some(body.as_bytes()), stop, &mut |_, _| {})
}

/// A GET, or a POST of JSON when there is a `body`.
fn request(
    host: &str,
    path: &str,
    body: Option<&[u8]>,
    stop: &AtomicBool,
    progress: &mut dyn FnMut(u64, u64),
) -> Result<Option<Vec<u8>>, String> {
    unsafe {
        let session = Handle(WinHttpOpen(
            &HSTRING::from(AGENT),
            WINHTTP_ACCESS_TYPE_AUTOMATIC_PROXY,
            PCWSTR::null(),
            PCWSTR::null(),
            0,
        ));
        if session.0.is_null() {
            return Err(last_error("Could not start WinHTTP"));
        }
        let _ = WinHttpSetTimeouts(session.0, 30_000, 30_000, 60_000, 60_000);
        let connect = Handle(WinHttpConnect(session.0, &HSTRING::from(host), 443, 0));
        if connect.0.is_null() {
            return Err(last_error(&format!("Could not connect to {host}")));
        }
        let req = Handle(WinHttpOpenRequest(
            connect.0,
            &HSTRING::from(if body.is_some() { "POST" } else { "GET" }),
            &HSTRING::from(path),
            PCWSTR::null(),
            PCWSTR::null(),
            std::ptr::null(),
            WINHTTP_FLAG_SECURE,
        ));
        if req.0.is_null() {
            return Err(last_error("Could not create the request"));
        }
        let sent = match body {
            Some(b) => {
                let headers: Vec<u16> = "Content-Type: application/json".encode_utf16().collect();
                WinHttpSendRequest(
                    req.0,
                    Some(&headers),
                    Some(b.as_ptr() as *const core::ffi::c_void),
                    b.len() as u32,
                    b.len() as u32,
                    0,
                )
            }
            None => WinHttpSendRequest(req.0, None, None, 0, 0, 0),
        };
        sent.map_err(|_| last_error(&format!("Could not send the request to {host}")))?;
        WinHttpReceiveResponse(req.0, std::ptr::null_mut())
            .map_err(|_| last_error(&format!("No answer from {host}")))?;
        let number = |query| {
            let mut n = 0u32;
            let mut size = 4u32;
            let _ = WinHttpQueryHeaders(
                req.0,
                query | WINHTTP_QUERY_FLAG_NUMBER,
                PCWSTR::null(),
                Some(&mut n as *mut u32 as _),
                &mut size,
                std::ptr::null_mut(),
            );
            n
        };
        let status = number(WINHTTP_QUERY_STATUS_CODE);
        if status != 200 {
            return Err(format!("{host} answered with status {status}"));
        }
        let total = number(WINHTTP_QUERY_CONTENT_LENGTH) as u64;
        let mut data = Vec::with_capacity(total as usize);
        loop {
            if stop.load(Ordering::Relaxed) {
                return Ok(None);
            }
            let mut available = 0u32;
            WinHttpQueryDataAvailable(req.0, &mut available)
                .map_err(|_| last_error(&format!("The download from {host} was cut off")))?;
            if available == 0 {
                break;
            }
            let start = data.len();
            data.resize(start + available as usize, 0);
            let mut read = 0u32;
            WinHttpReadData(req.0, data[start..].as_mut_ptr() as _, available, &mut read)
                .map_err(|_| last_error(&format!("The download from {host} was cut off")))?;
            data.truncate(start + read as usize);
            progress(data.len() as u64, total);
        }
        Ok(Some(data))
    }
}

/// SHA-256 through CNG, which is part of Windows.
pub fn sha256(data: &[u8]) -> [u8; 32] {
    use windows::Win32::Security::Cryptography::{BCryptHash, BCRYPT_SHA256_ALG_HANDLE};
    let mut out = [0u8; 32];
    unsafe {
        let _ = BCryptHash(BCRYPT_SHA256_ALG_HANDLE, None, data, &mut out);
    }
    out
}

pub fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sha256_matches_the_standard() {
        assert_eq!(
            hex(&sha256(b"abc")),
            "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad"
        );
    }
}
