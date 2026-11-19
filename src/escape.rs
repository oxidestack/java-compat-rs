//! `org.apache.commons.text.StringEscapeUtils`.

use std::fmt::Write;

/// `StringEscapeUtils.escapeJava`: `"` and `\` are backslash-escaped, `\b \n \t \f \r` use their short forms, and
/// every other UTF-16 code unit outside `0x20..=0x7f` becomes `\uXXXX` (upper-case hex, surrogate pairs escaped
/// one unit at a time). `/` is left alone, unlike `escapeEcmaScript`.
pub fn escape_java(s: &str) -> String {
    let mut out = String::with_capacity(s.len() + 8);
    for c in s.chars() {
        match c {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\u{8}' => out.push_str("\\b"),
            '\n' => out.push_str("\\n"),
            '\t' => out.push_str("\\t"),
            '\u{c}' => out.push_str("\\f"),
            '\r' => out.push_str("\\r"),
            ' '..='\u{7f}' => out.push(c),
            _ => {
                let mut units = [0u16; 2];
                for unit in c.encode_utf16(&mut units) {
                    let _ = write!(out, "\\u{unit:04X}");
                }
            }
        }
    }
    out
}

fn unicode_escape(c: char, out: &mut String) {
    let mut units = [0u16; 2];
    for unit in c.encode_utf16(&mut units) {
        let _ = write!(out, "\\u{unit:04X}");
    }
}

/// `StringEscapeUtils.escapeEcmaScript`: like `escapeJava`, plus `'` and `/`.
pub fn escape_ecma_script(s: &str) -> String {
    let mut out = String::with_capacity(s.len() + 8);
    for c in s.chars() {
        match c {
            '\'' => out.push_str("\\'"),
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '/' => out.push_str("\\/"),
            '\u{8}' => out.push_str("\\b"),
            '\n' => out.push_str("\\n"),
            '\t' => out.push_str("\\t"),
            '\u{c}' => out.push_str("\\f"),
            '\r' => out.push_str("\\r"),
            ' '..='\u{7f}' => out.push(c),
            _ => unicode_escape(c, &mut out),
        }
    }
    out
}

/// `StringEscapeUtils.escapeXml10`: the five XML entities, characters XML 1.0 forbids dropped, `0x7f..=0x84` and
/// `0x86..=0x9f` as decimal character references.
pub fn escape_xml10(s: &str) -> String {
    let mut out = String::with_capacity(s.len() + 8);
    for c in s.chars() {
        match c {
            '"' => out.push_str("&quot;"),
            '&' => out.push_str("&amp;"),
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            '\'' => out.push_str("&apos;"),
            '\u{0}'..='\u{8}' | '\u{b}' | '\u{c}' | '\u{e}'..='\u{1f}' | '\u{fffe}' | '\u{ffff}' => {}
            '\u{7f}'..='\u{84}' | '\u{86}'..='\u{9f}' => {
                let _ = write!(out, "&#{};", c as u32);
            }
            _ => out.push(c),
        }
    }
    out
}

/// `URLEncoder.encode(s, "UTF-8")`.
pub fn url_encode(s: &str) -> String {
    let mut out = String::with_capacity(s.len() + 8);
    for c in s.chars() {
        match c {
            'a'..='z' | 'A'..='Z' | '0'..='9' | '.' | '-' | '*' | '_' => out.push(c),
            ' ' => out.push('+'),
            _ => {
                let mut buf = [0u8; 4];
                for b in c.encode_utf8(&mut buf).bytes() {
                    let _ = write!(out, "%{b:02X}");
                }
            }
        }
    }
    out
}

/// `URLDecoder.decode(s, "UTF-8")`: `Err` with the `IllegalArgumentException` message.
pub fn url_decode(s: &str) -> Result<String, String> {
    let bytes = s.as_bytes();
    let mut out = String::with_capacity(s.len());
    let mut i = 0;
    while i < bytes.len() {
        match bytes[i] {
            b'+' => {
                out.push(' ');
                i += 1;
            }
            b'%' => {
                let mut decoded = Vec::new();
                while i < bytes.len() && bytes[i] == b'%' {
                    if i + 3 > bytes.len() {
                        return Err("URLDecoder: Incomplete trailing escape (%) pattern".into());
                    }
                    let hex = &bytes[i + 1..i + 3];
                    let digit = |b: u8| (b as char).to_digit(16);
                    match (digit(hex[0]), digit(hex[1])) {
                        (Some(h), Some(l)) => decoded.push((h * 16 + l) as u8),
                        (h, _) => {
                            let bad = if h.is_none() { hex[0] } else { hex[1] };
                            return Err(format!("URLDecoder: Illegal hex characters in escape (%) pattern - not a hexadecimal digit: \"{}\" = {bad}", bad as char));
                        }
                    }
                    i += 3;
                }
                out.push_str(&String::from_utf8_lossy(&decoded));
            }
            _ => {
                let c = s[i..].chars().next().expect("a char");
                out.push(c);
                i += c.len_utf8();
            }
        }
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn lang3_escapers() {
        assert_eq!(escape_ecma_script("He didn't say, \"stop!\" / é"), "He didn\\'t say, \\\"stop!\\\" \\/ \\u00E9");
        assert_eq!(escape_xml10("<a b='c'>&\u{1}\u{80}</a>"), "&lt;a b=&apos;c&apos;&gt;&amp;&#128;&lt;/a&gt;");
        assert_eq!(url_encode("a b&c/é~"), "a+b%26c%2F%C3%A9%7E");
        assert_eq!(url_decode("a+b%26c%2F%C3%A9").unwrap(), "a b&c/é");
        assert_eq!(url_decode("%zz").unwrap_err(), "URLDecoder: Illegal hex characters in escape (%) pattern - not a hexadecimal digit: \"z\" = 122");
        assert_eq!(url_decode("%4").unwrap_err(), "URLDecoder: Incomplete trailing escape (%) pattern");
    }

    #[test]
    fn matches_commons_text() {
        // StringEscapeUtilsTest.testEscapeJava inputs.
        assert_eq!(escape_java(""), "");
        assert_eq!(escape_java("foo"), "foo");
        assert_eq!(escape_java("\t"), "\\t");
        assert_eq!(escape_java("\\"), "\\\\");
        assert_eq!(escape_java("'"), "'");
        assert_eq!(escape_java("\\\u{8}\t\r"), "\\\\\\b\\t\\r");
        assert_eq!(escape_java("\u{1234}"), "\\u1234");
        assert_eq!(escape_java("\u{0234}"), "\\u0234");
        assert_eq!(escape_java("\u{00ef}"), "\\u00EF");
        assert_eq!(escape_java("\u{0001}"), "\\u0001");
        assert_eq!(escape_java("He didn't say, \"stop!\""), "He didn't say, \\\"stop!\\\"");
        assert_eq!(escape_java("This space is non-breaking:\u{a0}"), "This space is non-breaking:\\u00A0");
        assert_eq!(escape_java("\u{ABCD}\u{1234}\u{012C}"), "\\uABCD\\u1234\\u012C");
        assert_eq!(escape_java("/"), "/");
        assert_eq!(escape_java("\u{7f}"), "\u{7f}");
        assert_eq!(escape_java("\u{1F600}"), "\\uD83D\\uDE00");
    }
}
