# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

# VidFetch

A small Windows app: an accessible interface for YouTube (search, listen, follow channels, playlists,
download), with a screen reader first, which can also download from other video sites, as video (MP4)
or sound only (M4A or MP3). It began as a subset of [Video Scribe](../videoscribe) and shares its
window, list of downloads, yt-dlp handling and Media Foundation code.

**`PLAN.md` is the source of truth** for decisions and milestones. `log.md` holds findings and questions,
newest entry at the top.

## Where things stand

Split off from a Video Scribe session on 2 November 2026; this repository is now worked on on its own.

- Milestones 1 to 6 are on `main`, tested and pushed. Milestone 7, towards a first beta, is on
  `m7-beta`, awaiting `test-milestone-7.txt`. Log entry 7 has its questions.
- `src/announce.rs` raises UI Automation notifications on a provider of its own, and uses a hidden
  live region when NVDA is running. `hush()` stops JAWS through its COM API for the quiet Play window.
- Deno is optional, in `tools\deno`, fetched on request; yt-dlp starts it, not VidFetch.
- `src/ui/online.rs` is the YouTube window (IDD_BROWSE): search, new videos, channels, a channel's
  videos and playlists, a playlist, recently played. It runs its own message loop in front of the
  disabled main window and shares the main window's `App`. `src/ui/category.rs` holds the
  categories. Both are children of `ui.rs` and use its private items.
- yt-dlp is fetched as the release's folder build, `yt-dlp_win.zip`, into
  `%LOCALAPPDATA%\VidFetch\tools\yt-dlp`, and VidFetch updates it itself by checksum. An older single
  `tools\yt-dlp.exe` is still used until its next update replaces it.
- The private repository is https://github.com/ldefontenay/vidfetch, created by the user, as the
  permission check refused it from Claude. Still ask before pushing.
- The user now uses VidFetch for real: their settings are in `%APPDATA%\VidFetch` and yt-dlp in
  `%LOCALAPPDATA%\VidFetch\tools`. Test with `APPDATA` and `LOCALAPPDATA` pointed at a temporary
  folder (copy yt-dlp.exe into its `VidFetch\tools`), and never touch theirs.
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
  **Menu items are the exception**: the user asked for those that cannot apply to be greyed out as the
  menu opens (milestone 4 test, section 1). Do it in the `WM_INITMENUPOPUP` handler.

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
VF_URL=<address> VF_OUT=<folder> [VF_FORMAT=m4a|mp3] [VF_WHOLE=1] [VF_STOP_AT=3] [VF_CAPTIONS=1] \
    cargo test end_to_end -- --ignored --nocapture
```

## Session hygiene

- Use a branch for each milestone. Docs-only changes can go straight to `main`.
- Commit in small steps. Ask before pushing. Say plainly what is untested.
- Never commit downloaded media, yt-dlp, or build output.
- When testing the app, point its output folder at a scratch folder, and put the user's settings back.
  Setting `APPDATA` and `LOCALAPPDATA` to scratch folders for the test leaves theirs untouched.

## Provenance and licences

- **Video Scribe**, **NoteFetch**, **Photo Scribe**: same author, MIT; code reused with a note in each
  module saying where it came from.
- **yt-dlp** (Unlicense): downloaded at run time from its GitHub release, checked against its checksum,
  never shipped.
