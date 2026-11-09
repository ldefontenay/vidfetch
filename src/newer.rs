//! Whether a newer VidFetch has come out on GitHub: it tells, and installs
//! nothing (log entry 6, A2).

use std::sync::atomic::AtomicBool;

use crate::net;

/// The newest release, betas included.
const PATH: &str = "/repos/ldefontenay/vidfetch/releases?per_page=1";

/// The newest release's tag, such as "v0.7.0-beta.1"; None if there is none.
pub fn latest(stop: &AtomicBool) -> Result<Option<String>, String> {
    let Some(answer) = net::download("api.github.com", PATH, stop, &mut |_, _| {})? else {
        return Ok(None);
    };
    let v: serde_json::Value =
        serde_json::from_slice(&answer).map_err(|e| format!("GitHub's answer: {e}"))?;
    Ok(v[0]["tag_name"].as_str().map(str::to_string))
}

/// 1.2.3 as numbers, and which beta: a release is later than all its betas.
fn rank(version: &str) -> (Vec<u64>, u64) {
    let version = version.trim().trim_start_matches(['v', 'V']);
    let (main, beta) = match version.split_once('-') {
        Some((main, beta)) => {
            let n = beta
                .rsplit('.')
                .next()
                .and_then(|n| n.parse().ok())
                .unwrap_or(0);
            (main, n)
        }
        None => (version, u64::MAX),
    };
    let mut numbers: Vec<u64> = main.split('.').map(|n| n.parse().unwrap_or(0)).collect();
    numbers.resize(3, 0);
    (numbers, beta)
}

pub fn is_newer(tag: &str, mine: &str) -> bool {
    rank(tag) > rank(mine)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn versions_compare_as_numbers() {
        assert!(is_newer("v0.7.0-beta.2", "0.7.0-beta.1"));
        assert!(is_newer("v0.7.0-beta.10", "0.7.0-beta.9"));
        assert!(is_newer("v0.7.0", "0.7.0-beta.9"));
        assert!(is_newer("v0.10.0-beta.1", "0.9.0"));
        assert!(!is_newer("v0.7.0-beta.1", "0.7.0-beta.1"));
        assert!(!is_newer("v0.6.0", "0.7.0-beta.1"));
    }
}
