# VidFetch log

Newest entry at the top. Each question is followed by an `ANSWER:` line.

## Entry 7 — Towards a first beta: your milestone 6 answers, and all of A and B (2026-11-08)

Milestone 6 is merged and pushed, as you said in Q5. What is below is on a new branch, `m7-beta`, not
pushed. The test is `test-milestone-7.txt`.

### Your notes from the test

- **A silent Play window (2.1).** VidFetch no longer says anything itself as it opens, except
  "Please wait" when streaming. JAWS reads a new window by itself, and Windows offers no way to prevent that, so VidFetch tells JAWS to stop speaking, four times in the first second, through the programming interface JAWS installs. This is an experiment: you may hear the start of the title before it is cut off. Only your ears can tell. Page Down still says the name of what comes next, as otherwise you could not know. "Carrying on from" is no longer said, as you asked for nothing; T says where you are.
- **The Keys box (2.2)** has your exact wording.
- **A site other than YouTube (3.3).** Please try https://archive.org/details/BigBuckBunny_124. yt-dlp finds an MP4 there. It is in the test.
- **Alt+L on a playlist or channel (3.4)** now opens its list of videos in the YouTube window. I tried your playlist, To the Heights: 16 videos. Its last line says More, even though there are no more; pressing it says that was all. Nothing was played before, which was the fault you noticed.
- **A gone file in the main list (4.2).** These stay at the end of the main list as "no longer on this computer". Enter asks whether to download again; Delete takes it off the list. To make this possible VidFetch now keeps the record of a gone file, where it used to throw it away. Files gone before this version may therefore not appear. Files you delete in VidFetch are forgotten at once, and do not appear. Only single videos from YouTube; a playlist's videos are left out, as updating the playlist already deals with those.
- **Escape and Backspace (Q1)** are as you agreed, in every window and in the help.

### Starting the next download during "Taking out the sound"

Yes, it could be done, and nothing about YouTube prevents it, since it would still be one download at
a time. The time saved is the length of that stage, for every video but the last. I have **not built
it**: the download and that stage are one step in the code, so separating them touches what records
which videos are done, and a mistake there could lose your place in a playlist. I did not want that
mixed into this large milestone. See Q2.

### What is new, from A and B

**A1. LICENSE** (MIT) is added. The GitHub release and online guide are waiting for Q1.

**A2.** The version is 0.7.0-beta.1, and About says so. VidFetch looks for a newer version once a week and tells you; Help, Check for a newer version, looks now, and offers the page. It installs nothing.

**A3. NVDA and Narrator.** From NoteFetch's proven code: NVDA does not hear UI Automation announcements from plain windows, so with NVDA running, announcements now go through what is called a live region. Narrator gets its announcements from a small addition too. It is in the test, if you have the time.

**A4. Deno.** File menu, "Fetch Deno, which helps with YouTube". VidFetch asks first, downloads it from Deno's own GitHub releases, checks the checksum, and puts it with yt-dlp. yt-dlp starts it, so VidFetch itself still starts only yt-dlp. It is about 100 megabytes on disk, more than I expected. It is optional, and VidFetch works without it. I tried it here: yt-dlp solved YouTube's challenge with it. One thing to know: with Deno, YouTube offers yt-dlp more formats, and for one video it picked a different sound format (5.1 surround AAC) than without. I have not heard how that plays.

**A5. Clearer messages** for age-restricted videos, "confirm you are not a bot", premieres not yet started, live streams and the Deno case. A row showing "live now" says at once that VidFetch cannot take it while it is live. I have not met these errors live, so the wording comes from yt-dlp's known messages.

**B1. Search.** Ctrl+F now opens a window with what to search for, Look for (videos, channels, playlists), Sort, Length of videos, and Leave out YouTube Shorts.

**B2. Related videos**, Ctrl+R, on any video, also on a download in the main window. Lists stack, so you can wander, and Backspace comes back.

**B3. Import channels**, in the File menu: Google Takeout's subscriptions.csv, FreeTube, NewPipe, OPML. And Save a backup of your channels, in NewPipe's form.

