//! Email parsing: sender, text, and links with their labels (port of linuxautofill's
//! `imap_source.parse_message` / `parse_html`).

use crate::detect::{LINK_CONTEXT, Link, plain_links};

/// Mail older than this is not a fresh verification code.
pub const MAX_AGE_SECS: i64 = 10 * 60;

pub struct Parsed {
    pub sender: String,
    pub text: String,
    pub links: Vec<Link>,
}

/// Returns `None` for unparseable or stale mail.
pub fn parse_message(raw: &[u8], now_unix: i64) -> Option<Parsed> {
    let message = mail_parser::MessageParser::default().parse(raw)?;
    if let Some(date) = message.date()
        && now_unix - date.to_timestamp() > MAX_AGE_SECS
    {
        return None;
    }
    let sender = message
        .from()
        .and_then(|list| list.first())
        .and_then(|m| m.name().or(m.address()).map(str::to_owned))
        .unwrap_or_default();
    let subject = message.subject().unwrap_or_default().to_string();
    let html = message.body_html(0).map(|h| h.into_owned());
    let plain = message.body_text(0).map(|t| t.into_owned());
    let (html_text, mut links) = html.as_deref().map(parse_html).unwrap_or_default();
    let body = plain.clone().unwrap_or_else(|| html_text.clone());
    for text in [plain.as_deref().unwrap_or_default(), html_text.as_str()] {
        for link in plain_links(text) {
            if !links.iter().any(|l| l.url == link.url) {
                links.push(link);
            }
        }
    }
    Some(Parsed { sender, text: format!("{subject}\n{body}"), links })
}

fn decode_entities(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    let mut rest = text;
    while let Some(amp) = rest.find('&') {
        out.push_str(&rest[..amp]);
        rest = &rest[amp..];
        let Some(semi) = rest[..rest.len().min(12)].find(';') else {
            out.push('&');
            rest = &rest[1..];
            continue;
        };
        let entity = &rest[1..semi];
        let decoded = match entity {
            "amp" => Some('&'),
            "lt" => Some('<'),
            "gt" => Some('>'),
            "quot" => Some('"'),
            "apos" | "#39" => Some('\''),
            "nbsp" => Some(' '),
            e if e.starts_with("#x") || e.starts_with("#X") => u32::from_str_radix(&e[2..], 16).ok().and_then(char::from_u32),
            e if e.starts_with('#') => e[1..].parse().ok().and_then(char::from_u32),
            _ => None,
        };
        match decoded {
            Some(c) => {
                out.push(c);
                rest = &rest[semi + 1..];
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

fn attr(tag: &str, name: &str) -> Option<String> {
    let lower = tag.to_ascii_lowercase();
    let mut search = 0;
    while let Some(found) = lower[search..].find(name) {
        let at = search + found;
        search = at + name.len();
        let before_ok = at == 0 || lower.as_bytes()[at - 1].is_ascii_whitespace();
        let after = lower[at + name.len()..].trim_start();
        if !before_ok || !after.starts_with('=') {
            continue;
        }
        let value_start = tag.len() - after.len() + 1;
        let value = tag[value_start..].trim_start();
        let (quote, body) = match value.chars().next()? {
            q @ ('"' | '\'') => (Some(q), &value[1..]),
            _ => (None, value),
        };
        let end = match quote {
            Some(q) => body.find(q)?,
            None => body.find(|c: char| c.is_whitespace() || c == '>').unwrap_or(body.len()),
        };
        return Some(decode_entities(&body[..end]));
    }
    None
}

/// Visible text of an HTML body plus every `<a href>` with its label and preceding text.
pub fn parse_html(html: &str) -> (String, Vec<Link>) {
    let mut text = String::new();
    let mut links = Vec::new();
    let mut current: Option<(String, String, String)> = None; // url, label, context
    let mut skip_until: Option<&str> = None;
    let mut rest = html;
    while !rest.is_empty() {
        let Some(lt) = rest.find('<') else {
            push_text(&mut text, &mut current, &decode_entities(rest));
            break;
        };
        if lt > 0 && skip_until.is_none() {
            push_text(&mut text, &mut current, &decode_entities(&rest[..lt]));
        }
        rest = &rest[lt..];
        if rest.starts_with("<!--") {
            rest = rest.find("-->").map_or("", |e| &rest[e + 3..]);
            continue;
        }
        let Some(gt) = rest.find('>') else { break };
        let tag = &rest[1..gt];
        rest = &rest[gt + 1..];
        let name: String = tag.trim_start_matches('/').chars().take_while(|c| c.is_ascii_alphanumeric()).collect::<String>().to_ascii_lowercase();
        let closing = tag.starts_with('/');
        if let Some(until) = skip_until {
            if closing && name == until {
                skip_until = None;
            }
            continue;
        }
        match (name.as_str(), closing) {
            ("script", false) => skip_until = Some("script"),
            ("style", false) => skip_until = Some("style"),
            ("a", false) => {
                if let Some(href) = attr(tag, "href") {
                    let chars: Vec<char> = text.chars().collect();
                    let context: String = chars[chars.len().saturating_sub(LINK_CONTEXT)..].iter().collect();
                    current = Some((href.trim().to_string(), String::new(), context));
                }
            }
            ("a", true) => {
                if let Some((url, label, context)) = current.take() {
                    links.push(Link::new(url, label.split_whitespace().collect::<Vec<_>>().join(" "), context));
                }
            }
            ("img", false) => {
                if let (Some(alt), Some((_, label, _))) = (attr(tag, "alt"), current.as_mut()) {
                    label.push_str(&alt);
                }
            }
            ("br" | "p" | "div" | "tr" | "li" | "h1" | "h2" | "h3" | "table", _) => text.push('\n'),
            _ => {}
        }
    }
    if let Some((url, label, context)) = current.take() {
        links.push(Link::new(url, label, context));
    }
    (text, links)
}

fn push_text(text: &mut String, current: &mut Option<(String, String, String)>, chunk: &str) {
    text.push_str(chunk);
    if let Some((_, label, _)) = current {
        label.push_str(chunk);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::detect::{find_code, find_link};

    #[test]
    fn html_links_have_labels_and_context() {
        let html = r#"<html><style>.x{}</style><p>To verify your email &amp; secure it, click below.</p>
            <a href="https://click.example/b2" class="btn">Verify <b>My</b> Email</a>
            <a href='https://click.example/u'><img alt="Unsubscribe" src="x.png"></a></html>"#;
        let (text, links) = parse_html(html);
        assert!(text.contains("verify your email & secure"));
        assert!(!text.contains(".x{}"));
        assert_eq!(links[0].label, "Verify My Email");
        assert!(links[0].context.contains("click below."));
        assert_eq!(links[1].label, "Unsubscribe");
        assert_eq!(find_link(&links, &text).as_deref(), Some("https://click.example/b2"));
    }

    #[test]
    fn parses_fresh_mail_and_skips_stale() {
        let raw = b"From: Acme <no-reply@acme.example>\r\nSubject: Your Acme code\r\nDate: Mon, 6 Nov 2023 10:00:00 +0000\r\nContent-Type: text/plain\r\n\r\nYour verification code is 482913.\r\n";
        let sent = 1_699_264_800; // the Date above
        let parsed = parse_message(raw, sent + 60).unwrap();
        assert_eq!(parsed.sender, "Acme");
        assert_eq!(find_code(&parsed.text).as_deref(), Some("482913"));
        assert!(parse_message(raw, sent + 3600).is_none());
    }
}
