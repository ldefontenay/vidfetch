//! YouTube asked directly, for what yt-dlp is slow at: searching, a video's
//! details, finding a channel from an address, and a channel's latest videos.
//! Each takes about a second, where yt-dlp takes 5 to 15. Downloading and
//! listening still go through yt-dlp, which keeps up with YouTube's changes
//! (log entry 3, Q3: "Yes, go with C."). If YouTube changes what it sends,
//! search falls back to yt-dlp (`ytdlp::search`).
//!
//! The search and details come from YouTube's own web API, as its pages and
//! NewPipe use it, without signing in; a channel's latest videos come from
//! its public feed.

use std::sync::atomic::AtomicBool;

use crate::net;

const HOST: &str = "www.youtube.com";
/// Who VidFetch says it is to YouTube's API: its web page. English, so
/// lengths and dates read the same everywhere.
const CONTEXT: &str =
    r#"{"client":{"clientName":"WEB","clientVersion":"2.20251101.00.00","hl":"en"}}"#;
/// YouTube's search filter for videos only.
const VIDEOS_ONLY: &str = "EgIQAQ==";

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
    /// Seconds since 1970, from a channel's feed, for the newest first.
    pub published: i64,
    /// From a channel's feed: the description comes with it.
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

/// Searches YouTube for videos, or with `more` fetches the next page.
pub fn search(query: &str, more: Option<&str>, stop: &AtomicBool) -> Result<Results, String> {
    let body = match more {
        Some(token) => serde_json::json!({ "continuation": token }),
        None => serde_json::json!({ "query": query, "params": VIDEOS_ONLY }),
    };
    let v = post("/youtubei/v1/search?prettyPrint=false", body, stop)?;
    Ok(results_of(&v))
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
                    "continuationCommand" => {
                        if let Some(t) = x["token"].as_str() {
                            r.more = Some(t.to_string());
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
        let (name, _) = feed(&id, stop)?;
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
            let (name, _) = feed(id, stop)?;
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

/// A channel's name and latest videos (YouTube gives the last 15), newest
/// first, from its public feed.
pub fn feed(channel_id: &str, stop: &AtomicBool) -> Result<(String, Vec<Video>), String> {
    let path = format!("/feeds/videos.xml?channel_id={channel_id}");
    let body =
        net::download(HOST, &path, stop, &mut |_, _| {})?.ok_or_else(|| "Stopped.".to_string())?;
    let xml = String::from_utf8_lossy(&body);
    feed_of(&xml, now()).ok_or_else(|| {
        format!("YouTube's list of the channel's latest videos ({channel_id}) could not be read.")
    })
}

pub fn now() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |d| d.as_secs() as i64)
}

/// The text between `<tag ...>` and `</tag>` in `xml`, from `from`; and where
/// it ends.
fn element<'a>(xml: &'a str, tag: &str, from: usize) -> Option<(&'a str, usize)> {
    let open = format!("<{tag}");
    let mut at = from;
    loop {
        let i = at + xml[at..].find(&open)?;
        let after = i + open.len();
        // `<title>`, not `<titleSomething>`.
        if xml[after..].starts_with('>') || xml[after..].starts_with(' ') {
            let start = after + xml[after..].find('>')? + 1;
            let close = format!("</{tag}>");
            let end = start + xml[start..].find(&close)?;
            return Some((&xml[start..end], end + close.len()));
        }
        at = after;
    }
}

/// XML's escapes undone.
fn unescape(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    let mut rest = s;
    while let Some(i) = rest.find('&') {
        out.push_str(&rest[..i]);
        rest = &rest[i..];
        let Some(end) = rest.find(';').filter(|&e| e <= 10) else {
            out.push('&');
            rest = &rest[1..];
            continue;
        };
        let name = &rest[1..end];
        let c = match name {
            "amp" => Some('&'),
            "lt" => Some('<'),
            "gt" => Some('>'),
            "quot" => Some('"'),
            "apos" => Some('\''),
            n if n.starts_with("#x") => u32::from_str_radix(&n[2..], 16)
                .ok()
                .and_then(char::from_u32),
            n if n.starts_with('#') => n[1..].parse().ok().and_then(char::from_u32),
            _ => None,
        };
        match c {
            Some(c) => {
                out.push(c);
                rest = &rest[end + 1..];
            }
            None => {
                out.push('&');
                rest = &rest[1..];
            }
        }
    }
    out.push_str(rest);
    out
}

