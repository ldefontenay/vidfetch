//! YouTube asked directly, for what yt-dlp is slow at: searching, a video's
//! details, finding a channel from an address, and a channel's latest videos.
//! Each takes about a second, where yt-dlp takes 5 to 15. Downloading and
//! listening still go through yt-dlp, which keeps up with YouTube's changes
//! (log entry 3, Q3: "Yes, go with C."). If YouTube changes what it sends,
//! search falls back to yt-dlp (`ytdlp::search`).
//!
//! All of it comes from YouTube's own web API, as its pages and NewPipe use
//! it, without signing in. A channel's latest videos came from its public
//! feed until YouTube stopped answering those (404, November 2026: milestone
//! 4 test, 4.4); they come from the channel's Videos tab now, as its page
//! shows them, with its playlists from the Playlists tab.

use std::sync::atomic::AtomicBool;

use crate::net;

const HOST: &str = "www.youtube.com";
/// Who VidFetch says it is to YouTube's API: its web page. English, so
/// lengths and dates read the same everywhere.
const CONTEXT: &str =
    r#"{"client":{"clientName":"WEB","clientVersion":"2.20251101.00.00","hl":"en"}}"#;
/// A channel's Videos tab, and its Playlists tab.
const VIDEOS_TAB: &str = "EgZ2aWRlb3PyBgQKAjoA";
const PLAYLISTS_TAB: &str = "EglwbGF5bGlzdHPyBgQKAkIA";
/// A channel's search.
const SEARCH_TAB: &str = "EgZzZWFyY2jyBgQKAloA";
/// How many pages of comments are fetched: 20 comments each.
const COMMENT_PAGES: usize = 5;

/// What a search looks for.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub enum SearchFor {
    #[default]
    Videos,
    Channels,
    Playlists,
}

/// A search, as asked for in the Search YouTube window.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Query {
    pub text: String,
    pub what: SearchFor,
    pub newest: bool,
    /// YouTube's number for a length: 0 any, 1 under 4 minutes, 3 from 4 to
    /// 20, 2 over 20. Applies to videos.
    pub length: u8,
    /// Leave out YouTube Shorts.
    pub no_shorts: bool,
}

impl Query {
    /// YouTube's filter for this search: a small protocol buffer, in base64.
    fn params(&self) -> String {
        let kind = match self.what {
            SearchFor::Videos => 1,
            SearchFor::Channels => 2,
            SearchFor::Playlists => 3,
        };
        let mut filter = vec![0x10, kind];
        if self.what == SearchFor::Videos && self.length > 0 {
            filter.extend([0x18, self.length]);
        }
        let mut bytes = Vec::new();
        if self.newest {
            bytes.extend([0x08, 0x02]);
        }
        bytes.extend([0x12, filter.len() as u8]);
        bytes.extend(filter);
        base64(&bytes)
    }
}

fn base64(bytes: &[u8]) -> String {
    const LETTERS: &[u8] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut out = String::new();
    for c in bytes.chunks(3) {
        let n = (c[0] as u32) << 16
            | (*c.get(1).unwrap_or(&0) as u32) << 8
            | *c.get(2).unwrap_or(&0) as u32;
        out.push(LETTERS[(n >> 18) as usize & 63] as char);
        out.push(LETTERS[(n >> 12) as usize & 63] as char);
        out.push(if c.len() > 1 {
            LETTERS[(n >> 6) as usize & 63] as char
        } else {
            '='
        });
        out.push(if c.len() > 2 {
            LETTERS[n as usize & 63] as char
        } else {
            '='
        });
    }
    out
}

/// A video found online.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Video {
    pub id: String,
    pub title: String,
    pub channel: String,
    /// "UC..." when known.
    pub channel_id: String,
    /// "7:45", or empty when not known.
    pub length: String,
    /// When it came out, in words: "3 days ago".
    pub when: String,
    /// Seconds since 1970, about: worked out from `when`, for the newest
    /// first across channels.
    pub published: i64,
    /// The description, when it came with the video; else Ctrl+I fetches it.
    pub description: String,
    /// A YouTube Short.
    pub short: bool,
}

impl Video {
    pub fn url(&self) -> String {
        format!("https://www.youtube.com/watch?v={}", self.id)
    }

    /// The line in the list: "Title: channel, 7:45, 3 days ago".
    pub fn line(&self) -> String {
        let mut parts = Vec::new();
        if !self.channel.is_empty() {
            parts.push(self.channel.clone());
        }
        if self.short {
            parts.push("a short".into());
        }
        if !self.length.is_empty() {
            parts.push(self.length.clone());
        }
        if !self.when.is_empty() {
            parts.push(self.when.clone());
        }
        if parts.is_empty() {
            self.title.clone()
        } else {
            format!("{}: {}", self.title, parts.join(", "))
        }
    }
}

