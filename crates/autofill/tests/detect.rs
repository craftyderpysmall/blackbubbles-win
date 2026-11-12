//! Ported verbatim from linuxautofill `tests/test_detect.py`.

use bb_autofill::detect::{Link, find_code, find_code_with, find_link, keyword_regex, plain_links};

fn adp_email() -> String {
    let pad = format!("{}\n", " ".repeat(30)).repeat(10);
    format!(
        "Verification code\n{pad}   496660   \n{pad}\
         This code expires in 15 minutes.\n\
         Enter this code to access ADP services.\n\
         The email is sent from an automated system so please do not reply. \
         fdd6782f-8b9d-4d30-a77a-14b2e5f24933\n\
         ADP and the ADP logo are trademarks of ADP, Inc.' ©2026 ADP, Inc."
    )
}

#[test]
fn finds_codes() {
    let adp = adp_email();
    let matches: &[(&str, &str)] = &[
        ("Your verification code is 482913", "482913"),
        ("G-482913 is your Google verification code.", "G-482913"),
        ("482913 is your Instagram code. Don't share it.", "482913"),
        ("Your OTP: 8841", "8841"),
        ("OTP is 482913. Valid for 10 minutes.", "482913"),
        ("Use 482913 to verify your account", "482913"),
        ("Your Uber code: 4821", "4821"),
        ("Your one-time passcode is 123 456", "123456"),
        ("Your login code is 123-456", "123456"),
        ("Your PIN is 7391", "7391"),
        ("2FA token: A1B2C3", "A1B2C3"),
        ("Your sign-in code is a1b2c3", "a1b2c3"),
        ("Your confirmation code is ABCDEF", "ABCDEF"),
        ("Your code is **482913**", "482913"),
        ("Your code is \"482913\"", "482913"),
        ("Your verification code:\n\n    482913\n\nIt expires soon.", "482913"),
        ("Enter this code to sign in\n\n482913", "482913"),
        ("Verify your account. Use 482913", "482913"),
        ("Your code is 482913. Your order 99123 shipped.", "482913"),
        ("Your code for order 99123 is 482913", "482913"),
        ("Code 482913 expires at 10:30 on 12/25/2026", "482913"),
        ("Your Acme code is 482913. Reply STOP to 55512 to opt out.", "482913"),
        ("Your code is 4829, valid for 1000 seconds", "4829"),
        ("Your code is 2026", "2026"),
        ("Your authentication code is 482913. Visit https://acme.com/a/778899", "482913"),
        ("Your code is 482913. Save 20% off today!", "482913"),
        ("TEILEN SIE IHREN CODE NICHT: 482913", "482913"),
        (&adp, "496660"),
    ];
    let failures: Vec<_> = matches
        .iter()
        .filter(|(text, want)| find_code(text).as_deref() != Some(*want))
        .map(|(text, want)| format!("{text:?}: want {want}, got {:?}", find_code(text)))
        .collect();
    assert!(failures.is_empty(), "{failures:#?}");
}

#[test]
fn ignores_everything_else() {
    let long_pad = format!("Verify your email.\n{}\n\nYour balance is now over 1234 points and we think you will enjoy it.", " ".repeat(200));
    let no_match: &[&str] = &[
        "",
        "Hey, are we still on for 7pm?",
        "Call me at 555-1234 about the code review",
        "The meeting is in room 4821",
        "Your order 482913 has shipped",
        "Order #482913 confirmed. Total $1234.56",
        "Your order is confirmed! Thanks for buying 1000 widgets",
        "Your package will arrive on 2026-12-25",
        "Your zip code is 90210",
        "Area code 415 and 5551",
        "Error code 5001 occurred",
        "Use code SAVE20 for 20% off",
        "Promo code WINTER26 ends tonight",
        "Your code is expired",
        "Your code is invalid",
        "Your code is VALID",
        "We changed the password in 2026",
        "Your card ending in 4242 was used to sign in",
        "Log in by Jan 12, 2026",
        "Confirm your subscription\n\nAcme Inc, 1600 Main Street",
        "Verify your email. Your balance is now over 1234 points and we think you will enjoy it.",
        "Log in to see iphone15 deals",
        "Your code is on https://acme.com/verify/482913",
        "Your code is 1234 5678 9012 3456",
        "Valid login from 1234 and 5678",
        &long_pad,
    ];
    let failures: Vec<_> = no_match
        .iter()
        .filter_map(|text| find_code(text).map(|got| format!("{text:?}: got {got}")))
        .collect();
    assert!(failures.is_empty(), "{failures:#?}");
}

