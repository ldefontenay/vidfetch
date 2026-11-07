# VidFetch log

Newest entry at the top. Each question is followed by an `ANSWER:` line.

## Entry 3 — Your test answers built, and a plan for your feature requests (2026-11-07)

Thank you for the test. Milestone 2 is merged into `main`. The changes below are on a new branch,
`m3-player-and-fixes`, and the test is `test-milestone-3.txt`. It uses the playlist to keep the
queue busy, as you suggested. Your feature requests are further down, with a plan and questions:
I have not built them yet, because each one has a choice in it that is yours to make.

### What changed

- **Video Scribe's player (2.4).** Enter on a download plays it in a Play window: Play or pause,
  Back and Forward 10 seconds, the arrows for 2 seconds, Home, and T for where it is, as in Video
  Scribe. Escape closes it. It plays the sound only, of a video too. "Open in the default player",
  in the context menu and the Download menu, plays it in your usual player, with the picture.
- **"Play", not "Open" (1.3 and 2.5).** The button, the menus and what is said all say Play now. On a
  playlist, the button is "Show the playlist's videos".
- **The repeat question (3.6 and 6.2):** "Already downloaded as Me at the zoo.m4a, at this quality.
  Download it again?", with Cancel first. Enter or Escape cancels, says "Nothing downloaded", and
  chooses the copy you have in the list. Nothing opens.
- **Alt+T (5.3).** Pressed while you are already on the Status box, it now says the box every time.
  VidFetch says it itself, as Windows was not doing it after the second press.
- **"3 videos", not "3 files" (5.5).**
- **A playlist's videos in the list (section 5).** Enter on a playlist shows its videos in the same
  list, in the playlist's order, and the list's name says which playlist it is. Enter plays, and the
  context menu, F2 and Delete work as on any download. Escape or Backspace goes back to all
  downloads, on that playlist.
  - A decision of mine: while you are in a playlist, Escape goes back first, and a second Escape
    stops a download. Otherwise one Escape would have two jobs.
- **Titles in the queue (your Q2).** A YouTube video's title comes from YouTube itself, in under a
  second. yt-dlp takes about 15 seconds for one, so it is used only for playlists and other sites,
  and the address shows until the title comes.
- **The queue is kept (your Q3).**
  - Alt+F4 while something runs or waits asks first, with Keep downloading as the safe answer. The
    queue is kept, with the download that was running.
  - At the next start, VidFetch lists what had not finished and asks "Carry on downloading now, or
    empty the queue?", with Carry on first.
  - "Empty the queue" is in the Download menu, as well as Remove all in the Queue window. Both ask
    first.
  - A single video stopped part way starts again from the beginning; a playlist carries on with the
    films it does not have.
  - With nothing running or waiting, VidFetch closes without a question, as before.

### Tried by me

