//! A video's chapters (log entry 3, idea 6), read the way YouTube reads
//! them: from the times at the start of lines in its description, such as
//! "0:00 Intro", "4:12 - The plan". YouTube shows chapters only when the
//! first is at 0:00 and there are at least three, and so does VidFetch.

#[derive(Clone, Debug, PartialEq)]
pub struct Chapter {
    pub at: f64,
    pub name: String,
}

/// "4:12" or "1:02:05" as seconds.
fn seconds(t: &str) -> Option<f64> {
    let parts: Vec<&str> = t.split(':').collect();
    if !(2..=3).contains(&parts.len()) || parts.iter().any(|p| p.is_empty() || p.len() > 2) {
        return None;
    }
    let mut total = 0u64;
    for (i, p) in parts.iter().enumerate() {
        let n: u64 = p.parse().ok()?;
        if i > 0 && n >= 60 {
            return None;
        }
        total = total * 60 + n;
    }
    Some(total as f64)
}

/// The chapters in a description, or none.
pub fn from_description(text: &str) -> Vec<Chapter> {
    let mut out: Vec<Chapter> = Vec::new();
    for line in text.lines() {
        let line = line
            .trim()
            .trim_start_matches(['(', '[', '•', '-', '*', '▶'])
            .trim();
        let time: String = line
            .chars()
            .take_while(|c| c.is_ascii_digit() || *c == ':')
            .collect();
        let Some(at) = seconds(&time) else { continue };
        let name = line[time.len()..]
            .trim_start_matches([')', ']'])
            .trim()
            .trim_start_matches(['-', '–', '—', ':', '|', '.'])
            .trim()
            .to_string();
        if out.last().is_some_and(|c| c.at >= at) {
            continue;
        }
        out.push(Chapter {
            at,
            name: if name.is_empty() {
                format!("Chapter {}", out.len() + 1)
            } else {
                name
            },
        });
    }
    if out.len() < 3 || out[0].at != 0.0 {
        return Vec::new();
    }
    out
}

/// Which chapter `at` is in.
pub fn at(chapters: &[Chapter], at: f64) -> Option<usize> {
    chapters.iter().rposition(|c| c.at <= at + 0.5)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn chapters_come_from_the_description() {
        let d = "A film.\n\nChapters:\n0:00 Intro\n1:05 - The plan\n(12:30) Escape\n1:02:05 | Credits\n\nhttps://x 3:00 later";
        let c = from_description(d);
        assert_eq!(c.len(), 4, "{c:?}");
        assert_eq!(
            c[1],
            Chapter {
                at: 65.0,
                name: "The plan".into()
            }
        );
        assert_eq!(c[2].name, "Escape");
        assert_eq!(c[3].at, 3725.0);
        assert_eq!(at(&c, 70.0), Some(1));
        assert_eq!(at(&c, 0.0), Some(0));
    }

    #[test]
    fn no_chapters_without_a_start_at_nought() {
        assert!(from_description("1:00 a\n2:00 b\n3:00 c").is_empty());
        assert!(from_description("0:00 a\n2:00 b").is_empty());
        assert!(from_description("Released 12:00 noon").is_empty());
    }
}