**B4. Comments**, Ctrl+K: the first 100, with who, when, likes and how many replies, in a window to read with the arrow keys. Replies themselves are not included.

**B5. Transcript**, Ctrl+T, through yt-dlp, about 8 seconds.

**B6. A channel's search**, Ctrl+Shift+F, and More for older videos on a channel.

### Tried by me

With temporary settings. Automated checks and unit tests pass. In the real window: the search window with options, related videos, comments, transcript, the gone file listed, Escape closing the YouTube window, Alt+L on your playlist, the Play window with your Keys text. The accessibility probe found no problems in Search YouTube or the Play window. Against YouTube itself: channel and playlist search, related and its next page, comments, a channel's older videos and its search. Deno downloaded and checked.

**Not tried:** how JAWS sounds with the quiet opening; anything with NVDA or Narrator; the Import and Backup file windows; Fetch Deno and Download again from the menus; the look for a newer version, as the repository is private and GitHub will not answer; playing from archive.org.

### Not built

- C, as you preferred. My view: none of them is needed for a beta. If you want one, media keys is the cheapest and most useful.

### A name

You asked for suggestions. I have not checked whether any of these is taken.

- **VidFetch**: keep it. Your other programs are NoteFetch, Photo Scribe and Video Scribe, so it fits.
- **TubeListen**, says what you mostly do with it.
- **HearTube**, short, and clear to blind users looking for it.
- **Video Companion**, plain and without "tube", which YouTube's lawyers can dislike.

I would keep VidFetch for the beta, and change only if testers are misled by it.

### Where does this add value? Should you contribute elsewhere instead?

An honest opinion, bearing in mind that I have read about the other programs, not used them.

**What VidFetch has that I did not find elsewhere:** it was built and tested with JAWS, by a JAWS user, and it is one small program of real Windows controls. HexPlayer and A11YTube describe themselves as built around NVDA, and A11YTube relies on VLC.

**The real cost is not building it but keeping it working.** YouTube changes often. yt-dlp absorbs most of that for downloading, but search, comments and channels here use YouTube's private interface, and they will break from time to time and need fixing. If you publish a beta, people will come to depend on that.

**My recommendation:** carry on, because it already does what you want and works with your screen reader. But before investing in a public release, spend an hour with A11YTube and HexPlayer under JAWS. If one of them is as good for you, sending its authors your JAWS findings would help more people for less upkeep, and there is little lost. If neither is, you have your answer, and a good reason to publish.

Response: Thanks, please set up a document telling me where to get these apps.

### Questions

**Q1.** The repository is still private. You said public on GitHub. Once you have tested this, shall I make it public and publish 0.7.0-beta.1 with the guide online? And would you rather make it public yourself, as you created it?

ANSWER: Yes, make it public. Do as much of the process as you can yourself. Look at how NoteFetch's page is set up and do the same. Provide a exe and zip download. Create a friendly guide, similar to NoteFetch and the one I created for Andre Louis' Clipman.

**Q2.** Shall I build the overlap, starting the next download during "Taking out the sound", in the next milestone?

ANSWER: Yes please.

**Q3.** The name: keep VidFetch, or one of the above?

ANSWER: Keep VidFetch for now.

**Q4.** Having read the opinion above, would you like to carry on towards a public beta, or try the other programs first?

ANSWER: I'll carry on and try the other apps in parallel.

**Q5.** After your test, shall I merge this milestone into `main` and push?

ANSWER: Yes.

## Entry 6 — Your milestone 5 answers built, and what could come before a first beta (2026-11-08)

Thank you, and I am very glad it has become useful. Milestones 4 and 5 are merged into `main` and
pushed, as you said in Q5. What is below is on a new branch, `m6-beta`, and the test is
`test-milestone-6.txt`. The list of possible beta features is further down: **none of it is built
yet**, so that you can choose.

### What changed

- **"Continuous play"** replaces "Play next when one ends".
- **Recently played** now says "14 recently played items", and "items" elsewhere in that list.
- **The Play window (4.1).** The explanation of the keys was plain text at the top of the window, and
  JAWS reads such text as a window opens. It is now in a box of its own, Keys, Alt+K, which is only
  read if you go to it.
