//! Find a verification code or link in a message. Pure text handling, no I/O.
//!
//! Faithful port of linuxautofill's `detect.py`. Distances are measured in characters
//! (not bytes) to match Python's `len()`.

use std::sync::LazyLock;

use fancy_regex::Regex as FancyRegex;
use regex::Regex;

pub const DEFAULT_KEYWORDS: &[&str] = &[
    "codes?",
    "otp",
    "passcode",
    "password",
    "pin",
    "verification",
    "verify",
    "one[- ]time",
    "2fa",
    "mfa",
    "two[- ]factor",
    "two[- ]step",
    "2[- ]step",
    "authentication",
    "authenticate",
    "log[- ]?in",
    "sign[- ]?in",
    "confirm",
    "confirmation",
    "token",
];

/// A candidate further than this from a keyword is unrelated to it.
pub const MAX_GAP: usize = 60;
/// Added to the gap when a sentence ends between the keyword and the candidate.
pub const SENTENCE_PENALTY: usize = 40;
/// Two candidates this close to the same distance are too close to call.
pub const AMBIGUOUS_MARGIN: usize = 10;
/// How much text (in characters) before a link counts as describing it.
pub const LINK_CONTEXT: usize = 120;

fn re(pattern: &str) -> Regex {
    Regex::new(pattern).expect("valid regex")
}

static VALID_CODE: LazyLock<Regex> = LazyLock::new(|| re(r"^[A-Za-z0-9-]{4,12}$"));

/// Text that looks like a code but isn't. Blanked out before searching.
static NOISE: LazyLock<Regex> = LazyLock::new(|| {
    const MONTH: &str = r"(?:jan|feb|mar|apr|may|jun|jul|aug|sep|oct|nov|dec)[a-z]*\.?";
    let parts = [
        r"\bhttps?://\S+".to_string(),
        r"\bwww\.\S+".into(),
        r"\b\S+\.(?:com|net|org|io|co|me|ly)/\S*".into(),
        r"\S+@\S+\.\S+".into(),
        // Money.
        r"[$€£¥]\s?\d[\d,.]*".into(),
        r"\b\d[\d,.]*\s?(?:usd|eur|gbp|dollars?|euros?|pounds?)\b".into(),
        // Dates and times.
        r"\b\d{4}-\d{1,2}-\d{1,2}\b".into(),
        r"\b\d{1,2}[/.]\d{1,2}[/.]\d{2,4}\b".into(),
        format!(r"\b{MONTH}\s+\d{{1,2}}(?:st|nd|rd|th)?,?\s+\d{{4}}\b"),
        format!(r"\b\d{{1,2}}(?:st|nd|rd|th)?\s+{MONTH}\s+\d{{4}}\b"),
        r"\b\d{1,2}:\d{2}(?::\d{2})?\b".into(),
        // Durations, such as "10MIN".
        r"\b\d+\s*(?:minutes?|mins?|seconds?|secs?|hours?|hrs?|days?)\b".into(),
        // Phone numbers and short-code opt-outs.
        r"\+\d[\d ().-]{6,}\d".into(),
        r"\(?\b\d{3}\)?[ .-]\d{3}[ .-]\d{4}\b".into(),
        r"\b\d{3}-\d{4}\b".into(),
        r"\b(?:stop|help|reply|text)\s+\S+\s+to\s+\d{3,6}\b".into(),
        // Card and account tails.
        r"\b(?:ending(?:\s+in)?|last\s+4(?:\s+digits)?(?:\s+of)?)\s+\d{4}\b".into(),
        // Street addresses in email footers.
        concat!(
            r"\b\d{1,6}\s+(?:[A-Z][a-z]+\s+){1,3}",
            r"(?:Street|St|Avenue|Ave|Road|Rd|Boulevard|Blvd|Drive|Dr|Lane|Ln|Way|Pkwy|Parkway)\b"
        )
        .into(),
        // Labelled numbers.
        concat!(
            r"\b(?:order|invoice|tracking|account|acct|reference|ref|booking|ticket|case|",
            r"customer|member|policy|claim|transaction|txn|receipt|shipment|flight|gate|room)",
            r"\s*(?:number|no\.?|num|id|#)?(?:\s*(?:is|was))?[\s#:]*[A-Za-z0-9-]*\d[A-Za-z0-9-]*"
        )
        .into(),
        // Kinds of "code" that aren't verification codes.
        r"\b(?:zip|postal|area|country|error|status|product|sort|source|qr|bar|dress)\s+codes?\b"
            .into(),
        r"\bpostcodes?\b".into(),
        r"\bbarcodes?\b".into(),
    ];
    re(&format!("(?i){}", parts.join("|")))
});

