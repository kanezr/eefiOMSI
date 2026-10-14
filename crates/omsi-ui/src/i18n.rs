//! The interface's language. Every text the painter draws or measures goes through [`tr`]:
//! the English text is the key, and the application hands over a lookup (its rust-i18n
//! tables) and the language to look it up in. English, or a text the tables do not have
//! (a bus's name, a number), is drawn as it is.

use std::borrow::Cow;
use std::sync::RwLock;

/// A lookup: (language, English text) → the text in that language.
pub type Lookup = fn(&str, &str) -> Option<String>;

static STATE: RwLock<(Option<Lookup>, String)> = RwLock::new((None, String::new()));

/// What is asked when the tables have no translation (the application's machine
/// translation: a text it has translated already, None while it is still at it).
static FALLBACK: RwLock<Option<Lookup>> = RwLock::new(None);

/// The lookup for texts the tables do not have.
pub fn set_fallback(f: Option<Lookup>) {
    if let Ok(mut s) = FALLBACK.write() {
        *s = f;
    }
}

/// The lookup to translate with.
pub fn set_lookup(f: Lookup) {
    if let Ok(mut s) = STATE.write() {
        s.0 = Some(f);
    }
}

/// The language to show (`ru`, `de`, `fr`; empty or `en` for English).
pub fn set_language(code: &str) {
    if let Ok(mut s) = STATE.write() {
        s.1 = if code.eq_ignore_ascii_case("en") { String::new() } else { code.to_ascii_lowercase() };
    }
}

/// The language shown now (empty for English).
pub fn language() -> String {
    STATE.read().map(|s| s.1.clone()).unwrap_or_default()
}

/// `text` in the interface's language.
pub fn tr(text: &str) -> Cow<'_, str> {
    let Ok(s) = STATE.read() else { return Cow::Borrowed(text) };
    match (&s.0, s.1.is_empty()) {
        (Some(f), false) if !text.is_empty() => match f(&s.1, text) {
            Some(t) => Cow::Owned(t),
            None => match FALLBACK.read().ok().and_then(|g| *g).and_then(|g| g(&s.1, text)) {
                Some(t) => Cow::Owned(t),
                None => Cow::Borrowed(text),
            },
        },
        _ => Cow::Borrowed(text),
    }
}
