import Foundation

/// The launcher speaks the language of the game's settings (`language=` in settings.cfg), not
/// the system's: the string catalog's language is chosen for the app as it starts (a change
/// in the settings is heard from the next start; the game's own texts change at once).
enum AppLanguage {
    /// The settings' codes → Apple's language identifiers.
    static let codes: [String: String] = [
        "ENG": "en", "DEU": "de", "FRA": "fr", "RUS": "ru", "UKR": "uk", "BEL": "be", "KAZ": "kk",
        "POL": "pl", "CZE": "cs", "HUN": "hu", "ESP": "es", "CAT": "ca", "PTB": "pt-BR", "PTP": "pt-PT",
        "ITA": "it", "NLD": "nl", "SWE": "sv", "TUR": "tr", "JPN": "ja", "ZHT": "zh-Hant", "KOR": "ko", "THA": "th",
        "VIE": "vi", "IND": "id", "MSA": "ms", "TGL": "fil", "CHS": "zh-Hans", "HIN": "hi",
    ]

    /// `home` is the folder the game keeps `.openomsi` in (the app's data folder on an iPhone).
    static func apply(home: URL) {
        let file = home.appendingPathComponent(".openomsi/settings.cfg")
        guard let text = try? String(contentsOf: file, encoding: .utf8) else { return }
        let code = text.split(whereSeparator: \.isNewline)
            .first { $0.hasPrefix("language=") }
            .map { String($0.dropFirst("language=".count)).trimmingCharacters(in: .whitespaces) }
        guard let code, let lang = codes[code.uppercased()] else { return }
        UserDefaults.standard.set([lang], forKey: "AppleLanguages")
    }
}
