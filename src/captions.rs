//! A video's captions as an SRT file, which media players show. yt-dlp fetches
//! them as WebVTT or SubRip; without FFmpeg to convert them, they are written
//! out here. The reading is Video Scribe's captions.rs (MIT, same author).
//!
//! Hand-made captions are one cue per line of dialogue. YouTube's automatic
//! captions roll, repeating each line in the next cue, but tag each new line's
//! words with their times (`<00:00:05.040><c> word</c>`): only those tagged
//! lines are kept, or every line would be shown twice.

/// One caption: start and end in seconds, and its text.
type Cue = (f64, f64, String);

/// The cues of a `.vtt` or `.srt` file's text.
pub fn parse(text: &str) -> Vec<Cue> {
    let raw = raw_cues(text.trim_start_matches('\u{feff}'));
    let rolling = raw
        .iter()
        .any(|(_, _, b)| b.iter().any(|l| l.contains("<c>")));
    raw.into_iter()
        .filter_map(|(start, end, body)| {
            let lines: Vec<String> = body
                .iter()
                .filter(|l| !rolling || l.contains("<c>"))
                .map(|l| plain(l))
                .filter(|l| !l.is_empty())
                .collect();
            (end > start && !lines.is_empty()).then(|| (start, end, lines.join("\n")))
        })
        .collect()
}

/// The cues as a SubRip file, with Windows line endings.
pub fn to_srt(cues: &[Cue]) -> String {
    let mut out = String::new();
    for (i, (start, end, text)) in cues.iter().enumerate() {
        out += &format!(
            "{}\r\n{} --> {}\r\n{}\r\n\r\n",
            i + 1,
            stamp(*start),
            stamp(*end),
            text.replace('\n', "\r\n")
        );
    }
    out
}

/// The cues as text to read (log entry 3, idea 8): running on, in
/// paragraphs at each pause of 2 seconds or more, or each minute, each
/// starting with its time, "[1:23]". Repeated lines are said once.
pub fn as_text(cues: &[Cue]) -> String {
    let mut out = String::new();
    let mut started = f64::MIN;
    let mut last_end = f64::MIN;
    let mut last_line = String::new();
    for (start, end, text) in cues {
        let line = text.replace('\n', " ");
        if line == last_line {
            last_end = *end;
            continue;
        }
        if start - last_end >= 2.0 || start - started >= 60.0 {
            if !out.is_empty() {
                out += "\n\n";
            }
            out += &format!("[{}] ", crate::listen::clock(*start));
            started = *start;
        } else {
            out.push(' ');
        }
        out += &line;
        last_line = line;
        last_end = *end;
    }
    out
}

/// Every cue as (start, end, lines), WebVTT or SubRip.
fn raw_cues(text: &str) -> Vec<(f64, f64, Vec<String>)> {
    let mut out = Vec::new();
    let mut lines = text.lines().peekable();
    while let Some(line) = lines.next() {
        let Some((a, b)) = line.split_once("-->") else {
            continue;
        };
        let (Some(s), Some(e)) = (
            time(a.trim()),
            time(b.split_whitespace().next().unwrap_or("")),
        ) else {
            continue;
        };
        let mut body = Vec::new();
        while let Some(l) = lines.peek() {
            if l.is_empty() || l.contains("-->") {
                break;
            }
            body.push(lines.next().unwrap_or_default().to_string());
        }
        out.push((s, e, body));
    }
    out
}

/// A line without its tags, its spare spaces, or WebVTT's escaped characters.
fn plain(line: &str) -> String {
    let mut out = String::new();
    let mut inside = false;
    for c in line.chars() {
        match c {
            '<' => inside = true,
            '>' => inside = false,
            _ if !inside => out.push(c),
            _ => {}
        }
    }
    out.replace("&nbsp;", " ")
        .replace("&lt;", "<")
        .replace("&gt;", ">")
        .replace("&amp;", "&")
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
}

/// `00:01:02.345`, `01:02.345`, or SubRip's `00:01:02,345`, to seconds.
fn time(s: &str) -> Option<f64> {
    let s = s.replace(',', ".");
    let parts: Vec<&str> = s.split(':').collect();
    if parts.len() < 2 {
        return None;
    }
    let mut total: f64 = parts.last()?.parse().ok()?;
    let mut mult = 60.0;
    for p in parts.iter().rev().skip(1) {
        total += p.parse::<f64>().ok()? * mult;
        mult *= 60.0;
    }
    Some(total)
}

/// Seconds as SubRip writes them: `00:01:02,345`.
fn stamp(seconds: f64) -> String {
    let ms = (seconds * 1000.0).round() as u64;
    format!(
        "{:02}:{:02}:{:02},{:03}",
        ms / 3_600_000,
        ms / 60_000 % 60,
        ms / 1000 % 60,
        ms % 1000
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hand_made_captions_keep_their_lines() {
        let vtt =
            "WEBVTT\n\n00:00:01.000 --> 00:00:02.500 align:start\n<i>Hello</i> &amp; welcome\n\
                   second line\n\n01:02.000 --> 01:03.000\n\n00:00:04.000 --> 00:00:05.000\nBye\n";
        let cues = parse(vtt);
        assert_eq!(cues.len(), 2);
        assert_eq!(
            to_srt(&cues),
            "1\r\n00:00:01,000 --> 00:00:02,500\r\nHello & welcome\r\nsecond line\r\n\r\n\
             2\r\n00:00:04,000 --> 00:00:05,000\r\nBye\r\n\r\n"
        );
    }

    #[test]
    fn rolling_automatic_captions_are_not_shown_twice() {
        let vtt = "WEBVTT\n\n\
            00:00:01.000 --> 00:00:03.000 align:start position:0%\n \n\
            hello<00:00:01.500><c> there</c><00:00:02.000><c> friend</c>\n\n\
            00:00:03.000 --> 00:00:03.010 align:start position:0%\nhello there friend\n \n\n\
            00:00:03.010 --> 00:00:05.000 align:start position:0%\nhello there friend\n\
            how<00:00:03.500><c> are</c><00:00:04.000><c> you</c>\n";
        let cues = parse(vtt);
        assert_eq!(cues.len(), 2);
        assert_eq!(cues[0].2, "hello there friend");
        assert_eq!(cues[1].2, "how are you");
    }

    #[test]
    fn subrip_comes_through_and_long_times_are_written_whole() {
        let cues = parse("1\r\n01:00:01,250 --> 01:00:02,000\r\nHi\r\n");
        assert_eq!(cues[0].0, 3601.25);
        assert!(to_srt(&cues).contains("01:00:01,250 --> 01:00:02,000"));
        assert!(parse("WEBVTT\n").is_empty());
    }

    #[test]
    fn captions_read_as_text_in_paragraphs() {
        let cues = vec![
            (0.0, 2.0, "Hello".to_string()),
            (2.0, 4.0, "there.".to_string()),
            (4.0, 5.0, "there.".to_string()),
            (9.0, 10.0, "After a\npause".to_string()),
        ];
        assert_eq!(
            as_text(&cues),
            "[0:00] Hello there.\n\n[0:09] After a pause"
        );
    }
}