#[test]
fn custom_keywords() {
    assert_eq!(find_code_with("Tu clave es 482913", &keyword_regex(&["clave"])).as_deref(), Some("482913"));
    assert_eq!(find_code("Tu clave es 482913"), None);
}

const REDDIT_TEXT: &str = "Your email address was added to your Reddit account.
To verify your email address and secure your account, click the button below.
This link is only valid for 3 days and it can only be used once.";

fn reddit_links() -> Vec<Link> {
    vec![
        Link::new("https://click.redditmail.com/a1", "Account Settings", "Expired link? No worries, visit your "),
        Link::new("https://click.redditmail.com/b2", "Verify My Email", "click the button below."),
        Link::new("https://click.redditmail.com/c3", "Account Security FAQs", "check out our "),
        Link::new("https://click.redditmail.com/d4", "contact us", " or "),
        Link::new("https://click.redditmail.com/e5", "let us know", "isn't your Reddit account, "),
    ]
}

const WORKDAY_URL: &str = "https://dicks.wd1.myworkdayjobs.com/dsg/activate/i8ya1hxv0/?redirect=%2Fen-US%2Fdsg%2Fjob%2FRetail-Golf-Club-Tech_202641847%2Fapply";

fn workday_text() -> String {
    format!(
        "Verify your candidate account\n\
         Click this link to confirm your email address and complete setup for your candidate account\n\
         {WORKDAY_URL}\nThe link will expire after 24 hours.\n\n\
         This email box is not monitored. Please do not reply to this message."
    )
}

#[test]
fn button_is_picked_over_other_links() {
    assert_eq!(find_link(&reddit_links(), REDDIT_TEXT).as_deref(), Some("https://click.redditmail.com/b2"));
}

#[test]
fn button_order_does_not_matter() {
    let base = reddit_links();
    let mut links = vec![base[1].clone()];
    links.extend(base);
    assert_eq!(
        find_link(links.iter().rev(), REDDIT_TEXT).as_deref(),
        Some("https://click.redditmail.com/b2")
    );
}

#[test]
fn bare_url_with_verifying_path() {
    let text = workday_text();
    assert_eq!(find_link(&plain_links(&text), &text).as_deref(), Some(WORKDAY_URL));
}

#[test]
fn link_whose_label_is_its_own_url() {
    let links = [Link::new(WORKDAY_URL, WORKDAY_URL, "")];
    assert_eq!(find_link(&links, &workday_text()).as_deref(), Some(WORKDAY_URL));
}

#[test]
fn click_here_after_verifying_words() {
    let links = [Link::new("https://t.example/q9", "click here", "To verify your email, please ")];
    assert_eq!(find_link(&links, "To verify your email").as_deref(), Some("https://t.example/q9"));
}

#[test]
fn tracking_url_after_verifying_words() {
    let text = "Open this to confirm your email:\nhttps://t.example/q9";
    assert_eq!(find_link(&plain_links(text), text).as_deref(), Some("https://t.example/q9"));
}

#[test]
fn ordinary_emails_have_no_link() {
    let cases: Vec<(&str, Vec<Link>)> = vec![
        ("Your order shipped", vec![Link::new("https://shop.example/track/1", "Track package", "")]),
        ("Big sale today", vec![Link::new("https://shop.example/sale", "Shop now", "")]),
        ("News", plain_links("Read more at https://news.example/story")),
        ("Confirm your preferences", vec![Link::new("https://news.example/unsubscribe", "Unsubscribe", "")]),
        ("Please verify", vec![Link::new("https://acme.example/help", "Help center", "")]),
        ("Was this you?", vec![Link::new("https://acme.example/x", "This wasn't me, cancel confirmation", "")]),
        ("Please verify", plain_links("See https://acme.example/settings/verify")),
    ];
    for (text, links) in &cases {
        assert_eq!(find_link(links, text), None, "{text}");
    }
}

#[test]
fn only_web_links_are_returned() {
    for url in ["javascript:alert(1)", "file:///etc/passwd", "mailto:a@b.example", "ssh://h/x"] {
        assert_eq!(find_link(&[Link::new(url, "Verify My Email", "")], "verify"), None, "{url}");
    }
}

#[test]
fn plain_links_context_counts_characters() {
    let text = format!("{}é https://x.example/a.", "é".repeat(200));
    let links = plain_links(&text);
    assert_eq!(links[0].url, "https://x.example/a");
    assert_eq!(links[0].context.chars().count(), 120);
}