/// A page of search results, and the key to the next page, if there is one.
#[derive(Debug, Default)]
pub struct Results {
    pub videos: Vec<Video>,
    pub more: Option<String>,
    /// A playlist's own title, on its first page.
    pub title: String,
    /// Channels found, as id and name.
    pub channels: Vec<(String, String)>,
    pub playlists: Vec<Playlist>,
}

impl Results {
    /// How many things were found, not counting a next page.
    pub fn found(&self) -> usize {
        self.videos.len() + self.channels.len() + self.playlists.len()
    }
}

/// A playlist on a channel's Playlists tab.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Playlist {
    pub id: String,
    pub title: String,
    /// "95 videos", as YouTube says it.
    pub count: String,
}

impl Playlist {
    pub fn url(&self) -> String {
        format!("https://www.youtube.com/playlist?list={}", self.id)
    }
}

fn post(
    path: &str,
    body: serde_json::Value,
    stop: &AtomicBool,
) -> Result<serde_json::Value, String> {
    let mut body = body;
    body["context"] = serde_json::from_str(CONTEXT).unwrap_or_default();
    let answer = net::post_json(HOST, path, &body.to_string(), stop)?
        .ok_or_else(|| "Stopped.".to_string())?;
    serde_json::from_slice(&answer).map_err(|e| format!("YouTube's answer could not be read: {e}"))
}

/// Searches YouTube, or with `more` fetches the next page.
pub fn search(query: &Query, more: Option<&str>, stop: &AtomicBool) -> Result<Results, String> {
    let body = match more {
        Some(token) => serde_json::json!({ "continuation": token }),
        None => serde_json::json!({ "query": query.text, "params": query.params() }),
    };
    let v = post("/youtubei/v1/search?prettyPrint=false", body, stop)?;
    let mut r = results_of(&v);
    if query.what == SearchFor::Playlists {
        r.playlists = lockups(&v, "", "", now()).1;
    }
    if query.no_shorts {
        r.videos.retain(|v| !v.short);
    }
    Ok(r)
}

/// The videos YouTube suggests beside a video, a page at a time.
pub fn related(id: &str, more: Option<&str>, stop: &AtomicBool) -> Result<Results, String> {
    let body = match more {
        Some(token) => serde_json::json!({ "continuation": token }),
        None => serde_json::json!({ "videoId": id }),
    };
    let v = post("/youtubei/v1/next?prettyPrint=false", body, stop)?;
    let side = &v["contents"]["twoColumnWatchNextResults"]["secondaryResults"];
    let (videos, mut playlists, more) =
        lockups(if more.is_some() { &v } else { side }, "", "", now());
    // A mix is YouTube's endless list, which cannot be listed.
    playlists.retain(|p| !p.id.starts_with("RD"));
    Ok(Results {
        videos,
        more,
        playlists,
        ..Results::default()
    })
}

/// A channel's own search, a page at a time.
pub fn channel_search(
    channel: &crate::channels::Channel,
    query: &str,
    more: Option<&str>,
    stop: &AtomicBool,
) -> Result<Results, String> {
    let body = match more {
        Some(token) => serde_json::json!({ "continuation": token }),
        None => serde_json::json!({ "browseId": channel.id, "params": SEARCH_TAB, "query": query }),
    };
    let v = post("/youtubei/v1/browse?prettyPrint=false", body, stop)?;
    let mut r = results_of(&v);
    for video in &mut r.videos {
        video.channel = channel.name.clone();
        video.channel_id = channel.id.clone();
    }
    Ok(r)
}

/// A video's comments, as one text to read: how many there are, then the
/// first pages, as YouTube sorts them.
pub fn comments(id: &str, stop: &AtomicBool) -> Result<String, String> {
    let v = post(
        "/youtubei/v1/next?prettyPrint=false",
        serde_json::json!({ "videoId": id }),
        stop,
    )?;
    let Some(mut token) = token_in(&v["contents"]["twoColumnWatchNextResults"]["results"]) else {
        return Ok("This video has no comments, or they are turned off.".into());
    };
    let mut lines = Vec::new();
    let mut total = String::new();
    for _ in 0..COMMENT_PAGES {
        let page = post(
            "/youtubei/v1/next?prettyPrint=false",
            serde_json::json!({ "continuation": token }),
            stop,
        )?;
        if total.is_empty() {
            total = comment_count(&page);
        }
        lines.extend(comments_in(&page));
        match next_comments(&page) {
            Some(t) => token = t,
            None => break,
        }
    }
    if lines.is_empty() {
        return Ok("This video has no comments, or they are turned off.".into());
    }
    let head = if total.is_empty() {
        format!("{} comments, as YouTube sorts them.", lines.len())
    } else {
        format!(
            "{total}; the first {} are here, as YouTube sorts them.",
            lines.len()
        )
    };
    Ok(format!("{head}\n\n{}", lines.join("\n\n")))
}

