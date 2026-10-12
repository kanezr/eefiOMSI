//! The interface texts in the player's language: the game's own tables
//! (`crates/omsi-app/locales/app.yml`, the English text is the key), read once and handed to
//! the page whole for its language.

use std::collections::HashMap;
use std::sync::OnceLock;

const TABLES: &str = include_str!("../../omsi-app/locales/app.yml");

/// language → (English → translation)
type Tables = HashMap<String, HashMap<String, String>>;

fn tables() -> &'static Tables {
    static T: OnceLock<Tables> = OnceLock::new();
    T.get_or_init(|| parse(TABLES))
}

/// A double-quoted YAML scalar's text (the file uses nothing else).
fn unquote(s: &str) -> Option<String> {
    let s = s.trim();
    let inner = s.strip_prefix('"')?.strip_suffix('"')?;
    let mut out = String::with_capacity(inner.len());
    let mut chars = inner.chars();
    while let Some(c) = chars.next() {
        if c != '\\' {
            out.push(c);
            continue;
        }
        match chars.next() {
            Some('n') => out.push('\n'),
            Some('t') => out.push('\t'),
            Some('"') => out.push('"'),
            Some('\\') => out.push('\\'),
            Some('/') => out.push('/'),
            Some('u') => {
                let hex: String = chars.by_ref().take(4).collect();
                if let Some(ch) = u32::from_str_radix(&hex, 16).ok().and_then(char::from_u32) {
                    out.push(ch);
                }
            }
            Some(o) => {
                out.push('\\');
                out.push(o);
            }
            None => out.push('\\'),
        }
    }
    Some(out)
}

fn parse(text: &str) -> Tables {
    let mut out: Tables = HashMap::new();
    let mut key: Option<String> = None;
    for line in text.lines() {
        if line.trim().is_empty() || line.trim_start().starts_with('#') {
            continue;
        }
        if let Some(rest) = line.strip_prefix("  ") {
            let (Some(k), Some((lang, value))) = (&key, rest.split_once(": ")) else { continue };
            if let Some(v) = unquote(value) {
                out.entry(lang.trim().to_string()).or_default().insert(k.clone(), v);
            }
        } else if let Some(k) = line.strip_suffix(':') {
            key = unquote(k);
        } else {
            key = None;
        }
    }
    out
}

/// Every text of the tables in `lang` (`ru`, `de` ...; empty for English: nothing).
pub fn table(lang: &str) -> HashMap<String, String> {
    tables().get(lang).cloned().unwrap_or_default()
}

#[cfg(test)]
mod tests {
    #[test]
    fn the_tables_parse() {
        let t = super::parse("_version: 2\n\n\"Current time\":\n  de: \"Aktuelle Uhrzeit\"\n  ru: \"Текущее \\\"время\\\"\"\n");
        assert_eq!(t["de"]["Current time"], "Aktuelle Uhrzeit");
        assert_eq!(t["ru"]["Current time"], "Текущее \"время\"");
        assert!(super::table("ru").contains_key("Current time"));
    }
}