fn feed_of(xml: &str, now: i64) -> Option<(String, Vec<Video>)> {
    let first_entry = xml.find("<entry>").unwrap_or(xml.len());
    let (name, _) = element(&xml[..first_entry], "title", 0)?;
    let name = unescape(name);
    let mut videos = Vec::new();
    let mut at = 0;
    while let Some((entry, end)) = element(xml, "entry", at) {
        at = end;
        let get = |tag: &str| {
            element(entry, tag, 0)
                .map(|(t, _)| unescape(t))
                .unwrap_or_default()
        };
        let id = get("yt:videoId");
        if id.is_empty() {
            continue;
        }
        let published = parse_time(&get("published")).unwrap_or(0);
        videos.push(Video {
            id,
            title: get("title"),
            channel: element(entry, "author", 0)
                .and_then(|(a, _)| element(a, "name", 0))
                .map(|(n, _)| unescape(n))
                .unwrap_or_else(|| name.clone()),
            channel_id: get("yt:channelId"),
            length: String::new(),
            when: ago(now, published),
            published,
            description: get("media:description"),
            short: entry.contains("/shorts/"),
        });
    }
    videos.sort_by_key(|v| std::cmp::Reverse(v.published));
    Some((name, videos))
}

/// "2026-11-05T14:33:32+00:00" as seconds since 1970.
fn parse_time(t: &str) -> Option<i64> {
    let n = |a: usize, b: usize| t.get(a..b)?.parse::<i64>().ok();
    let (y, mo, d) = (n(0, 4)?, n(5, 7)?, n(8, 10)?);
    let (h, mi, s) = (n(11, 13)?, n(14, 16)?, n(17, 19)?);
    // Days since 1970, by the civil calendar (Howard Hinnant's method).
    let y2 = if mo <= 2 { y - 1 } else { y };
    let era = y2.div_euclid(400);
    let yoe = y2 - era * 400;
    let doy = (153 * (mo + if mo > 2 { -3 } else { 9 }) + 2) / 5 + d - 1;
    let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy;
    let days = era * 146_097 + doe - 719_468;
    let mut secs = days * 86_400 + h * 3600 + mi * 60 + s;
    // The offset from UTC, "+01:00".
    if let (Some(sign), Some(oh), Some(om)) = (t.get(19..20), n(20, 22), n(23, 25)) {
        let off = oh * 3600 + om * 60;
        secs += if sign == "+" {
            -off
        } else if sign == "-" {
            off
        } else {
            0
        };
    }
    Some(secs)
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
    fn a_feed_is_read_newest_first() {
        let xml = r#"<?xml version="1.0"?><feed><title>Blender &amp; Friends</title>
          <entry><yt:videoId>aaaaaaaaaaa</yt:videoId><yt:channelId>UCz</yt:channelId>
            <title>Older</title><link rel="alternate" href="https://www.youtube.com/watch?v=aaaaaaaaaaa"/>
            <author><name>Blender &amp; Friends</name></author>
            <published>2026-11-01T12:00:00+00:00</published>
            <media:group><media:description>It&#39;s &quot;old&quot;</media:description></media:group></entry>
          <entry><yt:videoId>bbbbbbbbbbb</yt:videoId><title>Newer</title>
            <link rel="alternate" href="https://www.youtube.com/shorts/bbbbbbbbbbb"/>
            <published>2026-11-06T12:00:00+00:00</published></entry>
        </feed>"#;
        let now = parse_time("2026-11-07T12:00:00+00:00").unwrap();
        let (name, v) = feed_of(xml, now).unwrap();
        assert_eq!(name, "Blender & Friends");
        assert_eq!(v.len(), 2);
        assert_eq!(v[0].title, "Newer");
        assert!(v[0].short);
        assert_eq!(v[0].line(), "Newer: Blender & Friends, a short, 1 day ago");
        assert_eq!(v[1].description, "It's \"old\"");
        assert_eq!(v[1].when, "6 days ago");
    }

    #[test]
    fn times_are_read_with_their_offset() {
        assert_eq!(parse_time("1970-01-02T00:00:00+00:00"), Some(86_400));
        assert_eq!(parse_time("1970-01-02T01:00:00+01:00"), Some(86_400));
        assert_eq!(parse_time("2000-03-01T00:00:00+00:00"), Some(951_868_800));
        assert_eq!(ago(100_000, 100_000 - 2 * 86_400), "2 days ago");
        assert_eq!(ago(100_000, 0), "");
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
        let r = search("blender open movie", None, &stop).unwrap();
        assert!(r.videos.len() > 5, "{r:?}");
        println!("{}", r.videos[0].line());
        let more = search("", r.more.as_deref(), &stop).unwrap();
        assert!(!more.videos.is_empty());
        let (id, name) = channel_of("https://www.youtube.com/@BlenderStudio", &stop).unwrap();
        assert_eq!(id, "UCz75RVbH8q2jdBJ4SnwuZZQ");
        println!("{name}");
        let (_, v) = feed(&id, &stop).unwrap();
        assert!(!v.is_empty());
        println!("{}", v[0].line());
        let d = details("aqz-KE-bpKQ", &stop).unwrap();
        assert_eq!(d.channel_id, "UCSMOQeBJ2RAnuFungnQOxLg");
    }
}
