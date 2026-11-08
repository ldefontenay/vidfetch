# VidFetch plan

## 1. What it is

An accessible interface for YouTube, which can act as a downloader for other video sites (the user,
milestone 5 test: "It has now evolved to something far more useful, i.e. an accessible interface for
Youtube."). It began as a downloader built from Video Scribe's parts (2 November 2026: "a subset of
VideoScribe without the description element, also able to handle Youtube playlists"). One small
Win32 program, a screen reader first, no installer.

## 2. How it works

- **yt-dlp** does the downloading, as in Video Scribe: found on the PATH or fetched from its GitHub
  release into `%LOCALAPPDATA%\VidFetch\tools`, checked against `SHA2-256SUMS`, updated with `-U` every
  two weeks. Only formats over plain HTTP are asked for, as a stream in pieces (HLS) needs FFmpeg.
  `--encoding utf-8`, or titles with accents arrive in Windows' old code page.
- **No FFmpeg.** YouTube sends the picture (H.264, up to 1080 lines) and the sound (AAC) as two files.
  Media Foundation's sink writer copies both, unchanged, into one MP4 (`media::join`). YouTube's picture
  file has an edit list that stops Media Foundation reading it; the box is renamed in our own copy and
  the shift kept (`ytdlp::drop_edit_list`, Video Scribe's log entry 13). `[proven]` on "Me at the zoo",
  2 November 2026.
- **Sound only**: M4A is the AAC copied, unchanged, into an ordinary M4A (YouTube's own is cut up for
  streaming, which Windows cannot tag); MP3 is decoded and encoded by Media Foundation, 192 kbps.
- **Tags** through the Windows property system: title, channel as artist, year, address as comment;
  for a playlist, its name as album and the place as track number.
- **Playlists** (and channels): `yt-dlp --flat-playlist -J` lists them; each video is downloaded in
  turn into a folder named after the playlist, as `03 - Title.mp4`. `VidFetch.json` in the folder records
  the playlist and each video done, so stopping, or Update the playlist (Ctrl+U) later, fetches only
  what is missing. A video that fails does not stop the rest; four failing in a row before any
  succeeds does.
- **A video in a playlist** (`watch?v=...&list=...`): VidFetch asks, whole playlist or just the video,
  with just the video the default. A YouTube mix (`list=RD...`) is never taken as a playlist.
- **The queue**: `ui.rs` holds what waits, each with the settings it had when asked for, and starts
  the next as a job ends. Escape stops the one running, not the queue. Jobs carry on behind any window
  in front, without moving focus; what they have to show waits for it to close (WM_ENABLE). Not kept
  when VidFetch closes.
- **Already downloaded**: `%APPDATA%\VidFetch\downloaded.json` records each single download by
  `ytdlp::key`, format and lines (none for sound). The same again asks, Open it being the default; a
  different format or quality just downloads. Rename in VidFetch keeps the record; File Explorer does not.
- **Captions** (an option, off to begin with): after the file is safely saved, `yt-dlp -J` lists them,
  then yt-dlp fetches one track, and `captions.rs` writes SRT, as yt-dlp's own conversion needs FFmpeg.
  A person's in the video's language or English; else the automatic track of the video's language
  (a dubbed video has one `-orig` track for each language); else a person's in another language. Never
  a machine translation. A failure is a note, not a lost video. Rename and Delete take the file along.
- Downloads in progress live in `%LOCALAPPDATA%\VidFetch\downloads`, kept a day so a stopped download
  carries on.

## 3. The window

Video Scribe's main window: the list of downloads (files and playlist folders in the output folder,
most recent first), Open (Enter: plays a file, opens a playlist's folder), New download (Ctrl+N),
Update playlist (Ctrl+U), Show queue (Ctrl+Q), Rename (F2), Show video in file explorer, Delete (to
the Recycle Bin), Status. What comes from YouTube has a window of its own (decision 16). New download takes the address from the clipboard, and has Download as,
Picture quality and Save captions in it.

## 4. Decisions

1. Defaults: sound only, M4A; 720 lines when video is chosen; playlist files numbered; captions off.
   The user: "720 is right, but default should be to download sound only when possible." (Log entry 1.)
2. Settings in `%APPDATA%\VidFetch\settings.txt`, as in Video Scribe; output in `Videos\VidFetch`.
3. A single video goes into the output folder itself, named after its title; a playlist into a folder.
4. New videos in a playlist are not announced as they start: the Status box is enough (the user, log
   entry 1, Q3). Likewise a queued download's steps are shown, not said; each end is said.
5. Signing in is not built: public videos need none. The user: "Not for now." (Log entry 2, Q1.)
6. Enter plays a download in VidFetch's own player (Video Scribe's), sound only; Open in the default
   player shows the picture. The user: "Could we please have VideoScribe's built-in media player?"
   (Milestone 2 test, 2.4.)
7. The repeat question is Download again or Cancel, Cancel the default: "Please take away the "open
   it" option." (Milestone 2 test, 3.6.)
8. Enter on a playlist shows its videos in the same list; Escape goes back (milestone 2 test, 5).
9. The queue shows titles (log entry 2, Q2: "Yes."), and is kept between runs, with a question at the
   start and Empty the queue in the Download menu (Q3).
10. Escape and Backspace go back one list at a time: "Consistent with Windows Explorer" (log entry 3,
    Q2). Search results and channels were in the main list at first; see decision 16.
11. Search asks YouTube directly (InnerTube, `youtube.rs`), falling back to yt-dlp; listening gets the
    sound's address from yt-dlp (`-g`), about 5 seconds, then streams it (Q3, option c).
12. Following from a search result, from a download, and from a channel address in New download (Q4).
    Channels are checked only when the user presses Ctrl+L (Q5), through their Videos page, as their
    public feeds now answer 404 (milestone 4 test, section 4). New is what was not listed last time.
13. Categories as NoteFetch has them: folders in the output folder, the rest in Uncategorised, the
    existing downloads moved there once (Q6). New download has a Category box, the category shown
    chosen (Q7).
14. Deno is fetched, asking first, only if yt-dlp comes to need it for YouTube (Q1). Not needed yet.
15. Order of work: search, channels, resume, speed and volume, categories, then the rest of the ideas
    list (Q8). Built in milestone 4: all but audio-described sound, skip silence and the clipboard
    watch.
16. The YouTube window: search, new videos, channels, a channel's videos and playlists, a playlist,
    and recently played, in a window of their own in front of the main one: "its own window to avoid
    confusion" (milestone 4 test, sections 3 and 5).
17. Menu items that cannot apply are greyed out as the menu opens; buttons never are (milestone 4
    test, section 1). Shorter menu wording, as the user gave it.
18. Cut silences and Offer copied web addresses are options, off: "Nice option to have, but should be
    off by default." (Log entry 4, Q2 and Q3.)
19. yt-dlp is fetched as its release's folder build, which starts in 1 second against 5, and VidFetch
    updates it by checksum (milestone 4 test, section 7). Nothing that might upset YouTube: no extra
    connections, no other client pretended, no addresses worked out without yt-dlp.
20. Audio-described sound waits for a video that has a described track: the M&S advert of 2026 has
    none (log entry 4, Q1).

## 5. Milestones

1. **The downloader** (this one): everything above. Built 2 November 2026; tested by Claude on
   YouTube (a video, a playlist stopped and carried on, M4A, MP3), a direct link and archive.org;
   awaiting the user's test, `test-milestone-1.txt`.
2. **The user's answers to log entry 1** (branch `m2-queue-captions`): sound by default, the queue,
   captions, noticing a repeat. Built 2 November 2026; tried by Claude (log entry 2); awaiting the
   user's test, `test-milestone-2.txt`, which replaces milestone 1's. Tested by the user 7 November
   2026, merged into `main`; the queue's own steps could not be tried, as single videos were too quick.
3. **The milestone 2 test's answers** (branch `m3-player-and-fixes`): the player, a playlist's videos,
   the queue with titles kept between runs, Download again or Cancel, Alt+T, "videos" not "files".
   Built 7 November 2026; tried by Claude (log entry 3).
   Tested by the user 7 November 2026 and merged.
4. **Search, channels, categories and the listening ideas** (branch `m4-search-channels`):
   decisions 10 to 15. Built 7 November 2026; tried by Claude (log entry 4); tested by the user and
   pushed.
5. **The milestone 4 test's answers** (branch `m5-windows-and-fixes`): decisions 16 to 20, channel
   playlists, the silent video fixed, Ctrl+C, faster speeds. Built 8 November 2026; tried by Claude
   (log entry 5); tested by the user, merged and pushed 8 November 2026.
6. **The milestone 5 test's answers** (branch `m6-beta`): Continuous play, the Play window's keys in a
   box, Copy all text, Listen without downloading, the silent video through yt-dlp, downloading a gone
   file again. Built 8 November 2026; awaiting `test-milestone-6.txt`. Log entry 6 proposes what comes
   before a first beta.
7. A private GitHub repository: done, https://github.com/ldefontenay/vidfetch (log entry 1, Q6). Next, a
   release like NoteFetch's with its guide online. Needs a LICENSE file, which the repository does not
   have yet.
