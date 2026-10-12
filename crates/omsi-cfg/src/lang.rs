//! The interface's languages and the spellings of them a settings file may use.

/// The interface's languages: the settings' code (OMSI's three-letter style), the name in
/// the language itself, the interface tables' code, and other spellings a file may use.
/// OMSI's own texts (key names, `.dsc` descriptions, tutorials) exist in English, German
/// and French: every other language shows those in English.
pub const LANGUAGES: &[(&str, &str, &str, &[&str])] = &[
    ("ENG", "English", "en", &["en", "english"]),
    ("DEU", "Deutsch", "de", &["de", "ger", "german", "deutsch"]),
    ("FRA", "Français", "fr", &["fr", "fre", "french", "francais", "français"]),
    ("RUS", "Русский", "ru", &["ru", "russian", "русский"]),
    ("UKR", "Українська", "uk", &["uk", "ua", "ukrainian", "українська"]),
    ("BEL", "Беларуская", "be", &["be", "by", "belarusian", "беларуская"]),
    ("KAZ", "Қазақша", "kk", &["kk", "kz", "kazakh", "қазақша"]),
    ("POL", "Polski", "pl", &["pl", "polish", "polski"]),
    ("CZE", "Čeština", "cs", &["cs", "cz", "czech", "čeština", "ces"]),
    ("HUN", "Magyar", "hu", &["hu", "hungarian", "magyar"]),
    ("ESP", "Español", "es", &["es", "spa", "spanish", "español"]),
    ("CAT", "Català", "ca", &["ca", "cat", "ca-es", "ca-ad", "catalan", "català", "catala"]),
    ("PTB", "Português (Brasil)", "pt", &["pt", "br", "pt-br", "por", "portuguese", "português"]),
    ("PTP", "Português (Portugal)", "pt-pt", &["pt-pt", "pt_pt", "pt-portugal", "portuguese-portugal", "português (portugal)", "português de portugal"]),
    ("ITA", "Italiano", "it", &["it", "italian", "italiano"]),
    ("NLD", "Nederlands", "nl", &["nl", "dutch", "nederlands"]),
    ("TUR", "Türkçe", "tr", &["tr", "turkish", "türkçe"]),
    ("JPN", "日本語", "ja", &["ja", "jp", "japanese", "日本語"]),
    ("ZHT", "繁體中文", "zh-tw", &["zh-tw", "zh-hant", "zh-hk", "zh-mo", "cht", "traditional chinese", "繁體中文", "繁体中文"]),
    ("KOR", "한국어", "ko", &["ko", "kr", "korean", "한국어"]),
    ("THA", "ไทย", "th", &["th", "thai", "ไทย"]),
    ("VIE", "Tiếng Việt", "vi", &["vi", "vietnamese", "tiếng việt"]),
    ("IND", "Bahasa Indonesia", "id", &["id", "indonesian", "bahasa indonesia"]),
    ("MSA", "Bahasa Melayu", "ms", &["ms", "malay", "bahasa melayu"]),
    ("TGL", "Filipino", "tl", &["tl", "fil", "filipino", "tagalog"]),
    ("CHS", "中文 (简体)", "zh", &["zh", "zh-cn", "zh-sg", "zh-hans", "zhs", "chs", "cn", "chinese", "simplified chinese", "简体中文", "簡體中文", "中文"]),
    ("HIN", "हिन्दी", "hi", &["hi", "hindi", "हिन्दी"]),
];

/// The settings' language code from any spelling the game accepts (English when unknown).
pub fn language_code(s: &str) -> &'static str {
    let s = s.trim().to_lowercase();
    LANGUAGES
        .iter()
        .find(|(code, _, _, aliases)| code.eq_ignore_ascii_case(&s) || aliases.iter().any(|a| *a == s))
        .map(|l| l.0)
        .unwrap_or("ENG")
}

/// The interface tables' code of a language (`ru`, `ja` ...; empty for English).
pub fn language_iso(code: &str) -> &'static str {
    let c = language_code(code);
    LANGUAGES.iter().find(|l| l.0 == c).map(|l| if l.2 == "en" { "" } else { l.2 }).unwrap_or("")
}