fn found_under<'a>(v: &'a serde_json::Value, name: &str, out: &mut Vec<&'a serde_json::Value>) {
    match v {
        serde_json::Value::Object(o) => {
            for (k, x) in o {
                if k == name {
                    out.push(x);
                } else {
                    found_under(x, name, out);
                }
            }
        }
        serde_json::Value::Array(a) => a.iter().for_each(|x| found_under(x, name, out)),
        _ => {}
    }
}

/// "5,237 Comments", from the top of the first page.
fn comment_count(v: &serde_json::Value) -> String {
    let mut found = Vec::new();
    found_under(v, "commentsHeaderRenderer", &mut found);
    found
        .first()
        .map(|h| text(&h["countText"]))
        .unwrap_or_default()
}

/// Each top-level comment on a page: who, when, and what they said.
fn comments_in(v: &serde_json::Value) -> Vec<String> {
    let mut found = Vec::new();
    found_under(v, "commentEntityPayload", &mut found);
    found
        .into_iter()
        .filter(|c| c["properties"]["replyLevel"].as_u64().unwrap_or(0) == 0)
        .filter_map(|c| {
            let said = c["properties"]["content"]["content"].as_str()?.trim();
            let mut who = vec![c["author"]["displayName"]
                .as_str()
                .unwrap_or_default()
                .trim_start_matches('@')
                .to_string()];
            who.extend(
                c["properties"]["publishedTime"]
                    .as_str()
                    .map(str::to_string),
            );
            let toolbar = &c["toolbar"];
            let count = |key: &str, one: &str, many: &str| {
                let n = toolbar[key].as_str().unwrap_or_default().trim();
                match n {
                    "" | "0" => None,
                    "1" => Some(format!("1 {one}")),
                    n => Some(format!("{n} {many}")),
                }
            };
            who.extend(count("likeCountNotliked", "like", "likes"));
            who.extend(count("replyCount", "reply", "replies"));
            Some(format!("{}:\n{said}", who.join(", ")))
        })
        .collect()
}

/// The key to the next page of comments: the last item of the list of
/// comments. Keys inside a comment are for its replies.
fn next_comments(v: &serde_json::Value) -> Option<String> {
    let mut lists = Vec::new();
    found_under(v, "continuationItems", &mut lists);
    lists
        .iter()
        .filter_map(|l| l.as_array()?.last())
        .filter(|last| last.get("continuationItemRenderer").is_some())
        .find_map(token_in)
}

/// The videos and next-page key in a search answer, in order.
fn results_of(v: &serde_json::Value) -> Results {
    let mut r = Results::default();
    walk(v, &mut r);
    r
}

fn walk(v: &serde_json::Value, r: &mut Results) {
    match v {
        serde_json::Value::Object(o) => {
            for (k, x) in o {
                match k.as_str() {
                    "videoRenderer" => {
                        if let Some(video) = video_of(x) {
                            r.videos.push(video);
                        }
                    }
                    "channelRenderer" => {
                        if let Some(id) = x["channelId"].as_str() {
                            r.channels.push((id.to_string(), text(&x["title"])));
                        }
                    }
                    "continuationItemRenderer" | "continuationItemViewModel" => {
                        if let Some(t) = token_in(x) {
                            r.more = Some(t);
                        }
                    }
                    _ => walk(x, r),
                }
            }
        }
        serde_json::Value::Array(a) => a.iter().for_each(|x| walk(x, r)),
        _ => {}
    }
}

/// The first `continuationCommand`'s key under `v`.
fn token_in(v: &serde_json::Value) -> Option<String> {
    match v {
        serde_json::Value::Object(o) => {
            if let Some(t) = o
                .get("continuationCommand")
                .and_then(|c| c["token"].as_str())
            {
                return Some(t.to_string());
            }
            o.values().find_map(token_in)
        }
        serde_json::Value::Array(a) => a.iter().find_map(token_in),
        _ => None,
    }
}

/// Text YouTube gives as `{"simpleText": ...}` or `{"runs": [{"text": ...}]}`.
fn text(v: &serde_json::Value) -> String {
    if let Some(t) = v["simpleText"].as_str() {
        return t.trim().to_string();
    }
    v["runs"]
        .as_array()
        .map(|runs| {
            runs.iter()
                .filter_map(|r| r["text"].as_str())
                .collect::<String>()
        })
        .unwrap_or_default()
        .trim()
        .to_string()
}

fn video_of(v: &serde_json::Value) -> Option<Video> {
    let id = v["videoId"].as_str()?.to_string();
    let owner = &v["ownerText"]["runs"][0];
    let live = v["badges"]
        .as_array()
        .is_some_and(|b| b.iter().any(|x| x.to_string().contains("LIVE")));
    Some(Video {
        id,
        title: text(&v["title"]),
        channel: owner["text"].as_str().unwrap_or_default().to_string(),
        channel_id: owner["navigationEndpoint"]["browseEndpoint"]["browseId"]
            .as_str()
            .unwrap_or_default()
            .to_string(),
        short: v["navigationEndpoint"]["reelWatchEndpoint"].is_object(),
        length: if live && v["lengthText"].is_null() {
            "live now".into()
        } else {
            text(&v["lengthText"])
        },
        when: text(&v["publishedTimeText"]),
        ..Video::default()
    })
}