static PROMO: LazyLock<Regex> =
    LazyLock::new(|| re(r"(?i)\d+%\s*off\b|\bpromo|\bcoupon|\bdiscount|\bvoucher|\bsale\b"));

static CANDIDATE: LazyLock<FancyRegex> = LazyLock::new(|| {
    FancyRegex::new(concat!(
        r"(?<![\w.,/:#$-])(?<!\d[ -])",
        r"(?:",
        r"(?P<prefixed>[A-Z]{1,4}-\d{4,8})",
        r"|(?P<split>\d{3}[ -]\d{3}|\d{4}[ -]\d{4})",
        r"|(?P<digits>\d{4,8})",
        r"|(?P<mixed>(?=[A-Za-z0-9]*[A-Za-z])(?=[A-Za-z0-9]*\d)[A-Za-z0-9]{4,10})",
        r")",
        r"(?![\w%-]|[.,]\d|/\d|[ ]\d)",
    ))
    .expect("valid regex")
});

/// "code is", "OTP:", and the like. What follows is a code, whatever it looks like.
static EXPLICIT: LazyLock<Regex> = LazyLock::new(|| {
    re(concat!(
        r"\b(?i:codes?|otp|passcode|password|pin|token)\b",
        r"(?:\s+(?i:is|are)\s*:?|\s*[:=])",
        r#"[\s"'“‘*`(\[<]*"#,
    ))
});
static LETTERS: LazyLock<Regex> = LazyLock::new(|| re(r"^[A-Z]{4,10}\b"));
const NOT_LETTER_CODES: &[&str] = &[
    "ABOUT", "BELOW", "CANNOT", "CORRECT", "EXPIRED", "INCORRECT", "INVALID", "ONLY", "REQUIRED",
    "SENT", "VALID", "WRONG",
];

static YEAR: LazyLock<Regex> = LazyLock::new(|| re(r"^(?:19|20)\d\d$"));
static SENTENCE_END: LazyLock<Regex> = LazyLock::new(|| re(r"[.!?](?:\s|$)"));
static NEWLINE_PAD: LazyLock<Regex> = LazyLock::new(|| re(r"\s*\n\s*"));
static INLINE_SPACE: LazyLock<Regex> = LazyLock::new(|| re(r"[^\S\n]+"));
static SPLIT_SEP: LazyLock<Regex> = LazyLock::new(|| re(r"[ -]"));
static DEFAULT_KEYWORD_RE: LazyLock<Regex> = LazyLock::new(|| keyword_regex(DEFAULT_KEYWORDS));

const URL_BODY: &str = r#"https?://[^\s<>"'\])]+"#;
static URL: LazyLock<Regex> = LazyLock::new(|| re(&format!("(?i){URL_BODY}")));
static URL_AT_START: LazyLock<Regex> = LazyLock::new(|| re(&format!("(?i)^{URL_BODY}")));
static LINK_ACTION: LazyLock<Regex> = LazyLock::new(|| {
    re(r"(?i)\b(?:verify|verification|confirm|confirmation|activate|activation|validate)\b")
});
static URL_ACTION: LazyLock<Regex> = LazyLock::new(|| re(r"(?i)verif|confirm|activat|validat"));
static LINK_NOISE: LazyLock<Regex> = LazyLock::new(|| {
    re(concat!(
        r"(?i)unsubscribe|opt[- ]out|preferences|privacy|terms|policy|help|support|faq|contact|report|",
        r"settings|not you|wasn.t me|didn.t|cancel|deny|reject|resend|learn more|manage",
    ))
});
static GENERIC_LABEL: LazyLock<Regex> = LazyLock::new(|| {
    re(r"(?i)^(?:(?:click |tap )?(?:here|this link|the link|link|open)\.?)$")
});

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Link {
    pub url: String,
    pub label: String,
    pub context: String,
}

