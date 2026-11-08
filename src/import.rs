//! Channels brought in from elsewhere, and a backup of those followed
//! (log entry 6, B3): a Google Takeout `subscriptions.csv`, a FreeTube or
//! NewPipe export, an OPML list, or VidFetch's own backup, which is written
//! in NewPipe's form so that NewPipe and FreeTube can read it too.

use crate::channels::Channel;

/// The channels in an export file, as id and name, each once.
pub fn parse(text: &str) -> Vec<(String, String)> {
    let text = text.trim_start_matches('\u{feff}').trim();
    let mut out = Vec::new();
    if let Ok(v) = serde_json::from_str::<serde_json::Value>(text) {
        json(&v, &mut out);
    } else if text.starts_with('<') {
        opml(text, &mut out);
    } else if text.starts_with('{') {
        // FreeTube's own file: one object to a line.
        for line in text.lines() {
            if let Ok(v) = serde_json::from_str::<serde_json::Value>(line) {
                json(&v, &mut out);
            }
        }
    } else {
        csv(text, &mut out);
    }
    let mut seen = std::collections::HashSet::new();
    out.retain(|(id, _)| seen.insert(id.clone()));
    out
}

/// The "UC..." id in a channel's address, or the id itself.
fn id_in(text: &str) -> Option<String> {
    let at = text.find("UC")?;
    let id: String = text[at..]
        .chars()
        .take_while(|c| c.is_ascii_alphanumeric() || *c == '-' || *c == '_')
        .collect();
    (id.len() == 24).then_some(id)
}

/// NewPipe: `subscriptions`, each with `url` and `name`. FreeTube: each with
/// `id` and `name`, under profiles.
fn json(v: &serde_json::Value, out: &mut Vec<(String, String)>) {
    match v {
        serde_json::Value::Object(o) => {
            if let Some(list) = o.get("subscriptions").and_then(|s| s.as_array()) {
                for s in list {
                    let id = s["id"]
                        .as_str()
                        .or_else(|| s["url"].as_str())
                        .and_then(id_in);
                    if let Some(id) = id {
                        let name = s["name"].as_str().unwrap_or(&id).to_string();
                        out.push((id, name));
                    }
                }
            } else {
                o.values().for_each(|x| json(x, out));
            }
        }
        serde_json::Value::Array(a) => a.iter().for_each(|x| json(x, out)),
        _ => {}
    }
}

/// `<outline title="Name" xmlUrl="...channel_id=UC..."/>`.
fn opml(text: &str, out: &mut Vec<(String, String)>) {
    for part in text.split("<outline").skip(1) {
        let attribute = |name: &str| {
            let at = part.find(&format!("{name}=\""))? + name.len() + 2;
            Some(part[at..].split('"').next()?.to_string())
        };
        if let Some(id) = attribute("xmlUrl").as_deref().and_then(id_in) {
            let name = attribute("title")
                .or_else(|| attribute("text"))
                .unwrap_or_else(|| id.clone())
                .replace("&amp;", "&");
            out.push((id, name));
        }
    }
}

/// Takeout: `Channel Id,Channel Url,Channel Title`, in whatever language.
fn csv(text: &str, out: &mut Vec<(String, String)>) {
    for line in text.lines() {
        let mut cells = line.splitn(3, ',');
        let (Some(first), Some(_), Some(name)) = (cells.next(), cells.next(), cells.next()) else {
            continue;
        };
        if let Some(id) = id_in(first) {
            out.push((id, name.trim().trim_matches('"').replace("\"\"", "\"")));
        }
    }
}

/// The channels followed, as a file NewPipe and FreeTube can read too.
pub fn backup(channels: &[Channel]) -> String {
    let list: Vec<serde_json::Value> = channels
        .iter()
        .map(|c| {
            serde_json::json!({
                "service_id": 0,
                "url": format!("https://www.youtube.com/channel/{}", c.id),
                "name": c.name,
            })
        })
        .collect();
    let v = serde_json::json!({
        "app_version": env!("CARGO_PKG_VERSION"),
        "app_version_int": 1,
        "subscriptions": list,
    });
    serde_json::to_string_pretty(&v).unwrap_or_default()
}

#[cfg(test)]
mod tests {
    use super::*;

    const A: &str = "UCaaaaaaaaaaaaaaaaaaaaaa";
    const B: &str = "UCbbbbbbbbbbbbbbbbbbbbbb";

    fn both() -> Vec<(String, String)> {
        vec![
            (A.to_string(), "One".to_string()),
            (B.to_string(), "Two, too".to_string()),
        ]
    }

    #[test]
    fn takeout() {
        let text = format!(
            "\u{feff}Channel Id,Channel Url,Channel Title\n{A},http://www.youtube.com/channel/{A},One\n{B},http://www.youtube.com/channel/{B},\"Two, too\"\n"
        );
        assert_eq!(parse(&text), both());
    }

    #[test]
    fn newpipe_and_our_own_backup() {
        let channels = [Channel::named(A, "One"), Channel::named(B, "Two, too")];
        assert_eq!(parse(&backup(&channels)), both());
    }

    #[test]
    fn freetube_one_object_to_a_line() {
        let text = format!(
            "{{\"_id\":\"allChannels\",\"subscriptions\":[{{\"id\":\"{A}\",\"name\":\"One\"}}]}}\n{{\"_id\":\"x\",\"subscriptions\":[{{\"id\":\"{B}\",\"name\":\"Two, too\"}},{{\"id\":\"{A}\",\"name\":\"One\"}}]}}\n"
        );
        assert_eq!(parse(&text), both());
    }

    #[test]
    fn opml_list() {
        let text = format!(
            "<opml><body><outline text=\"YouTube\"><outline title=\"One\" xmlUrl=\"https://www.youtube.com/feeds/videos.xml?channel_id={A}\"/><outline title=\"Two, too\" xmlUrl=\"https://www.youtube.com/feeds/videos.xml?channel_id={B}\"/></outline></body></opml>"
        );
        assert_eq!(parse(&text), both());
    }

    #[test]
    fn something_else_holds_no_channels() {
        assert!(parse("just some text, with, commas").is_empty());
    }
}
