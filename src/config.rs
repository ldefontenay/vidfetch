//! Remembered settings: `%APPDATA%\VidFetch\settings.txt`, one `key = value`
//! per line, so it can be read and corrected in Notepad (Video Scribe's and
//! Photo Scribe's format).

use std::path::PathBuf;

#[derive(Clone, Debug, PartialEq)]
pub struct Config {
    /// Where downloads are saved. None: `Videos\VidFetch`.
    pub output_folder: Option<PathBuf>,
    pub format: Format,
    /// Most lines of picture: 360, 480, 720 or 1080.
    pub lines: u32,
    /// A playlist's files start with their place in it: "03 - Title.mp4".
    pub number: bool,
    /// Save a video's captions as an SRT file beside it, when it has any.
    pub captions: bool,
    /// How fast the Play window plays: 1.0 is normal.
    pub speed: f64,
    /// At the end of one, the Play window goes on to the next in the list.
    pub play_next: bool,
    /// The Play window cuts every silence down to a second (log entry 4, Q2).
    pub cut_silence: bool,
    /// A web address copied anywhere is offered for downloading as VidFetch
    /// comes to the front (log entry 4, Q3).
    pub offer_copied: bool,
    /// The category the list shows, and new downloads go into: None for all
    /// of them, with new downloads in Uncategorised.
    pub category: Option<String>,
    /// When VidFetch last looked for a newer version of itself, in seconds
    /// since 1970.
    pub looked_for_newer: i64,
}

/// What a download is saved as.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Format {
    /// Picture and sound, as an MP4.
    Video,
    /// The sound alone, as the site sends it: AAC in an M4A, nothing lost.
    M4a,
    /// The sound alone, as an MP3, which plays everywhere.
    Mp3,
}

/// The choices, as the menus and the New download window say them.
pub const FORMATS: &[(Format, &str)] = &[
    (
        Format::M4a,
        "Sound only, as the site sends it, nothing lost (M4A)",
    ),
    (Format::Mp3, "Sound only, for any player (MP3)"),
    (Format::Video, "Video, with picture and sound (MP4)"),
];

impl Format {
    /// The word for it in the settings file and the record of downloads.
    pub fn word(self) -> &'static str {
        match self {
            Format::Video => "video",
            Format::M4a => "m4a",
            Format::Mp3 => "mp3",
        }
    }

    /// The format a word names; M4A for anything else.
    pub fn from_word(word: &str) -> Format {
        match word.trim().to_ascii_lowercase().as_str() {
            "video" => Format::Video,
            "mp3" => Format::Mp3,
            _ => Format::M4a,
        }
    }
}

/// YouTube offers H.264, which an MP4 holds, up to 1080 lines.
pub const QUALITIES: &[(u32, &str)] = &[
    (360, "Low, 360 lines: small files"),
    (480, "Standard, 480 lines"),
    (720, "High, 720 lines"),
    (1080, "Full HD, 1080 lines: large files"),
];

impl Default for Config {
    fn default() -> Self {
        Config {
            output_folder: None,
            // The user's choice (log entry 1, Q1): sound only unless asked.
            format: Format::M4a,
            lines: 720,
            number: true,
            captions: false,
            speed: 1.0,
            play_next: false,
            cut_silence: false,
            offer_copied: false,
            looked_for_newer: 0,
            category: None,
        }
    }
}

impl Config {
    fn path() -> Option<PathBuf> {
        let appdata = PathBuf::from(std::env::var_os("APPDATA")?);
        Some(appdata.join("VidFetch").join("settings.txt"))
    }

    /// `%APPDATA%\VidFetch`, where settings and the help page live.
    pub fn dir() -> Option<PathBuf> {
        Self::path().and_then(|p| p.parent().map(PathBuf::from))
    }

    /// What waits in the queue (queue.rs).
    pub fn queue_path() -> Option<PathBuf> {
        Some(Self::dir()?.join("queue.json"))
    }

    /// The record of single downloads (library.rs, Past).
    pub fn past_path() -> Option<PathBuf> {
        Self::dir().map(|d| d.join("downloaded.json"))
    }

    /// False the very first time VidFetch runs on this computer.
    pub fn exists() -> bool {
        Self::path().is_some_and(|p| p.exists())
    }

    pub fn load() -> Config {
        let text = Self::path()
            .and_then(|p| std::fs::read_to_string(p).ok())
            .unwrap_or_default();
        Self::parse(&text)
    }

    /// Where downloads go when no folder has been chosen.
    pub fn default_output_folder() -> PathBuf {
        let home =
            std::env::var_os("USERPROFILE").map_or_else(|| PathBuf::from("."), PathBuf::from);
        home.join("Videos").join("VidFetch")
    }

