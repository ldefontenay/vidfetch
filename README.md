# VidFetch

Download videos, and whole playlists, from YouTube and many other sites, as video or as sound
only, with a window built for screen reader users first.

A single Windows program, built from the parts of [Video Scribe](https://github.com/ldefontenay/videoscribe).
No installer, no FFmpeg, no runtime to install.

- **New download, Ctrl+N**: copy a video's or a playlist's address in the browser, press Ctrl+N, then
  Enter. The address is taken from the clipboard.
- **Sound only (M4A or MP3) to begin with, or video (MP4)**, with the picture at 360 to 1080 lines.
- **A queue**: Ctrl+N while one download runs adds the next; Ctrl+Q shows what is waiting.
- **Captions**, as an option, saved beside each download as an SRT file.
- **Already have it?** Asking for the same video at the same quality offers to open what you have.
- **Playlists and channels** go into a folder of their own, numbered in the playlist's order.
  **Update the playlist, Ctrl+U**, fetches only the videos added since.
- Each file is tagged with its title, channel, year and address, and a playlist's with its album and
  track number.

Downloading is done by [yt-dlp](https://github.com/yt-dlp/yt-dlp), which VidFetch fetches from its
official release the first time, checks against its published checksum, and keeps up to date. YouTube's
separate picture and sound are put together by Windows' own Media Foundation, copied as they are.

Windows 10 or 11, 64-bit. F1 in the program opens the help.

Only download what you have the right to.

## Building

Rust stable with the MSVC toolchain, and the Windows SDK for rc.exe.

```
cargo build --release
```

MIT licensed.
