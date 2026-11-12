//! Pull the plain text out of a Messages `attributedBody` (an NSArchiver "typedstream").
//!
//! Since macOS Ventura many messages have `text = NULL` and only this blob. The string
//! follows the `NSString` class name: a few type bytes, a `+` marker, a length, then UTF-8.
//! Length encoding: one byte < 0x80, or 0x81 + u16 LE, or 0x82 + u32 LE.

pub fn extract_text(blob: &[u8]) -> Option<String> {
    let marker = b"NSString";
    let start = blob.windows(marker.len()).position(|w| w == marker)? + marker.len();
    // Skip to the '+' that introduces the string bytes (normally 5 bytes later).
    let plus = blob[start..].iter().take(16).position(|&b| b == b'+')? + start + 1;
    let (len, text_start) = match *blob.get(plus)? {
        0x81 => (u16::from_le_bytes([*blob.get(plus + 1)?, *blob.get(plus + 2)?]) as usize, plus + 3),
        0x82 => (
            u32::from_le_bytes([*blob.get(plus + 1)?, *blob.get(plus + 2)?, *blob.get(plus + 3)?, *blob.get(plus + 4)?])
                as usize,
            plus + 5,
        ),
        n if n < 0x80 => (n as usize, plus + 1),
        _ => return None,
    };
    let bytes = blob.get(text_start..text_start.checked_add(len)?)?;
    Some(String::from_utf8_lossy(bytes).into_owned())
}

/// Clean up message text: drop attachment placeholders (U+FFFC) and surrounding space.
pub fn clean(text: &str) -> Option<String> {
    let cleaned: String = text.chars().filter(|&c| c != '\u{fffc}').collect();
    let trimmed = cleaned.trim();
    (!trimmed.is_empty()).then(|| trimmed.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn blob(text: &str) -> Vec<u8> {
        let mut out = b"\x04\x0bstreamtyped\x81\xe8\x03\x84\x01@\x84\x84\x84\x12NSAttributedString\x00\x84\x84\x08NSObject\x00\x85\x92\x84\x84\x84\x08NSString\x01\x94\x84\x01+".to_vec();
        let bytes = text.as_bytes();
        if bytes.len() < 0x80 {
            out.push(bytes.len() as u8);
        } else {
            out.push(0x81);
            out.extend_from_slice(&(bytes.len() as u16).to_le_bytes());
        }
        out.extend_from_slice(bytes);
        out.extend_from_slice(b"\x86\x84\x02iI\x01");
        out
    }

    #[test]
    fn extracts_short_and_long_strings() {
        assert_eq!(extract_text(&blob("Your code is 482913")).as_deref(), Some("Your code is 482913"));
        let long = "é".repeat(300);
        assert_eq!(extract_text(&blob(&long)).as_deref(), Some(long.as_str()));
        assert_eq!(extract_text(b"garbage"), None);
    }

    #[test]
    fn cleans_placeholders() {
        assert_eq!(clean("\u{fffc}"), None);
        assert_eq!(clean(" hi \u{fffc}").as_deref(), Some("hi"));
    }
}