/// What YouTube says about one video.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Details {
    pub title: String,
    pub channel: String,
    pub channel_id: String,
    pub seconds: u64,
    pub description: String,
    /// "2014-11-10".
    pub date: String,
}

/// A video's title, channel and description.
pub fn details(id: &str, stop: &AtomicBool) -> Result<Details, String> {
    let v = post(
        "/youtubei/v1/player?prettyPrint=false",
        serde_json::json!({ "videoId": id }),
        stop,
    )?;
    details_of(&v).ok_or_else(|| {
        format!(
            "YouTube did not say anything about the video {id}. It may be private or removed. {}",
            v["playabilityStatus"]["reason"]
                .as_str()
                .unwrap_or_default()
        )
        .trim()
        .to_string()
    })
}

fn details_of(v: &serde_json::Value) -> Option<Details> {
    let d = &v["videoDetails"];
    let s = |k: &str| d[k].as_str().unwrap_or_default().to_string();
    let channel_id = s("channelId");
    if channel_id.is_empty() {
        return None;
    }
    let date = v["microformat"]["playerMicroformatRenderer"]["publishDate"]
        .as_str()
        .unwrap_or_default();
    Some(Details {
        title: s("title"),
        channel: s("author"),
        channel_id,
        seconds: s("lengthSeconds").parse().unwrap_or(0),
        description: s("shortDescription"),
        date: date.get(..10).unwrap_or(date).to_string(),
    })
}

/// The channel an address belongs to: a channel's own address, by its id or
/// its handle (`/@name`), or a video's, by the video's channel. Its id and
/// name.
pub fn channel_of(url: &str, stop: &AtomicBool) -> Result<(String, String), String> {
    if let Some(id) = channel_id_in(url) {
        let (name, _) = channel_videos(&id, stop)?;
        return Ok((id, name));
    }
    if crate::ytdlp::is_youtube(url) {
        if let Some(video) = crate::ytdlp::video_id(url) {
            let d = details(&video, stop)?;
            return Ok((d.channel_id, d.channel));
        }
        let v = post(
            "/youtubei/v1/navigation/resolve_url?prettyPrint=false",
            serde_json::json!({ "url": url }),
            stop,
        )?;
        if let Some(id) = v["endpoint"]["browseEndpoint"]["browseId"]
            .as_str()
            .filter(|id| id.starts_with("UC"))
        {
            let (name, _) = channel_videos(id, stop)?;
            return Ok((id.to_string(), name));
        }
    }
    Err(format!(
        "{url} is not the address of a YouTube channel, or of one of its videos. Copy the \
         address of the channel's page, such as https://www.youtube.com/@BlenderStudio."
    ))
}

/// The "UC..." id in a `/channel/UC...` address.
pub fn channel_id_in(url: &str) -> Option<String> {
    let i = url.find("/channel/UC")?;
    let id: String = url[i + 9..]
        .chars()
        .take_while(|c| c.is_ascii_alphanumeric() || *c == '-' || *c == '_')
        .collect();
    (id.len() == 24).then_some(id)
}

/// Whether `url` is a YouTube channel's own page, rather than a video or a
/// playlist.
pub fn is_channel(url: &str) -> bool {
    crate::ytdlp::is_youtube(url)
        && crate::ytdlp::video_id(url).is_none()
        && crate::ytdlp::list_id(url).is_none()
        && (url.contains("/@")
            || url.contains("/channel/")
            || url.contains("/c/")
            || url.contains("/user/"))
}

pub fn now() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |d| d.as_secs() as i64)
}

/// A channel's name and latest videos, newest first, from its Videos tab:
/// about 30.
pub fn channel_videos(channel_id: &str, stop: &AtomicBool) -> Result<(String, Vec<Video>), String> {
    channel_page(channel_id, stop).map(|(name, r)| (name, r.videos))
}

/// A channel's name and first page of videos, with the key to the next.
pub fn channel_page(channel_id: &str, stop: &AtomicBool) -> Result<(String, Results), String> {
    let v = post(
        "/youtubei/v1/browse?prettyPrint=false",
        serde_json::json!({ "browseId": channel_id, "params": VIDEOS_TAB }),
        stop,
    )?;
    let name = channel_name(&v);
    let (videos, _, _) = lockups(&v, &name, channel_id, now());
    // The grid's own last item: other keys on the page are for side panels.
    let mut grids = Vec::new();
    found_under(&v, "richGridRenderer", &mut grids);
    let more = grids
        .first()
        .and_then(|g| g["contents"].as_array()?.last())
        .and_then(|last| last.get("continuationItemRenderer"))
        .and_then(token_in);
    if name.is_empty() && videos.is_empty() {
        return Err(format!(
            "YouTube said nothing about the channel {channel_id}. It may have been removed."
        ));
    }
    Ok((
        name,
        Results {
            videos,
            more,
            ..Results::default()
        },
    ))
}