I used temporary settings and a temporary folder, so your own settings and downloads were not
touched. The automated checks and 32 unit tests pass. The accessibility probe found no problems in
the main window (in a playlist's videos too), the Play window and the Queue window.

- **The player:** a 15-minute M4A played, paused, skipped and played to the end. In the real window,
  the title said what was playing, and Back and pause did what they should.
- **Through the real window:**
  - The offer to carry on a saved queue, then that download running.
  - Enter on a playlist, then Escape back to it.
  - Escape with nothing running, which says so.
  - The repeat question cancelled, leaving the file chosen.
  - Two downloads queued, both showing titles within 4 seconds.
  - Alt+F4 asking, then Exit keeping both in `queue.json`.
- **Found and fixed on the way:** a `queue.json` saved by Notepad, with a byte order mark at the
  start, was not read. Now it is.

**Not tried:**

- Anything by ear, including whether the Play window is quick enough to start.
- Alt+T, which needs real key presses.
- Whether JAWS speaks a finishing download from behind another VidFetch window. Your queue test could
  not reach this last time. Section 5 of the test reaches the windows involved, but a playlist says
  nothing as each film starts, so it may need the end of a single video.
- A video's sound in the player. Only M4A was tried; an MP4's sound should play the same way.

### Two findings

**yt-dlp warns that YouTube without a JavaScript runtime is on its way out.** Every YouTube request
now prints: "No supported JavaScript runtime could be found ... YouTube extraction without a JS runtime
has been deprecated, and some formats may be missing." Downloads still work today. But at some point
yt-dlp may need Deno, a separate program of about 40 to 100 MB (I have not checked the size), to keep
working with YouTube.

That would mean a second program besides yt-dlp, which goes against one of our rules, although yt-dlp
would start it, not VidFetch. Getting it the way VidFetch gets yt-dlp, asking first, would be the
natural way. Nothing needs doing yet; I am telling you so it is no surprise. (Q1)

**yt-dlp is slow to start on this computer**, which matters for searching and listening online:

- 3.7 seconds to start with nothing to do.
- About 15 seconds for one video's details, or for a search.

Playing straight from YouTube works in VidFetch's player once it has the address: sound started in
2.5 seconds. But jumping 5 minutes ahead took 17 seconds, as Windows reads the stream from the start
up to that point.

### Your feature requests: a plan

**One idea that ties them together.** You asked for a playlist's videos to appear in the main list,
with Escape going back. Search results and new videos from followed channels could work the same
way. Then there is one list, with one set of keys, whatever it shows:

- Enter plays, or listens online.
- Ctrl+D downloads with the current settings, into the queue if something is running.
- The context menu has the rest, and Escape goes back.

I recommend this over separate windows. (Q2)

**1. Searching YouTube.** Ctrl+F, Search YouTube, asks what to search for, then the list shows the
results: title, channel and length.

- Enter listens to the sound straight from YouTube in the Play window.
- Ctrl+D adds it to the queue, and the context menu also has "Follow this channel" and "Copy the
  address".
- Escape goes back to your downloads.
- The catch is speed (Q3):
  - **(a) Through yt-dlp:** about 15 seconds to search, and 15 more before a result starts to play.
    This is reliable, and yt-dlp keeps up with YouTube's changes for us.
  - **(b) Asking YouTube directly**, as NewPipe and similar apps do: about a second for both, built
    into VidFetch. But YouTube changes break it from time to time, and VidFetch would need a new
    version each time, where yt-dlp updates itself. It is also something YouTube does not invite. I
    have not tried this way yet.
  - **(c) Both:** search and titles directly, and the sound through yt-dlp, falling back to yt-dlp
    for search when the direct way fails. Search would be quick, but listening would still wait about
    15 seconds.

**2. Following channels**, without an account:

- **Kept in VidFetch:** a list in `%APPDATA%\VidFetch\channels.json`.
- **Finding what is new:** each YouTube channel has a public feed of its latest 15 videos, which
  VidFetch can read itself through Windows, in about a second a channel, with no yt-dlp and no
  sign-in.
- **Ctrl+L, Followed channels:** the list shows what is new across all of them, newest first, each
  with its channel. Enter listens, and Ctrl+D downloads.
- **Choosing a channel:** from the context menu, the list could show one channel's recent videos, or
  stop following it.
- **Adding a channel** (Q4):
  - "Follow this channel" on a search result.
  - "Follow its channel" on any download in your list, as the file knows where it came from.
  - A channel's address in Ctrl+N, with a "Follow" button beside "Start downloading".
- **What counts as new:** anything since you last looked. VidFetch could check when it starts and say
  "5 new videos from your channels" (Q5).

**3. Categories, as in NoteFetch.** NoteFetch keeps a real folder for each category, and an
"Uncategorised" folder for the rest, so File Explorer shows the same as the program. In VidFetch:

- The output folder would hold only category folders and Uncategorised. Your downloads so far would
  move into Uncategorised once.
- A playlist's folder can go into a category whole.
- "Move to category", Ctrl+Shift+M, as in NoteFetch, with New, Rename and Delete category in a
  Category menu.
- The list could show one category at a time, or all of them, like NoteFetch's Folder box.
- Questions Q6 and Q7 below.

**4. What else blind users want.** I looked at the accessible YouTube programs others have built,
mainly A11YTube, "Accessible YouTube Downloader Pro" and "AN Video Downloader". I also looked at what
comes up most for blind viewers. I found no proper survey, so this list is my reading of those
programs and articles, not what blind users told me. In my suggested order:

1. **Remember where you stopped.** The Play window picks up where you left off, per file. A11YTube
   has this. Small to build.
2. **Playing speed**, from 1.25 to 2 times as fast, without the voice going high. Many blind people
   listen to speech fast, and Netflix adding speed control was reported as "a big deal for blind
   fans". Windows can do this; I have not tried it here.
3. **Play the next one.** At the end of a video in a playlist, or a search result, the next one
   starts, optionally.
4. **Volume** in the Play window, with the Up and Down arrows.
5. **Audio-described sound.** Some YouTube videos have a second, described sound track. VidFetch
   could choose it when there is one, as an option. I have not yet checked how often YouTube offers
   them.
6. **Chapters.** Many YouTube videos have named chapters. The Play window could jump from chapter to
   chapter, saying each name, with Page Up and Page Down.
7. **The description, readable.** A video's description, from its page, in a box to read with the
   arrow keys. Links and timestamps are often there.
8. **The captions as text.** The captions you already save, shown as plain text to read, or as the
   video plays.
9. **Favourites and history.** A11YTube has both: what you played lately, and what you marked.
10. **Skip silence** at the start and end, which A11YTube does.
11. **Copy a link and be asked.** When you copy a YouTube address anywhere, VidFetch offers to
    download it. This would be an option, off to begin with, as it may annoy.

My suggested order overall (Q8):

1. Search with listening.
2. Following channels.
3. Remembering where you stopped, speed and volume, which make listening online pleasant.
4. Categories.
5. The rest of the list.

Sources: https://github.com/Daoductrung/A11YTube (A11YTube's features);
https://www.applevis.com/forum/windows/video-downloader-accessible-youtube-downloader-blind-users
(AN Video Downloader); https://www.listennotes.com/podcasts/seeing-in-the-dark/using-accessible-youtube-F27OykRZNht/
(Accessible YouTube Downloader Pro); https://www.npr.org/2020/08/12/901317290/netflix-is-letting-some-people-speed-up-playback-thats-a-big-deal-for-blind-fans
(speed); https://www.ghacks.net/?p=180033 (YouTube's channel feeds).

### Questions

**Q1.** If yt-dlp comes to need Deno for YouTube, should VidFetch fetch it the way it fetches yt-dlp,
asking first?

ANSWER: Yes

**Q2.** Search results and followed channels in the main list, like a playlist's videos, with Escape
going back, rather than in windows of their own?

ANSWER: Yes. Also great that you thought of implementing backspace. Consistent with Windows Explorer.

**Q3.** For search and listening online: (a) through yt-dlp, slow but self-updating; (b) asking
YouTube directly, quick but breaking now and then until VidFetch is updated; or (c) both, searching
directly and listening through yt-dlp? My suggestion is (c).

ANSWER: Yes, go with C.

**Q4.** Which ways of adding a channel do you want: from search, from a download, from an address
in Ctrl+N? Any other?

ANSWER: All of the above for now please.

**Q5.** Should VidFetch check your channels when it starts and say how many new videos there are,
or only when you press Ctrl+L?

ANSWER: Only when I press ctrl+l.

**Q6.** Categories: should the output folder hold only category folders and Uncategorised, as in
NoteFetch, with your existing downloads moved into Uncategorised once?

ANSWER: Yes.

**Q7.** Where should a new download go: into Uncategorised, into the category the list is showing,
or should the New download window have a Category box?

ANSWER: New downloads can have a category box, with the current category preselected.

**Q8.** Is my order right, and which of the eleven ideas do you want at all?

ANSWER: Lets go with your order. Its very sensible.

## Entry 2 — Your answers built: sound by default, a queue, captions (2026-11-02)

The test is `test-milestone-2.txt`. It replaces `test-milestone-1.txt`, which you had not started, so
there is one test to do. Three questions and one thing for you to do are at the end of this entry.

### Your question: what is the impact of not signing in?

Very little for ordinary use. **Signing in would not make most videos available, because most already
are:** anything public on YouTube downloads without it, which is nearly everything you would find by
searching or be sent a link to. So I have not built it, as you asked for it only if it opened up most
videos. Here is what you would gain and what it would cost, so you can decide (Q1).

What signing in would add:

- Age-restricted videos.
- Your private videos and playlists, your Watch Later and Liked videos.
- Members-only videos on channels you pay for.
- Sometimes, a way past YouTube's "Sign in to confirm you're not a bot". This falls mostly on cloud
  servers and VPNs, less on a home connection, and reports say it usually passes. VidFetch now suggests
  trying again later when it appears. I did not meet it in any test tonight.

What it would cost:

- **It may not work with your browser.** It means reading the browser's login cookies. Since version
  127, Chrome on Windows has protected them with app-bound encryption, and yt-dlp has an open issue
  about the failure this causes (number 10927). Reports say Firefox works, or a cookies file you export
  by hand, which is awkward with a screen reader. I found this in forum threads and issue reports and
  have not tried it myself, so it may have changed.
- **It could put your Google account at some risk.** Automated downloading can be against YouTube's
  terms, and guides advise using a spare account rather than your own. I found no solid figures on how
  often accounts are actually penalised.
- **Those cookies are your logged-in session.** VidFetch would only hand them to yt-dlp, but it is
  still a program reaching into your browser's logins.

If you want it, the sensible form is an option, off to begin with, used only by the download that
needs it, and clearly saying what it does.

Sources: https://github.com/yt-dlp/yt-dlp/issues/10128 (the bot message) and
https://www.technetexperts.com/fix-yt-dlp-cloud-signin-error/ (cookies and account risk).

### What changed

- **Sound only by default (your Q1).** A new download is an M4A; it is now first in the Download as
  list. 720 lines stays as the picture quality when you choose video. "When possible": if a site has no
  separate sound, VidFetch takes the sound out of the video when it can. If it cannot, it keeps what
  came and says so.
- **The same video again (your Q2).** Same address, format and quality: "Already downloaded as Me at the
  zoo.m4a, at this quality. Open it, or download it again?" Open it comes first, and Enter or Escape
  both choose it. A different format or picture quality just downloads, with no question. Picture
  quality makes no difference to sound, so changing it does not count for an M4A or MP3. Every form of
  a YouTube address counts as the same video.
- **Each new video in a playlist (your Q3)** stays in the Status box and is not spoken, as it was.
- **The queue (your Q4).** While one download runs, Ctrl+N or Ctrl+U adds the next. Each one keeps the
  choices it had when you asked for it. Decisions of mine, which you may want otherwise:
  - Escape stops only the download running; the next one then starts. To stop everything, press Ctrl+Q,
    Remove all, close it, then Escape.
  - Show the queue, Ctrl+Q, is a new window: what is running, what is waiting, Remove (Delete key) and
    Remove all.
  - Queued downloads start silently, and their steps only go in the Status box, in the spirit of your
    Q3. The end of each is spoken, with how many remain.
  - A failure does not stop the queue. VidFetch says one stopped, and shows the reasons when all are done.
  - The same download twice in the queue is refused. Alt+F4 with something waiting asks first.
  - A download that ends while another VidFetch window is open no longer takes the focus from it.
- **Captions (your Q5).** Save captions, in the Options menu and the New download window, off to begin
  with. It saves "Title.en.srt" beside the file. A failure to get captions is only a note: the download
  is already safe by then. Rename and Delete in VidFetch take the captions along. Turning it on adds
  a few seconds a video.
- **GitHub (your Q6).** Done, by the command you typed: https://github.com/ldefontenay/vidfetch, private,
  with both branches. The release is for after your test, and will need a licence file, which the
  repository does not have yet.

The program is now 502 KB (it was 460).

### Tried by me

All with temporary settings and folders, so your own are untouched and you still get the first-run
welcome. The automated checks and 29 unit tests pass. The accessibility probe found no problems in the
main window, New download or the Queue window.

- **Video:** "Me at the zoo" as an MP4 at 360 lines, on the final code.
- **Captions:** "Me at the zoo" (a person's captions, correct); the first Blender film (automatic captions
  in English, no repeated lines); the Steve Jobs speech's automatic captions (no repeated lines).
- **Two bugs found that way and fixed.** The Blender film has dubbed sound, so it offers an "original"
  automatic track in 21 languages, and I first picked Arabic; now it picks the video's own language.
  And YouTube's automatic captions start some lines with a line holding one space, which ended the
  caption too early.
- **The queue, through the real window:** a download running, two more added, the same one refused, one
  removed, and the rest starting by themselves. An address that cannot work in the middle did not stop
  the next, and its reason appeared only after I closed the New download window I had left open. Focus
  stayed in that window throughout, then went to the list. Stop moved on to the next. The Alt+F4 question
  appeared, and answering Keep downloading carried on.
- **The repeat question:** a youtu.be form of the address was recognised as the same video; a change to
  MP3 asked nothing. Enter opened the file, which briefly started your default player on this computer.

**Not tried:**

- **Anything by ear.** Above all, whether JAWS speaks a finishing download from behind another open
  VidFetch window (test step 4.6), and how the wording sounds.
- Deleting a download with its captions through the window (a unit test covers finding them).
- The playlist with captions past its first film.
- A queue while yt-dlp is still being fetched on a first run.

### Found on the way, which may affect Video Scribe

Video Scribe's `captions.rs` has the same one-space-line problem, so it may lose the first line after a
silence in automatic captions. I read its code and did not test it, and I have not changed it. Would you
like it looked at in a Video Scribe session?

### Questions

**Q1.** Knowing the above, do you want signing in as an option? My suggestion is not for now. If you
find videos you want that VidFetch refuses, tell me which and it becomes worth doing. Also, which
browser would it use?

ANSWER: Not for now.

**Q2.** The Queue window shows addresses, not titles, as a title is only known once a download starts.
Fetching each title when it is added would take a few seconds in the background. Worth it?

ANSWER: Yes.

**Q3.** The queue is forgotten when VidFetch closes (it asks first). Should it be remembered, and carry
on next time?

ANSWER: Yes please. Make sure items can be removed from the queue by providing a clear queue option in an appropriate place. When the app starts with material in the queue, alert the user and ask whether to continue downloads or not.

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

ANSWER: See entry 2, at the top.