- **The video that took two minutes (4.7).** You guessed well that the download was different.
  VidFetch was asking YouTube for the whole file in one request, and YouTube slows those down.
  yt-dlp asks in smaller pieces. VidFetch now fetches it through yt-dlp: 11 seconds in all here,
  from pressing the button to hearing it. Playing it again the same day needs no fetching. The Play
  window says what is happening, and the help explains what this type of video is, under Playing.
- **Ctrl+C (5.1).** JAWS keeps Ctrl+C to itself when nothing is selected, says "nothing selected",
  and VidFetch never receives the key, so I cannot fix that key from here. Every message window now
  has a **Copy all text** button, Alt+O. Selecting a part and pressing Ctrl+C works as usual, and so
  does Ctrl+A then Ctrl+C.
- **Listen without downloading (5.2).** New download has this new button, Alt+L, for YouTube and for
  other sites. Ctrl+D in the Play window downloads it after all.
  - Offer copied web addresses now opens New download with the address, rather than asking a
    question first. From there you can download, listen, or press Escape. That seemed better than a
    question with three answers. Tell me if not.
  - A playlist or channel address cannot be listened to as one video, and VidFetch says so.
- **A download whose file has gone (Q3).** The main list shows only what is in the folder, so such a
  download appears only in Recently played. Enter on it now asks whether to download it again, into
  the same category. This works for downloads from YouTube that VidFetch has a record of.
- **An accessible interface for YouTube.** The welcome message, About, the help, the README, the plan
  and the program's description now say this, and that it can also download from other video sites.
  The name stays VidFetch unless you would like another (Q3 below).

### Tried by me

With temporary settings. The automated checks and 53 unit tests pass. The accessibility probe found
no problems in New download or in the Play window, with its new Keys box. Through the real window,
Listen without downloading on the silent video worked: VidFetch noticed there was no sound, fetched
it and played it.

**Not tried:** how JAWS behaves with the Keys box; the Copy all text button, so as not to touch your
clipboard; Download again on a gone file; Listen on a site other than YouTube.

### About your other notes

- **Page Down still took 5 seconds (4.4).** That step came before the faster yt-dlp in the test, and
  6.2 then said about 2 seconds. If you press Page Down within the first few seconds of a video, its
  next one is still being found, so it can take longer.

### Escape and Backspace: what I think

I agree with you: one key, one meaning, is better. Today Escape does three jobs: it stops, goes back
and closes. I recommend:

- **Backspace is always "back":** in the YouTube window and among a playlist's videos.
- **Escape is always "stop or leave":**
  - In the main window, it stops the download, even while a playlist's videos are shown.
  - In the YouTube window, it stops a lookup if one is running; otherwise it closes the window,
    from whichever list. It would no longer go back one list.
  - In all other windows, it closes them, as it does now.

The one cost is that Escape in a deep list closes the whole window, and you lose your place in the
lists. See Q1.

### Before a first beta: what I found