/// The next page of a channel's videos.
pub fn channel_more(
    channel: &crate::channels::Channel,
    token: &str,
    stop: &AtomicBool,
) -> Result<Results, String> {
    let v = post(
        "/youtubei/v1/browse?prettyPrint=false",
        serde_json::json!({ "continuation": token }),
        stop,
    )?;
    let (videos, _, more) = lockups(&v, &channel.name, &channel.id, now());
    Ok(Results {
        videos,
        more,
        ..Results::default()
    })
}

/// A channel's playlists, from its Playlists tab.
pub fn channel_playlists(channel_id: &str, stop: &AtomicBool) -> Result<Vec<Playlist>, String> {
    let v = post(
        "/youtubei/v1/browse?prettyPrint=false",
        serde_json::json!({ "browseId": channel_id, "params": PLAYLISTS_TAB }),
        stop,
    )?;
    Ok(lockups(&v, "", channel_id, now()).1)
}

/// A playlist's videos, a page at a time (about 100), and the key to the
/// next page; with `more`, the next page.
pub fn playlist_videos(id: &str, more: Option<&str>, stop: &AtomicBool) -> Result<Results, String> {
    let body = match more {
        Some(token) => serde_json::json!({ "continuation": token }),
        None => serde_json::json!({ "browseId": format!("VL{id}") }),
    };
    let v = post("/youtubei/v1/browse?prettyPrint=false", body, stop)?;
    let (videos, _, more) = lockups(&v, "", "", now());
    let title = v["metadata"]["playlistMetadataRenderer"]["title"]
        .as_str()
        .unwrap_or_default()
        .to_string();
    Ok(Results {
        videos,
        more,
        title,
        ..Results::default()
    })
}

fn channel_name(v: &serde_json::Value) -> String {
    v["metadata"]["channelMetadataRenderer"]["title"]
        .as_str()
        .unwrap_or_default()
        .to_string()
}

/// The videos and playlists YouTube's newer pages list as "lockups", in
/// order, and the key to the next page. `channel`: whose they are, when the
/// page does not say each one's channel.
fn lockups(
    v: &serde_json::Value,
    channel: &str,
    channel_id: &str,
    now: i64,
) -> (Vec<Video>, Vec<Playlist>, Option<String>) {
    let mut found = Vec::new();
    let mut more = None;
    collect(v, &mut found, &mut more);
    let mut videos = Vec::new();
    let mut playlists = Vec::new();
    for l in found {
        let Some(id) = l["contentId"].as_str() else {
            continue;
        };
        let meta = &l["metadata"]["lockupMetadataViewModel"];
        let title = meta["title"]["content"]
            .as_str()
            .unwrap_or_default()
            .to_string();
        let rows: Vec<Vec<String>> = meta["metadata"]["contentMetadataViewModel"]["metadataRows"]
            .as_array()
            .into_iter()
            .flatten()
            .map(|r| {
                r["metadataParts"]
                    .as_array()
                    .into_iter()
                    .flatten()
                    .filter_map(|p| p["text"]["content"].as_str())
                    .map(str::to_string)
                    .collect()
            })
            .collect();
        let mut badges = Vec::new();
        badges_in(&l["contentImage"], &mut badges);
        match l["contentType"].as_str().unwrap_or_default() {
            "LOCKUP_CONTENT_TYPE_VIDEO" => {
                // On a playlist the first row is the channel, then views and
                // age; on a channel's tab, just views and age.
                let (by, last) = match rows.len() {
                    0 => (String::new(), Vec::new()),
                    1 => (String::new(), rows[0].clone()),
                    _ => (rows[0].join(" and "), rows[rows.len() - 1].clone()),
                };
                let when = last
                    .iter()
                    .find(|t| t.ends_with(" ago"))
                    .cloned()
                    .unwrap_or_default();
                let length = badges
                    .iter()
                    .find(|b| b.contains(':'))
                    .cloned()
                    .or_else(|| {
                        badges
                            .iter()
                            .find(|b| b.contains("LIVE"))
                            .map(|_| "live now".into())
                    })
                    .unwrap_or_default();
                videos.push(Video {
                    id: id.to_string(),
                    title,
                    channel: if by.is_empty() {
                        channel.to_string()
                    } else {
                        by
                    },
                    channel_id: channel_id.to_string(),
                    length,
                    published: published(now, &when),
                    when,
                    ..Video::default()
                });
            }
            "LOCKUP_CONTENT_TYPE_PLAYLIST" => playlists.push(Playlist {
                id: id.to_string(),
                title,
                count: badges
                    .iter()
                    .find(|b| b.contains("video") || b.contains("episode"))
                    .cloned()
                    .unwrap_or_default(),
            }),
            _ => {}
        }
    }
    (videos, playlists, more)
}