impl Link {
    pub fn new(url: impl Into<String>, label: impl Into<String>, context: impl Into<String>) -> Self {
        Self { url: url.into(), label: label.into(), context: context.into() }
    }
}

/// Byte offset of the char `n` characters before byte offset `end` (clamped to 0).
fn chars_back(text: &str, end: usize, n: usize) -> usize {
    if n == 0 {
        return end;
    }
    text[..end].char_indices().rev().nth(n - 1).map_or(0, |(i, _)| i)
}

/// Return the bare URLs in text, with the text leading up to each.
pub fn plain_links(text: &str) -> Vec<Link> {
    URL.find_iter(text)
        .map(|m| {
            let url = m.as_str().trim_end_matches(['.', ',', ';', ':', '!', '?']);
            let start = chars_back(text, m.start(), LINK_CONTEXT);
            Link::new(url, "", &text[start..m.start()])
        })
        .collect()
}

/// The parts of `urllib.parse.urlsplit` that `find_link` uses.
struct UrlParts {
    scheme: String,
    hostname: Option<String>,
    path: String,
}

fn urlsplit(url: &str) -> UrlParts {
    let url: String = url
        .trim_matches(|c: char| c <= ' ')
        .chars()
        .filter(|c| !matches!(c, '\t' | '\r' | '\n'))
        .collect();
    let mut rest = url.as_str();
    let mut scheme = String::new();
    if let Some(i) = rest.find(':') {
        let candidate = &rest[..i];
        if candidate.chars().next().is_some_and(|c| c.is_ascii_alphabetic())
            && candidate.chars().all(|c| c.is_ascii_alphanumeric() || "+-.".contains(c))
        {
            scheme = candidate.to_ascii_lowercase();
            rest = &rest[i + 1..];
        }
    }
    let mut hostname = None;
    if let Some(after) = rest.strip_prefix("//") {
        let end = after.find(['/', '?', '#']).unwrap_or(after.len());
        let netloc = &after[..end];
        rest = &after[end..];
        let hostport = netloc.rsplit_once('@').map_or(netloc, |(_, h)| h);
        let host = if let Some(bracketed) = hostport.strip_prefix('[') {
            bracketed.split_once(']').map_or("", |(h, _)| h)
        } else {
            hostport.split_once(':').map_or(hostport, |(h, _)| h)
        };
        if !host.is_empty() {
            hostname = Some(host.to_lowercase());
        }
    }
    let path_end = rest.find(['?', '#']).unwrap_or(rest.len());
    UrlParts { scheme, hostname, path: rest[..path_end].to_string() }
}

fn is_labelled_action(link: &Link) -> bool {
    LINK_ACTION.is_match(&link.label) && !LINK_NOISE.is_match(&link.label)
}

fn is_described_action(link: &Link, host: &str, path: &str, text: &str) -> bool {
    if !link.label.is_empty()
        && !URL_AT_START.is_match(&link.label)
        && !GENERIC_LABEL.is_match(&link.label)
    {
        return false;
    }
    let location = format!("{host}{path}");
    if LINK_NOISE.is_match(&location) {
        return false;
    }
    if URL_ACTION.is_match(&location) && LINK_ACTION.is_match(text) {
        return true;
    }
    LINK_ACTION.is_match(&link.context) && !LINK_NOISE.is_match(&link.context)
}

/// Return the link that verifies an account, or `None` if there isn't a clear one.
///
/// A button labelled like "Verify My Email" wins. Otherwise the first link whose address
/// or preceding words say it verifies something is used. Only web links can be returned.
pub fn find_link<'a>(links: impl IntoIterator<Item = &'a Link>, text: &str) -> Option<String> {
    let mut described = None;
    for link in links {
        let parts = urlsplit(&link.url);
        let Some(host) = parts.hostname.as_deref() else { continue };
        if parts.scheme != "http" && parts.scheme != "https" {
            continue;
        }
        if is_labelled_action(link) {
            return Some(link.url.clone());
        }
        if described.is_none() && is_described_action(link, host, &parts.path, text) {
            described = Some(link.url.clone());
        }
    }
    described
}

fn normalise(caps: &fancy_regex::Captures) -> String {
    let whole = caps.get(0).unwrap().as_str();
    if caps.name("split").is_some() {
        SPLIT_SEP.replace_all(whole, "").into_owned()
    } else {
        whole.to_string()
    }
}

