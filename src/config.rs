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
                "download as" => {
                    cfg.format = match lower.as_str() {
                        "video" => Format::Video,
                        "mp3" => Format::Mp3,
                        _ => Format::M4a,
                    }
                }
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
             save captions = {}\r\n",
            self.output_folder
                .as_ref()
                .map(|p| p.display().to_string())
                .unwrap_or_default(),
            self.format.word(),
            self.lines,
            yn(self.number),
            yn(self.captions),
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