I looked at the other accessible YouTube programs for Windows ([HexPlayer](https://makhlwf.github.io/accessible_youtube_downloader_pro),
[A11YTube](https://gh.r-mj.com/Daoductrung/A11YTube), [AN Video Downloader](https://applevis.com/forum/windows/video-downloader-accessible-youtube-downloader-blind-users)
and [the one from Accessible Tech Labs India](https://accessibletechlabsindia.github.io/software/youtube-downloader/youtube-downloader.html)),
and at [FreeTube](https://docs.freetubeapp.io/about/freetube), the best-known YouTube program without
an account. I read their descriptions but did not try them. The two closest to VidFetch say they are
built around NVDA, so a program that works well with JAWS has a place of its own.

**A. Needed for any beta** (I recommend all of these):

1. A LICENSE file, a GitHub release, and the guide online, as NoteFetch has.
2. A version number with "beta" in it, and a check for a newer VidFetch: telling you, not installing
   anything by itself.
3. A test with NVDA and Narrator. Everything so far has been proven with JAWS only.
4. **A warning about yt-dlp.** [yt-dlp's reports](https://github.com/yt-dlp/yt-dlp/issues/15043) say
   that YouTube without a JavaScript program called Deno is deprecated and some formats may go
   missing. VidFetch works without it today, and you decided in log entry 3 to fetch Deno, asking
   first, only if it became necessary. It may become necessary during the beta, and it would be
   better built before than after. It would be one more program started, an exception to the rule
   that only yt-dlp is.
Comment: Happy to go with Deno implementation now if that can prevent things from breaking soon. Is there also any functionality that could be gained by using Deno now?

5. Clearer messages for what beta testers will meet: age-restricted videos, "confirm you are not a
   bot", live streams and upcoming premieres. Live streams are the unknown one, as VidFetch only
   takes sound over plain web requests.

**B. Most useful as a YouTube interface** (my order of value):

1. **Search for channels and playlists**, not only videos, plus filters: newest first, length, and
   leaving out Shorts.
2. **Related videos** on any video, the way people wander through YouTube.
3. **Import your subscriptions**, from a Google Takeout file or from FreeTube or NewPipe, so that
   nobody has to follow 100 channels one at a time. Also a backup of your channels.
4. **Read the comments** of a video, in a window to read with the arrow keys.
5. **A transcript of any video** without downloading it. Captions already work for downloads.
6. **A channel's search, and a whole channel's list** beyond its latest 30 videos, with More.

**C. Listening**:

1. **Windows' media keys** (play and pause on the keyboard or a headset), which work with VidFetch in
   the background.
2. **A background player**, so you can keep listening while using the YouTube window or other
   programs. Today the Play window stands in front. This is the largest item here.
3. **A sleep timer**, and **bookmarks** within a long video.
4. **[SponsorBlock](https://wiki.sponsor.ajay.app/w/API_Docs)**, off by default: skips sponsor
   messages using timings shared by volunteers. It is built into FreeTube, and can be asked about
   without telling the service which video you are playing.
5. **Audio description**, once a video with a described track turns up.

**D. Later, not for a beta:** your own playlists, favourites (you said "already covered for now"),
signing in, translations.

Sources: the pages linked above; [FreeTube on GitHub](https://github.com/FreeTubeApp/FreeTube).

### Questions

**Q1.** Escape and Backspace: shall I make the change recommended above?

ANSWER: Yes

**Q2.** Which of A, B and C would you like before the beta? For example "all of A, B1 to B3, C1, C3".

ANSWER: all of a and b. in c, I'm happy to do those later, unless you think that some should be included.

**Q3.** The name VidFetch sounds like a downloader. Keep it, or would you like another name for the
beta? If so, have you one in mind, or shall I suggest some?

ANSWER: Please suggest some.

**Q4.** Should the beta be public on GitHub, or shared privately with a few testers first? The
repository is private now.

ANSWER: Public on Github.

**Q5.** After your test of this milestone, shall I merge it into `main` and push?

ANSWER: Yes.

Question: Bearing in mind the existing programmes doing a similar job, where does this app add value? Should I rather use the best of what's already available and maybe contribute to it?

## Entry 5 — Your milestone 4 answers built: the YouTube window, playlists, faster starts (2026-11-08)

Thank you for the test and the answers. Milestone 4 was pushed, as you said in Q6. Everything below
is on a new branch, `m5-windows-and-fixes`, and the test is `test-milestone-5.txt`.

### What changed

- **The YouTube window (sections 3 and 5).** Search results, new videos, your channels, a
  channel's videos and playlists, a playlist's videos, and Recently played now have a window of
  their own. The main list holds only your downloads, and the Category box no longer clears
  anything.
  - The window stands in front of the main one, which waits behind it, as with a dialog box.
    Downloads carry on.
  - Enter, Ctrl+D, Ctrl+I, the context menu, and Escape or Backspace work as before. From the first
    list, Escape closes the window.
  - Its menus are Video (Alt+V), Go (Alt+G) and Help. The buttons are Play, Download, Search and
    Close window.
- **Channels (section 4).** YouTube's channel feeds now answer "404 not found" for every channel,
  which is why Ctrl+L and Enter on a channel failed. VidFetch reads the channel's own Videos page
  instead.
  - "New" marks are exact now: VidFetch remembers the videos it listed last time.
  - **A channel's playlists:** the last line of a channel's videos is "Playlists of ...". Enter
    lists them, with how many videos each has. Enter on one shows its videos, and Ctrl+D downloads
    it whole, after asking. A video's context menu also has "Show channel's playlists".
- **More results (3.2).** The first press said "0 added" because VidFetch took the next-page key
  of one of YouTube's filter buttons, not of the results. It takes the right one now.
- **Wording (section 1).** The buttons, the Category box and every menu now use your shorter
  wording, such as "Update playlist", "Show queue", "Show video in file explorer", "Select output
  folder" and "Follow channel".
- **Greyed menu items (section 1).** A menu item that cannot apply now is greyed out, such as
  Update playlist on a single video or Stop download with nothing downloading. The buttons stay
  as they were: they say why instead. CLAUDE.md records this.
- **"Download video"** is in the YouTube window's menu rather than the main window's, as the main
  list now holds only downloads. See Q3.
- **The Play window (section 2).**
  - Where playing is (2.1) now updates while it has focus. See Q1.
  - The volume moves in steps of 5% (2.2).
  - Page Down says only the next title (2.5).
  - Enter says "Please wait" while the sound is coming. Page Down and Page Up say nothing extra,
    and the 5-second message is gone (3.3).
  - Faster and slower (2.3): the pieces the sound is cut into are longer, and the search for where
    each joins the last now reaches past one pitch period of a low voice. Before, it did not, which
    is the likely cause of the roughness at twice the speed and below. I cannot judge this by ear.
  - Cut silences (your Q2) is in the Options menu, off. Each silence is cut down to a second.
- **The video that would not play (section 3).** See "Findings" below; it is fixed.
- **Ctrl+C (section 6)** copies the whole description or captions when nothing is selected. With
  some text selected, it copies that, as before.
- **Offer copied web addresses (your Q3)** is in the Options menu, off. When it is on, and you copy
  a web address elsewhere and come back to VidFetch, it asks whether to download it.

### Tried by me

I used temporary settings, so yours were not touched. The automated checks and 53 unit tests pass.

- The accessibility probe found no problems in the main window or in the YouTube window, which I
  probed on four lists: your channels, a channel's videos, its playlists, and a playlist's videos.
- Live from YouTube: a channel's 30 latest videos, its 29 playlists, and a playlist of 95 videos.
- The sound file of the video that failed: VidFetch now notices that it gives no sound, and the
  repaired copy plays all 284 seconds.
- Fetching yt-dlp's faster build, checking it against its checksum, unpacking it, starting it, and
  updating it.

**Not tried:**

- Anything by ear: the speed change, cut silences, and whether JAWS reads the moving "Where
  playing is" box well.
- Playing from the YouTube window, Page Down through a list, and the Play window fetching a silent
  video whole. Each part is tested; I did not play anything through the real window.
- Ctrl+C in the description box, the offer of a copied address, and Unfollow channel on a download.
  I did not want to touch your clipboard again.
- Your own yt-dlp being replaced by the faster build. That happens at its next update: after two
  weeks, or with File, Update yt-dlp.

### Findings

- **Why 1g4rGEA4g-U had no sound.** YouTube's sound file for that video carries an "edit list", a
  note at the start saying to skip the first few hundredths of a second. In this kind of file,
  delivered in pieces, the note stops Windows' Media Foundation reading any sound at all: it read 0
  seconds of a 284-second file. With the note removed, it reads all of it.
  - VidFetch already removed this note from downloaded videos. It now does the same for sound-only
    downloads.
  - Listening online cannot change the file as it streams, so the Play window notices when nothing
    plays. It then fetches the sound whole, about 4 MB for this video, removes the note, and plays
    that.
- **Starting faster (section 7).** yt-dlp spends most of its time starting itself up.
  - **Done:** yt-dlp's single yt-dlp.exe unpacks itself into a temporary folder every time it runs.
    Its own release also offers a ready-unpacked folder build, which VidFetch now fetches instead.
    Here it starts in 1.0 seconds against 4.7, so listening online and downloads should start about
    4 seconds sooner. It is the same yt-dlp, from the same release, checked the same way.
    - The first start after each update is slow, 33 seconds here, while Windows Defender checks
      its new files. See Q4.
    - The folder build cannot update itself, so VidFetch updates it, by comparing the release's
      checksum with the one it kept.
  - **Done:** while one video plays from a list, VidFetch finds the next one's sound in the
    background, so Page Down should start it at once.
  - **Done:** a sound's address is kept for 3 hours, so playing the same video again starts at
    once. YouTube's addresses last about 6 hours.
  - **Not done, as it could upset YouTube:** making many connections at once, pretending to be
    another kind of YouTube app, or working out YouTube's addresses without yt-dlp. Each of these
    is a known way to be slowed down or blocked.
- **The M&S Christmas advert (your Q1).** I found this year's advert, "The Home Of Christmas"
  (Q1NywzDe0nM). On YouTube it has its original English sound and about 20 dubbed languages made by
  YouTube, but no described track. I found no separate described version either. So audio
  description stays not built until we find a video that has it.

### Questions

**Q1.** The "Where playing is" box now changes while it has focus. Does JAWS read it well, or does it
chatter as the time changes? If it chatters, I will make it change only when you press T or move
to it.

ANSWER: Jaws behaves exactly as I would wish, see response in tests.

**Q2.** The YouTube window stands in front, and the main window waits behind it. Is that right? The
other way is two windows side by side, moving between them with Alt+Tab. That is more flexible, but
easier to lose track of.

ANSWER: Very good as it is.

**Q3.** "Download video" is now only in the YouTube window. Does the main window need anything in its
place? For instance, "Download again" for a download whose file has gone?

ANSWER: No. But if a download's file has gone, it should be possible to download it again when pressing enter on it.

**Q4.** After each yt-dlp update, about every two weeks, the first download or listen may take about
30 seconds to start. Is that acceptable for starts about 4 seconds faster the rest of the time?

ANSWER: Yes.

**Q5.** After your test, shall I merge milestones 4 and 5 into `main` and push?

ANSWER: Yes

## Entry 4 — Search, channels, categories and the listening ideas built (2026-11-07)

Thank you for your answers. Milestone 3 is merged into `main`. Everything below is on a new branch,
`m4-search-channels`, and the test is `test-milestone-4.txt`. As you asked, I built as much as I
could before your next test: your answers Q1 to Q8, and most of the ideas list.

### What was built

- **Search YouTube, Ctrl+F (Q2, Q3).** The results go in the main list, with each one's channel,
  length and age. "More results" at the end fetches the next page.
  - Enter listens online, in the Play window.
  - Ctrl+D downloads, and Ctrl+I reads the description.
  - Escape or Backspace goes back one list at a time, as in File Explorer.
  - As agreed in option (c), the search asks YouTube directly, which takes about a second. When
    that fails, yt-dlp searches instead.
- **Following channels (Q4, Q5).** You can follow from three places: a search result, a download,
  or a channel's address in New download, with the new "Follow the channel instead" button.
  - Ctrl+L, New from your channels, lists their latest videos, newest first. Those that came out
    since your last Ctrl+L start with "New".
  - Ctrl+Shift+L, Your channels, lists the channels themselves. Enter shows a channel's videos;
    Delete stops following it, after asking.
  - Channels are checked only when you press Ctrl+L.
- **Categories (Q6, Q7).** They work as in NoteFetch: folders in the output folder, with
  Uncategorised for the rest.
  - The first start moves your downloads into Uncategorised, and says so.
  - "Category to show", Alt+C, is above the list. The Category menu is Alt+G.
  - Ctrl+Shift+M moves a download into a category.
  - New download has a Category box, with the category shown already chosen.
- **The Play window** gains your ideas 1 to 4 and 6:
  - It carries on where you stopped, for downloads and for videos online alike.
  - Up and Down change the volume.
  - Shift with Up and Down changes the speed, from half speed to three times as fast, a quarter at
    a time. Voices keep their pitch.
  - Page Down and Page Up go to the next or previous in the list, and "Play the next one at the
    end" in the Options menu does it by itself.
  - Ctrl with Page Down and Page Up goes from chapter to chapter, saying each name. End goes to 10
    seconds before the end.
  - While listening online, Ctrl+D downloads.
- **Reading (ideas 7 and 8).** "Read its description", Ctrl+I, and "Read its captions as text" are
  in the Download menu. The captions come as running text in paragraphs, each starting with its
  time.
- **Recently played, Ctrl+H (idea 9, the history half).** It lists the last 100 things you played,
  downloads and videos online alike, each with where you stopped. Enter carries on, and Delete takes
  one off the list without deleting the file.
- **Deno (Q1)** is not needed yet, so nothing was built for it.

Some decisions of mine, which you can change:

- A place within 10 seconds of the start or the end is not remembered. A finished video starts
  again from the beginning.
- Speed is kept from one video to the next, and saved, so it is the same tomorrow.
- A download started from search results says how to stop it, as Escape there goes back first.
- Chapters are found the way YouTube finds them: times at the start of lines in the description,
  starting at 0:00, with at least three chapters.

### Tried by me

I used temporary settings and copies of your Zoo and Blender downloads, so your own were not
touched. The automated checks and 48 unit tests pass. The accessibility probe found no problems in
the main window, New download, the Play window and the Search box.

Through the real window, each of these worked:

- Loose downloads moving into Uncategorised, with that said.
- A new category, and Ctrl+D downloading into it.
- A search with 20 results, then More results.
- Listening: about 6 seconds to start. Page Down started the next result in about 5 seconds.
- Following from all three places, then Ctrl+L, Your channels, and a channel's own videos.
- Descriptions, captions as text, and Recently played, including carrying on and Delete.
- Escape going back one list at a time.
- Playing at twice the speed, in an automated test: 7 seconds of playing went 14 seconds into the
  file, as it should.

**Not tried:**

- Anything by ear, including whether fast speech sounds good. Speed changes the sound itself, which
  only you can judge.
- Move to a category's menu, which I could not drive safely. I tried the moving underneath it.
- Chapters on a real video: the reading of them is tested, but I did not find one with chapters to
  try by hand.
- Your real downloads being moved into Uncategorised. That happens the first time you start it.

**Sorry:** during my test of "Copy its web address", I replaced what was on your clipboard with a
YouTube address. If you had something there you needed, it is gone; I should have kept it.

### Findings

- **Listening online takes about 5 seconds to start.** Of that, yt-dlp takes almost all the time
  finding the sound's address. The sound itself starts in under half a second. Jumping 5 minutes
  ahead takes about 3 seconds.
- **YouTube's second page of results is small.** It sometimes has only 4 videos rather than 20.
  That is YouTube, not VidFetch.
- **Audio-described sound (idea 5) is not built.** yt-dlp marks such a track "descriptive", but I
  could not find a single YouTube video that has one, so I could not test it. See Q1.

### Questions

**Q1.** Audio-described sound: do you know a YouTube video that has a described track? With one, I
can build it as an option: "Choose the described sound when there is one".

ANSWER: I think this year's M&S Christmas add has one. See if you can find it.

**Q2.** Skip silence (idea 10): should the Play window skip long silences, for instance any silence
longer than 2 seconds, as an option that is off to begin with?

ANSWER: Nice option to have, but should be off by default.

**Q3.** Copy a link and be asked (idea 11): when you copy a YouTube address anywhere, should VidFetch,
if it is running, offer to download it? It would be an option, off to begin with.

ANSWER: Nice to have, but off by default.

**Q4.** Favourites (the other half of idea 9): do you want to mark videos as favourites, beside
Recently played? Or do categories, and following channels, already cover it?

ANSWER: Already covered for now. Will consider adding later after using the app for a while and getting feedback.

**Q5.** Speed goes from half speed to three times as fast, in steps of a quarter. Is that the right
range and step?

ANSWER: Good for now.

**Q6.** May I push `main` and this branch to your GitHub repository? The last two milestones are
not on GitHub yet.

ANSWER: Yes, please push.

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
