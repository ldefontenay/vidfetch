# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

# VidFetch

A small Windows app that downloads videos and whole playlists from YouTube and other sites, as video
(MP4) or sound only (M4A or MP3), with a screen reader first. It is a subset of
[Video Scribe](../videoscribe) without the describing: the same window, list of downloads, yt-dlp
handling and Media Foundation code, plus playlists.

**`PLAN.md` is the source of truth** for decisions and milestones. `log.md` holds findings and questions,
newest entry at the top.

## Where things stand

Split off from a Video Scribe session on 2 November 2026; this repository is now worked on on its own.

- Milestone 1, the downloader, is built and committed on `main`. Nothing is pushed: there is no GitHub
  repository yet (log entry 1, Q6, asks).
- The user's test is `test-milestone-1.txt`; their answers to it and to log entry 1 are what comes next.
- Tried by Claude, not yet by the user (see log entry 1): YouTube video as MP4, M4A and MP3; a playlist
  stopped and carried on with Update; a direct file link; an archive.org film; an accented title. The
  a11y probe was clean on the main window and New download.
- `%APPDATA%\VidFetch` and `%LOCALAPPDATA%\VidFetch` were deleted after testing, so the user's first run
  shows the welcome and the yt-dlp question. If you test, use a scratch output folder and delete both
  again afterwards, unless the user has started using VidFetch for real (then back up their settings first).
- Video Scribe's yt-dlp encoding fix (`--encoding utf-8`) came from here and lives in that repository.

## Editing practice

Python edit scripts: write the `.py` with the Write tool, with raw strings (`r'''...'''`). Bash heredocs
mangle backslashes, and a backslash-newline in a Rust string inside a non-raw Python string is swallowed.

Sibling projects are the reference for house style and code to reuse: `..\videoscribe` (most of this
app came from there), `..\NoteFetch` (the list and naming), `..\PhotoScribe`. Read them freely; never
modify them from a VidFetch session.

## Working with the user

- The user is blind and uses JAWS. **Findings, plans and questions go in `log.md`, with the newest entry at
  the top.** Each question is followed by an `ANSWER:` line. Keep terminal replies short and point to the file.
- Manual tests go in `test-milestone-N.txt`: plain text, one step at a time, each followed by `ANSWER:`,
  plus an "ANYTHING ODD:" line in each section. Never put manual steps in chat.
- Load the `accessible-windows-apps` skill before any UI work, and run its probe before asking the user to test.
- Do not grey out a control to express a constraint: every control stays enabled and says what it did.

## Non-negotiable constraints

1. Accessibility is the product. Everything must be usable without sight.
2. Real Win32 controls from an rc.exe dialog template. No drawn-UI toolkit.
3. Keyboard first. Every state change the user causes is audible or announced.
4. Small. Prefer Windows built-ins (Media Foundation, WinHTTP, CNG, the property system) to crates.
   **FFmpeg does not ship**: Media Foundation joins YouTube's separate picture and sound.
5. Nothing starts a second process except `yt-dlp.exe`.
6. A download is written under a temporary name and renamed when whole, never half-written under its
   real name.

## Commands

```
cargo build / cargo run / cargo build --release
cargo test <name>          # a single test
cargo fmt
cargo clippy --all-targets -- -D warnings
cargo test
# A real download, without the window (needs the network):
VF_URL=<address> VF_OUT=<folder> [VF_FORMAT=m4a|mp3] [VF_WHOLE=1] [VF_STOP_AT=3] \
    cargo test end_to_end -- --ignored --nocapture
```

## Session hygiene

- Use a branch for each milestone. Docs-only changes can go straight to `main`.
- Commit in small steps. Ask before pushing. Say plainly what is untested.
- Never commit downloaded media, yt-dlp, or build output.
- When testing the app, point its output folder at a scratch folder, and put the user's settings back.

## Provenance and licences

- **Video Scribe**, **NoteFetch**, **Photo Scribe**: same author, MIT; code reused with a note in each
  module saying where it came from.
- **yt-dlp** (Unlicense): downloaded at run time from its GitHub release, checked against its checksum,
  never shipped.