    pub fn output_folder(&self) -> PathBuf {
        self.output_folder
            .clone()
            .unwrap_or_else(Self::default_output_folder)
    }

    pub fn parse(text: &str) -> Config {
        let mut cfg = Config::default();
        // Notepad can save with a byte order mark, which would hide the first key.
        for line in text.trim_start_matches('\u{feff}').lines() {
            let Some((key, value)) = line.split_once('=') else {
                continue;
            };
            let (key, value) = (key.trim(), value.trim());
            let lower = value.to_ascii_lowercase();
            let yes = matches!(lower.as_str(), "yes" | "true" | "1" | "on");
            match key {
                "output folder" if !value.is_empty() => {
                    cfg.output_folder = Some(PathBuf::from(value))
                }
                "download as" => cfg.format = Format::from_word(&lower),
                "lines of picture" => {
                    if let Ok(n) = value.parse::<u32>() {
                        cfg.lines = QUALITIES
                            .iter()
                            .map(|q| q.0)
                            .min_by_key(|l| l.abs_diff(n))
                            .unwrap_or(720);
                    }
                }
                "number the files of a playlist" => cfg.number = yes,
                "save captions" => cfg.captions = yes,
                "playing speed" => {
                    if let Ok(x) = value.parse::<f64>() {
                        cfg.speed = x.clamp(crate::player::SLOWEST, crate::player::FASTEST);
                    }
                }
                "play the next one at the end" => cfg.play_next = yes,
                "cut silences" => cfg.cut_silence = yes,
                "offer copied web addresses" => cfg.offer_copied = yes,
                "category shown" if !value.is_empty() => cfg.category = Some(value.to_string()),
                "looked for a newer version" => cfg.looked_for_newer = value.parse().unwrap_or(0),
                _ => {}
            }
        }
        cfg
    }

    pub fn to_text(&self) -> String {
        let yn = |b: bool| if b { "yes" } else { "no" };
        format!(
            "# VidFetch settings. Safe to edit while VidFetch is closed.\r\n\
             output folder = {}\r\n\
             download as = {}\r\n\
             lines of picture = {}\r\n\
             number the files of a playlist = {}\r\n\
             save captions = {}\r\n\
             playing speed = {}\r\n\
             play the next one at the end = {}\r\n\
             cut silences = {}\r\n\
             offer copied web addresses = {}\r\n\
             category shown = {}\r\n\
             looked for a newer version = {}\r\n",
            self.output_folder
                .as_ref()
                .map(|p| p.display().to_string())
                .unwrap_or_default(),
            self.format.word(),
            self.lines,
            yn(self.number),
            yn(self.captions),
            self.speed,
            yn(self.play_next),
            yn(self.cut_silence),
            yn(self.offer_copied),
            self.category.as_deref().unwrap_or_default(),
            self.looked_for_newer,
        )
    }

    pub fn save(&self) {
        let Some(path) = Self::path() else { return };
        if let Some(dir) = path.parent() {
            let _ = std::fs::create_dir_all(dir);
        }
        let _ = std::fs::write(path, self.to_text());
    }

    /// The format chosen, as said.
    pub fn format_label(&self) -> &'static str {
        FORMATS
            .iter()
            .find(|f| f.0 == self.format)
            .map_or("", |f| f.1)
    }

    /// Where new downloads go: the category shown, else Uncategorised.
    pub fn download_folder(&self) -> PathBuf {
        let category = self
            .category
            .clone()
            .unwrap_or_else(|| crate::library::UNCATEGORISED.to_string());
        self.output_folder().join(category)
    }

    /// The picture quality chosen, as said.
    pub fn quality_label(&self) -> &'static str {
        QUALITIES
            .iter()
            .find(|q| q.0 == self.lines)
            .map_or("", |q| q.1)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn round_trips() {
        let cfg = Config {
            output_folder: Some(PathBuf::from(r"D:\Downloads")),
            format: Format::Mp3,
            lines: 1080,
            number: false,
            captions: true,
            speed: 1.75,
            play_next: true,
            cut_silence: true,
            offer_copied: true,
            looked_for_newer: 0,
            category: Some("Talks".into()),
        };
        assert_eq!(Config::parse(&cfg.to_text()), cfg);
    }

    #[test]
    fn empty_file_gives_defaults() {
        assert_eq!(Config::parse(""), Config::default());
    }

    #[test]
    fn a_hand_typed_quality_goes_to_the_nearest() {
        assert_eq!(Config::parse("lines of picture = 700\n").lines, 720);
        assert_eq!(Config::parse("lines of picture = 2160\n").lines, 1080);
        assert_eq!(
            Config::parse("\u{feff}download as = M4A\r\n").format,
            Format::M4a
        );
    }
}