/// Like Python's `pattern.match(text, pos)`: anchored at `pos`, lookbehind sees earlier text.
fn candidate_at<'t>(text: &'t str, pos: usize) -> Option<fancy_regex::Captures<'t>> {
    CANDIDATE
        .captures_from_pos(text, pos)
        .ok()
        .flatten()
        .filter(|c| c.get(0).unwrap().start() == pos)
}

fn explicit_code(text: &str) -> Option<String> {
    for phrase in EXPLICIT.find_iter(text) {
        if let Some(caps) = candidate_at(text, phrase.end()) {
            return Some(normalise(&caps));
        }
        if let Some(m) = LETTERS.find(&text[phrase.end()..])
            && !NOT_LETTER_CODES.contains(&m.as_str())
        {
            return Some(m.as_str().to_string());
        }
    }
    None
}

/// Check a candidate that has no explicit phrase vouching for it.
fn is_plausible(caps: &fancy_regex::Captures) -> bool {
    if let Some(d) = caps.name("digits") {
        return !YEAR.is_match(d.as_str());
    }
    if let Some(m) = caps.name("mixed") {
        // Lowercase mixed words are usually names, like "sha256" or "iphone15".
        return m.as_str() == m.as_str().to_uppercase();
    }
    true
}

fn gap(text: &str, start: usize, end: usize, keyword: &regex::Match) -> usize {
    let between = if keyword.end() <= start {
        &text[keyword.end()..start]
    } else {
        &text[end.min(keyword.start())..keyword.start()]
    };
    let penalty = if SENTENCE_END.is_match(between) { SENTENCE_PENALTY } else { 0 };
    between.chars().count() + penalty
}

fn nearest_code(text: &str, keywords: &[regex::Match]) -> Option<String> {
    // Insertion-ordered, like a Python dict, so ties sort the same way.
    let mut scored: Vec<(String, usize)> = Vec::new();
    for caps in CANDIDATE.captures_iter(text).filter_map(Result::ok) {
        if !is_plausible(&caps) {
            continue;
        }
        let m = caps.get(0).unwrap();
        let g = keywords.iter().map(|k| gap(text, m.start(), m.end(), k)).min()?;
        let code = normalise(&caps);
        match scored.iter_mut().find(|(c, _)| *c == code) {
            Some((_, best)) => *best = (*best).min(g),
            None => scored.push((code, g)),
        }
    }
    scored.sort_by_key(|&(_, g)| g);
    let (best, best_gap) = scored.first()?;
    if *best_gap > MAX_GAP {
        return None;
    }
    if scored.len() > 1 && scored[1].1 - best_gap < AMBIGUOUS_MARGIN {
        return None;
    }
    Some(best.clone())
}

/// Build the keyword regex from regex fragments (escape user-supplied words first).
pub fn keyword_regex(keywords: &[impl AsRef<str>]) -> Regex {
    let alts: Vec<&str> = keywords.iter().map(AsRef::as_ref).collect();
    re(&format!(r"(?i)\b(?:{})\b", alts.join("|")))
}

/// Return the verification code in text using the default keywords.
pub fn find_code(text: &str) -> Option<String> {
    find_code_with(text, &DEFAULT_KEYWORD_RE)
}

/// Return the verification code in text, or `None` if there isn't a clear one.
///
/// A false positive overwrites the clipboard, so ambiguous text returns `None`.
pub fn find_code_with(text: &str, keywords: &Regex) -> Option<String> {
    // Email layouts pad text with whitespace, which shouldn't count as distance.
    let collapsed = NEWLINE_PAD.replace_all(text, "\n");
    let blanked = NOISE.replace_all(&collapsed, |c: &regex::Captures| {
        " ".repeat(c[0].chars().count())
    });
    let cleaned = INLINE_SPACE.replace_all(&blanked, " ");
    let found: Vec<_> = keywords.find_iter(&cleaned).collect();
    if found.is_empty() {
        return None;
    }

    let code = explicit_code(&cleaned).or_else(|| nearest_code(&cleaned, &found))?;
    if !VALID_CODE.is_match(&code) {
        return None;
    }
    if PROMO.is_match(text) && !code.bytes().all(|b| b.is_ascii_digit()) {
        return None;
    }
    Some(code)
}