fn collect<'a>(
    v: &'a serde_json::Value,
    found: &mut Vec<&'a serde_json::Value>,
    more: &mut Option<String>,
) {
    match v {
        serde_json::Value::Object(o) => {
            for (k, x) in o {
                match k.as_str() {
                    "lockupViewModel" => found.push(x),
                    "continuationItemRenderer" | "continuationItemViewModel" => {
                        if let Some(t) = token_in(x) {
                            *more = Some(t);
                        }
                    }
                    // The sort and filter buttons: their keys are not the
                    // next page.
                    "chipBarViewModel"
                    | "chipCloudRenderer"
                    | "sortFilterSubMenuRenderer"
                    | "engagementPanels" => {}
                    _ => collect(x, found, more),
                }
            }
        }
        serde_json::Value::Array(a) => a.iter().for_each(|x| collect(x, found, more)),
        _ => {}
    }
}

fn badges_in(v: &serde_json::Value, out: &mut Vec<String>) {
    match v {
        serde_json::Value::Object(o) => {
            if let Some(t) = o
                .get("thumbnailBadgeViewModel")
                .and_then(|b| b["text"].as_str())
            {
                out.push(t.to_string());
            }
            o.values().for_each(|x| badges_in(x, out));
        }
        serde_json::Value::Array(a) => a.iter().for_each(|x| badges_in(x, out)),
        _ => {}
    }
}

/// "5 days ago", "Streamed 2 weeks ago", as seconds since 1970, about. Zero
/// when it says no age.
pub fn published(now: i64, when: &str) -> i64 {
    let words: Vec<&str> = when.split_whitespace().collect();
    let Some(i) = words.iter().position(|w| *w == "ago") else {
        return 0;
    };
    if i < 2 {
        return 0;
    }
    let Ok(n) = words[i - 2].parse::<i64>() else {
        return 0;
    };
    let unit = match words[i - 1].trim_end_matches('s') {
        "second" => 1,
        "minute" => 60,
        "hour" => 3600,
        "day" => 86_400,
        "week" => 7 * 86_400,
        "month" => 30 * 86_400,
        "year" => 365 * 86_400,
        _ => return 0,
    };
    now - n * unit
}

