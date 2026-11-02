# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

# VidFetch

A small Windows app that downloads videos and whole playlists from YouTube and other sites, as video
(MP4) or sound only (M4A or MP3), with a screen reader first. It is a subset of
[Video Scribe](../videoscribe) without the describing: the same window, list of downloads, yt-dlp
handling and Media Foundation code, plus playlists.

**`PLAN.md` is the source of truth** for decisions and milestones. `log.md` holds findings and questions,
newest entry at the top.

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
