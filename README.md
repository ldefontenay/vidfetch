# VidFetch

An accessible interface for YouTube, built for screen reader users first: search, listen, follow
channels without an account, and download. It can also download videos and whole playlists from
many other video sites.

A single Windows program, built from the parts of [Video Scribe](https://github.com/ldefontenay/videoscribe).
No installer, no FFmpeg, no runtime to install.

## Download

**[Download vidfetch.exe][exe]** or **[vidfetch.zip][zip]** (the same, zipped, which Microsoft Edge
objects to less). Windows 10 or 11, 64-bit. This is a beta.

Nothing to install: put it in a folder of its own and run it. It fetches
[yt-dlp](https://github.com/yt-dlp/yt-dlp) itself the first time, after asking. **The
[VidFetch guide](https://ldefontenay.github.io/vidfetch/userguide/) walks you through it.**

VidFetch is not signed with a code-signing certificate, so Windows SmartScreen may show "Windows
protected your PC" the first time; More info, then Run anyway, gets past it, once. Download it only
from [this repository's releases page][releases].

[exe]: https://github.com/ldefontenay/vidfetch/releases/latest/download/vidfetch.exe
[zip]: https://github.com/ldefontenay/vidfetch/releases/latest/download/vidfetch.zip
[releases]: https://github.com/ldefontenay/vidfetch/releases

Tested with JAWS, NVDA and Narrator. VidFetch is not connected with YouTube or Google. Only download
what you have the right to.

## What it does

- **New download, Ctrl+N**: copy a video's or a playlist's address in the browser, press Ctrl+N, then
  Enter. The address is taken from the clipboard.
- **Sound only (M4A or MP3) to begin with, or video (MP4)**, with the picture at 360 to 1080 lines.
- **A built-in player**: Enter on a download plays its sound at once, carrying on where you stopped,
  with volume, speed (voices keep their pitch), the next one, and a YouTube video's chapters.
- **Search YouTube, Ctrl+F**, and listen to what it finds before downloading it, in a window of its
  own.
- **Follow channels** without an account: Ctrl+L lists what is new from them. A channel's playlists
  can be listed, played and downloaded.
- **Categories**, as folders, with the downloads that are in none in Uncategorised.
- **As a YouTube interface**: search for videos, channels or playlists with filters, related videos,
  comments and transcripts to read with the arrow keys, a channel's search and older videos, and import
  of your subscriptions from Google Takeout, FreeTube or NewPipe.
- **Read** a video's description, or its captions as text; **Recently played**, Ctrl+H.
- **A queue**: Ctrl+N while one download runs adds the next; Ctrl+Q shows what is waiting, by title.
  What has not finished is kept when VidFetch closes, and offered again when it starts.
- **Captions**, as an option, saved beside each download as an SRT file.
- **Already have it?** Asking for the same video at the same quality asks before a second copy.
- **Playlists and channels** go into a folder of their own, numbered in the playlist's order.
  Enter on one shows its videos in the list. **Update the playlist, Ctrl+U**, fetches only the
  videos added since.
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
