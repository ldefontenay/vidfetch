# VidFetch log

Newest entry at the top. Each question is followed by an `ANSWER:` line.

## Entry 1 — VidFetch built: videos, sound only, playlists (2026-11-02)

You asked for "VidFetch, a video downloader using YT-DLP ... Basically a subset of VideoScribe without
the description element, also able to handle Youtube playlists". It is built, in
`C:\Users\ldefo\dev\VidFetch`, as its own program and its own git repository (not pushed anywhere yet).
The test is `test-milestone-1.txt`; it costs nothing, as nothing is sent to any paid service.

### What it does

- **New download, Ctrl+N.** Copy an address in the browser (Alt+D, Ctrl+C), press Ctrl+N in VidFetch:
  the address is already in the box, so Enter starts. The window also has Download as and Picture
  quality, and a "What will happen" box that says what the address is and where it will be saved.
- **Video (MP4), or sound only (M4A or MP3).** YouTube sends picture and sound separately. As in Video
  Scribe, there is no FFmpeg: Windows' own Media Foundation puts the two together, copying both as they
  are, so nothing is lost and it takes a second or two. M4A is YouTube's sound exactly as sent. MP3 is
  converted, for players that take nothing else.
- **Playlists and channels.** Every video goes into a folder named after the playlist, numbered in its
  order: "03 - Title.mp4". A video that cannot be downloaded (private, removed) does not stop the rest;
  the report at the end lists it with the reason.
- **Update the playlist, Ctrl+U.** The playlist's folder remembers which videos are there, so Ctrl+U on
  it later downloads only the new ones. Stopping with Escape keeps everything already saved, and Ctrl+U
  carries on.
- **A video in a playlist.** An address with both a video and a playlist in it (as when a video is
  opened from a playlist's page) brings a question: the whole playlist, or just this video. Just the
  video is the default. YouTube's "mixes", the endless lists it makes for a video, are never taken as
  playlists.
- **Tags.** Every file carries its title, the channel as the artist, the year, and the address it came
  from; a playlist's files also the playlist as the album and their place as the track number, so a
  music player keeps them together and in order.
- **The list of downloads** is Video Scribe's list: most recent first, Enter opens (plays a file, or opens
  a playlist's folder), F2 renames, Delete moves to the Recycle Bin, the Applications key gives the menu.

The program is 460 KB. yt-dlp is fetched the first time, as in Video Scribe, into VidFetch's own folder.

### Tried by me

- **"Me at the zoo"** (19 seconds, YouTube's first video): as video in 10 seconds, including fetching
  yt-dlp, then as M4A. Length, title, channel, year and address all correct in the file.
- **Blender's "Open Movies" playlist** (18 videos), as M4A, stopped as the third began: 2 saved, and the
  folder's record listed both. Then fetched again as MP3, which started at the third, named it
  "03 - ...", and tagged it with the album "Blender Open Movies", track 3. I stopped it there: the films
  are long.
- **A direct link to a video file** (the Sintel trailer on download.blender.org): saved as it is.
- **An archive.org page** (His Girl Friday, 92 minutes, 575 MB) as MP3: 17 minutes in
  all, most of it the download, as archive.org has no separate sound to fetch. The MP3 is the whole
  91.6 minutes, at 192 kbps, tagged.
- **Through the window**: Ctrl+N with an address on the clipboard, Enter, and the download appeared at
  the top of the list, chosen, with "Downloaded: Me at the zoo 2.mp4. Press Enter to open it." The
  accessibility probe found no problems in the main window or the New download window.

### Found on the way, which affects Video Scribe too

yt-dlp writes titles in Windows' old code page when its output goes to another program, so a title with
an accent, such as "Les Misérables", did not arrive as proper text. In Video Scribe that would stop a
YouTube download of any video whose title has an accent, which matters now that French films are in
reach. The fix is one option for yt-dlp (`--encoding utf-8`); VidFetch has it, and accented titles now
come through: "Retour à la réalité" was saved under that name, tagged with the channel "Café Crème". The same fix is now in Video Scribe's code, on its m11-languages branch, but I have not rebuilt the
Video Scribe program you are testing, so nothing changes under you mid-test. It goes in with the next
build.

### Not done, and why

- **Signing in** (private, members-only or age-restricted videos): VidFetch cannot, as Video Scribe
  cannot. yt-dlp could use the browser's cookies, but that reaches into the browser's saved logins,
  which I would not do without asking.
- **Live streams** while live, and sites that only offer a stream in pieces (which needs FFmpeg).
  VidFetch says so when that is the reason.
- **More than 1080 lines.** YouTube offers the kind of picture an MP4 holds (H.264) only up to 1080.
  Higher would be VP9 or AV1 in WebM, which Media Foundation cannot put into an MP4.

### Questions

**Q1.** Defaults for a new user: video, MP4, at 720 lines, and playlist files numbered. Is 720 right,
or would you rather 1080 (sharper, about twice the size)?

ANSWER: 720 is right, but default should be to download sound only when possible.

**Q2.** Downloading the same single video twice now saves a second copy, "Me at the zoo 2.mp4". Should
VidFetch notice instead, and say "Already downloaded as Me at the zoo.mp4: download it again, or open
it?" (A playlist already only fetches what is missing.)

ANSWER: The latter, unless the quality setting changed.

**Q3.** During a playlist, VidFetch speaks when it starts and when it finishes; each video's progress
("Video 3 of 18, title: downloading the sound, 45%") is in the Status box (Alt+T), not spoken. Would you
like each new video announced as it starts?

ANSWER: No, just showing it in the status line is fine.

**Q4.** A queue: should you be able to start a second download while one is running, so it waits its
turn? At the moment VidFetch says "Already downloading" and you wait.

ANSWER: Please implement a queue.

**Q5.** Captions: should VidFetch be able to save a video's captions as a file beside it (SRT, which
media players show)? Not useful to you directly, perhaps, but to someone you share a video with.

ANSWER: Make that an option.

**Q6.** Publishing: should VidFetch become a GitHub repository (private, like Video Scribe), and later a
release like NoteFetch's, with its guide online? I have pushed nothing.

ANSWER: Yes.


---
Question: What is the impact of not signing in? If that makes most videos available, I'd like that option.