/// How long ago, in words, as YouTube says it.
pub fn ago(now: i64, then: i64) -> String {
    if then == 0 {
        return String::new();
    }
    let s = (now - then).max(0);
    let plural = |n: i64, unit: &str| {
        if n == 1 {
            format!("1 {unit} ago")
        } else {
            format!("{n} {unit}s ago")
        }
    };
    match s {
        s if s < 60 => "just now".into(),
        s if s < 3600 => plural((s / 60).max(1), "minute"),
        s if s < 86_400 => plural(s / 3600, "hour"),
        s if s < 7 * 86_400 => plural(s / 86_400, "day"),
        s if s < 31 * 86_400 => plural(s / (7 * 86_400), "week"),
        s if s < 365 * 86_400 => plural(s / (30 * 86_400), "month"),
        s => plural(s / (365 * 86_400), "year"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn words(text: &str) -> Query {
        Query {
            text: text.into(),
            ..Query::default()
        }
    }

    /// Everything new asked of YouTube, against the real thing.
    #[test]
    #[ignore]
    fn youtube_answers_the_new_questions() {
        let stop = AtomicBool::new(false);
        let found = |what| {
            search(
                &Query {
                    text: "blender".into(),
                    what,
                    ..Query::default()
                },
                None,
                &stop,
            )
            .unwrap()
        };
        let channels = found(SearchFor::Channels);
        println!(
            "channels {} {:?}",
            channels.channels.len(),
            channels.channels.first()
        );
        assert!(!channels.channels.is_empty());
        let playlists = found(SearchFor::Playlists);
        println!(
            "playlists {} {:?}",
            playlists.playlists.len(),
            playlists.playlists.first()
        );
        assert!(!playlists.playlists.is_empty());
        let r = related("aqz-KE-bpKQ", None, &stop).unwrap();
        println!("related {} more {}", r.videos.len(), r.more.is_some());
        assert!(!r.videos.is_empty());
        let m = related("aqz-KE-bpKQ", r.more.as_deref(), &stop).unwrap();
        println!("related next {}", m.videos.len());
        assert!(!m.videos.is_empty());
        let c = comments("aqz-KE-bpKQ", &stop).unwrap();
        println!("comments: {}", c.chars().take(300).collect::<String>());
        assert!(c.matches(":\n").count() > 40);
        let (name, page) = channel_page("UCSMOQeBJ2RAnuFungnQOxLg", &stop).unwrap();
        println!(
            "channel {name} {} more {}",
            page.videos.len(),
            page.more.is_some()
        );
        let channel = crate::channels::Channel::named("UCSMOQeBJ2RAnuFungnQOxLg", &name);
        let older = channel_more(&channel, page.more.as_deref().unwrap(), &stop).unwrap();
        println!(
            "older {} {:?}",
            older.videos.len(),
            older.videos.first().map(|v| v.line())
        );
        assert!(!older.videos.is_empty());
        let s = channel_search(&channel, "geometry", None, &stop).unwrap();
        println!(
            "channel search {} more {}",
            s.videos.len(),
            s.more.is_some()
        );
        assert!(!s.videos.is_empty());
        let p = playlist_videos("PLuaAWflRSLYNAo4ER_MVItiLkDm6TaJNV", None, &stop).unwrap();
        println!("playlist {:?} {}", p.title, p.videos.len());
        assert!(!p.title.is_empty());
    }

    #[test]
    fn search_filters_are_youtubes_own() {
        let q = |what, newest, length| Query {
            what,
            newest,
            length,
            ..Query::default()
        };
        assert_eq!(q(SearchFor::Videos, false, 0).params(), "EgIQAQ==");
        assert_eq!(q(SearchFor::Channels, false, 0).params(), "EgIQAg==");
        assert_eq!(q(SearchFor::Playlists, false, 0).params(), "EgIQAw==");
        assert_eq!(q(SearchFor::Videos, true, 0).params(), "CAISAhAB");
        assert_eq!(q(SearchFor::Videos, false, 3).params(), "EgQQARgD");
        assert_eq!(q(SearchFor::Videos, true, 2).params(), "CAISBBABGAI=");
    }

    #[test]
    fn comments_are_read_with_who_and_when() {
        let v = serde_json::json!({"m": [{"commentEntityPayload": {
            "properties": {"content": {"content": "Nice"}, "publishedTime": "2 years ago", "replyLevel": 0},
            "author": {"displayName": "@Seed"},
            "toolbar": {"likeCountNotliked": "1.9K", "replyCount": "1"}
        }}, {"commentEntityPayload": {
            "properties": {"content": {"content": "A reply"}, "replyLevel": 1},
            "author": {"displayName": "@Other"}, "toolbar": {}
        }}]});
        assert_eq!(
            comments_in(&v),
            ["Seed, 2 years ago, 1.9K likes, 1 reply:\nNice"]
        );
    }

    #[test]
    fn the_next_comments_are_not_a_comments_replies() {
        let v = serde_json::json!({"a": {"continuationItems": [
            {"commentThreadRenderer": {"replies": {"continuationItemRenderer": {"continuationEndpoint": {"continuationCommand": {"token": "reply"}}}}}},
            {"continuationItemRenderer": {"continuationEndpoint": {"continuationCommand": {"token": "next"}}}}
        ]}});
        assert_eq!(next_comments(&v).as_deref(), Some("next"));
    }

    #[test]
    fn search_results_are_read_in_order() {
        let v: serde_json::Value = serde_json::from_str(
            r#"{"contents":{"x":[
              {"videoRenderer":{"videoId":"WhWc3b3KhnY","title":{"runs":[{"text":"Spring - Blender Open Movie"}]},
                "ownerText":{"runs":[{"text":"Blender Studio","navigationEndpoint":{"browseEndpoint":{"browseId":"UCz75RVbH8q2jdBJ4SnwuZZQ"}}}]},
                "lengthText":{"simpleText":"7:45"},"publishedTimeText":{"simpleText":"7 years ago"}}},
              {"channelRenderer":{"channelId":"UCx"}},
              {"videoRenderer":{"videoId":"abcdefghijk","title":{"runs":[{"text":"Live"}]},
                "badges":[{"metadataBadgeRenderer":{"label":"LIVE"}}]}},
              {"continuationItemRenderer":{"continuationEndpoint":{"continuationCommand":{"token":"NEXT"}}}}
            ]}}"#,
        )
        .unwrap();
        let r = results_of(&v);
        assert_eq!(r.videos.len(), 2);
        assert_eq!(
            r.videos[0].line(),
            "Spring - Blender Open Movie: Blender Studio, 7:45, 7 years ago"
        );
        assert_eq!(r.videos[0].channel_id, "UCz75RVbH8q2jdBJ4SnwuZZQ");
        assert_eq!(r.videos[1].length, "live now");
        assert_eq!(r.more.as_deref(), Some("NEXT"));
    }

    #[test]
    fn ages_are_read_and_said() {
        assert_eq!(published(1_000_000, "5 days ago"), 1_000_000 - 5 * 86_400);
        assert_eq!(
            published(1_000_000, "Streamed 1 hour ago"),
            1_000_000 - 3600
        );
        assert_eq!(published(1_000_000, "Premieres soon"), 0);
        assert_eq!(ago(100_000, 100_000 - 2 * 86_400), "2 days ago");
        assert_eq!(ago(100_000, 0), "");
    }

    #[test]
    fn a_channels_videos_and_playlists_are_read() {
        let v: serde_json::Value = serde_json::from_str(
            r#"{"metadata":{"channelMetadataRenderer":{"title":"Blender"}},"contents":[
              {"chipBarViewModel":{"x":{"continuationCommand":{"token":"CHIP"}}}},
              {"lockupViewModel":{"contentId":"mHrHLqQj7Ts","contentType":"LOCKUP_CONTENT_TYPE_VIDEO",
                "contentImage":{"thumbnailViewModel":{"overlays":[{"thumbnailBottomOverlayViewModel":
                  {"badges":[{"thumbnailBadgeViewModel":{"text":"4:40"}}]}}]}},
                "metadata":{"lockupMetadataViewModel":{"title":{"content":"Making of"},
                  "metadata":{"contentMetadataViewModel":{"metadataRows":[{"metadataParts":[
                    {"text":{"content":"16K views"}},{"text":{"content":"5 days ago"}}]}]}}}}}},
              {"lockupViewModel":{"contentId":"PLa1","contentType":"LOCKUP_CONTENT_TYPE_PLAYLIST",
                "contentImage":{"collectionThumbnailViewModel":{"x":[{"thumbnailBadgeViewModel":{"text":"95 videos"}}]}},
                "metadata":{"lockupMetadataViewModel":{"title":{"content":"Blender Conference 2026"}}}}},
              {"continuationItemRenderer":{"continuationEndpoint":{"continuationCommand":{"token":"NEXT"}}}}
            ]}"#,
        )
        .unwrap();
        assert_eq!(channel_name(&v), "Blender");
        let (videos, playlists, more) = lockups(&v, "Blender", "UCx", 1_000_000);
        assert_eq!(videos.len(), 1);
        assert_eq!(videos[0].line(), "Making of: Blender, 4:40, 5 days ago");
        assert_eq!(videos[0].published, 1_000_000 - 5 * 86_400);
        assert_eq!(playlists[0].count, "95 videos");
        assert_eq!(more.as_deref(), Some("NEXT"));
    }

    #[test]
    fn the_filter_buttons_keys_are_not_the_next_page() {
        let v: serde_json::Value = serde_json::from_str(
            r#"{"a":[{"continuationItemRenderer":{"continuationEndpoint":{"continuationCommand":{"token":"NEXT"}}}}],
                "b":{"chipCloudChipRenderer":{"navigationEndpoint":{"continuationCommand":{"token":"CHIP"}}}}}"#,
        )
        .unwrap();
        assert_eq!(results_of(&v).more.as_deref(), Some("NEXT"));
    }

    #[test]
    fn channel_addresses_are_recognised() {
        assert_eq!(
            channel_id_in("https://www.youtube.com/channel/UCz75RVbH8q2jdBJ4SnwuZZQ/videos")
                .as_deref(),
            Some("UCz75RVbH8q2jdBJ4SnwuZZQ")
        );
        assert!(is_channel("https://www.youtube.com/@BlenderStudio"));
        assert!(!is_channel("https://www.youtube.com/watch?v=jNQXAC9IVRw"));
        assert!(!is_channel("https://www.youtube.com/playlist?list=PLx"));
    }

    #[test]
    fn details_say_whose_video_it_is() {
        let v: serde_json::Value = serde_json::from_str(
            r#"{"videoDetails":{"title":"Big Buck Bunny","author":"Blender","channelId":"UCSMOQeBJ2RAnuFungnQOxLg",
                "lengthSeconds":"635","shortDescription":"UHD"},
               "microformat":{"playerMicroformatRenderer":{"publishDate":"2014-11-10T06:05:55-08:00"}}}"#,
        )
        .unwrap();
        let d = details_of(&v).unwrap();
        assert_eq!(d.channel, "Blender");
        assert_eq!(d.seconds, 635);
        assert_eq!(d.date, "2014-11-10");
    }

    /// Needs the network: `cargo test live_youtube -- --ignored --nocapture`.
    #[test]
    #[ignore]
    fn live_youtube() {
        let stop = AtomicBool::new(false);
        let r = search(&words("blender open movie"), None, &stop).unwrap();
        assert!(r.videos.len() > 5, "{r:?}");
        println!("{}", r.videos[0].line());
        let more = search(&words(""), r.more.as_deref(), &stop).unwrap();
        assert!(!more.videos.is_empty());
        let (id, name) = channel_of("https://www.youtube.com/@BlenderStudio", &stop).unwrap();
        assert_eq!(id, "UCz75RVbH8q2jdBJ4SnwuZZQ");
        println!("{name}");
        let (_, v) = channel_videos(&id, &stop).unwrap();
        assert!(v.len() > 5, "{v:?}");
        println!("{}", v[0].line());
        let p = channel_playlists(&id, &stop).unwrap();
        assert!(!p.is_empty());
        let pv = playlist_videos(&p[0].id, None, &stop).unwrap();
        assert!(!pv.videos.is_empty());
        println!("{}: {}", p[0].title, pv.videos[0].line());
        assert!(
            more.videos.len() >= 10,
            "the second page: {}",
            more.videos.len()
        );
        let d = details("aqz-KE-bpKQ", &stop).unwrap();
        assert_eq!(d.channel_id, "UCSMOQeBJ2RAnuFungnQOxLg");
    }
}
