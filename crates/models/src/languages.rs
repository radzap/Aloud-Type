//! Languages Whisper takes, and the keyboard-layout locale to Whisper code mapping.

/// Every language Whisper models take for a fixed choice (code, English name),
/// sorted by name. Cantonese (`yue`) is left out: only the large-v3 models have
/// a token for it, so the smaller models would decode garbage.
pub const LANGUAGES: &[(&str, &str)] = &[
    ("af", "Afrikaans"),
    ("sq", "Albanian"),
    ("am", "Amharic"),
    ("ar", "Arabic"),
    ("hy", "Armenian"),
    ("as", "Assamese"),
    ("az", "Azerbaijani"),
    ("ba", "Bashkir"),
    ("eu", "Basque"),
    ("be", "Belarusian"),
    ("bn", "Bengali"),
    ("bs", "Bosnian"),
    ("br", "Breton"),
    ("bg", "Bulgarian"),
    ("my", "Burmese"),
    ("ca", "Catalan"),
    ("zh", "Chinese"),
    ("hr", "Croatian"),
    ("cs", "Czech"),
    ("da", "Danish"),
    ("nl", "Dutch"),
    ("en", "English"),
    ("et", "Estonian"),
    ("fo", "Faroese"),
    ("fi", "Finnish"),
    ("fr", "French"),
    ("gl", "Galician"),
    ("ka", "Georgian"),
    ("de", "German"),
    ("el", "Greek"),
    ("gu", "Gujarati"),
    ("ht", "Haitian Creole"),
    ("ha", "Hausa"),
    ("haw", "Hawaiian"),
    ("he", "Hebrew"),
    ("hi", "Hindi"),
    ("hu", "Hungarian"),
    ("is", "Icelandic"),
    ("id", "Indonesian"),
    ("it", "Italian"),
    ("ja", "Japanese"),
    ("jw", "Javanese"),
    ("kn", "Kannada"),
    ("kk", "Kazakh"),
    ("km", "Khmer"),
    ("ko", "Korean"),
    ("lo", "Lao"),
    ("la", "Latin"),
    ("lv", "Latvian"),
    ("ln", "Lingala"),
    ("lt", "Lithuanian"),
    ("lb", "Luxembourgish"),
    ("mk", "Macedonian"),
    ("mg", "Malagasy"),
    ("ms", "Malay"),
    ("ml", "Malayalam"),
    ("mt", "Maltese"),
    ("mi", "Maori"),
    ("mr", "Marathi"),
    ("mn", "Mongolian"),
    ("ne", "Nepali"),
    ("no", "Norwegian"),
    ("nn", "Norwegian Nynorsk"),
    ("oc", "Occitan"),
    ("ps", "Pashto"),
    ("fa", "Persian"),
    ("pl", "Polish"),
    ("pt", "Portuguese"),
    ("pa", "Punjabi"),
    ("ro", "Romanian"),
    ("ru", "Russian"),
    ("sa", "Sanskrit"),
    ("sr", "Serbian"),
    ("sn", "Shona"),
    ("sd", "Sindhi"),
    ("si", "Sinhala"),
    ("sk", "Slovak"),
    ("sl", "Slovenian"),
    ("so", "Somali"),
    ("es", "Spanish"),
    ("su", "Sundanese"),
    ("sw", "Swahili"),
    ("sv", "Swedish"),
    ("tl", "Tagalog"),
    ("tg", "Tajik"),
    ("ta", "Tamil"),
    ("tt", "Tatar"),
    ("te", "Telugu"),
    ("th", "Thai"),
    ("bo", "Tibetan"),
    ("tr", "Turkish"),
    ("tk", "Turkmen"),
    ("uk", "Ukrainian"),
    ("ur", "Urdu"),
    ("uz", "Uzbek"),
    ("vi", "Vietnamese"),
    ("cy", "Welsh"),
    ("yi", "Yiddish"),
    ("yo", "Yoruba"),
];

/// English name of a Whisper language code.
pub fn language_name(code: &str) -> Option<&'static str> {
    LANGUAGES.iter().find(|l| l.0 == code).map(|l| l.1)
}

/// The Whisper language for a Windows locale name such as "sk-SK", "nb-NO" or
/// "sr-Latn-RS". `None` when Whisper has no such language.
pub fn whisper_language_for_locale(locale: &str) -> Option<&'static str> {
    let primary = locale.split(['-', '_']).next()?.to_ascii_lowercase();
    // Where the ISO 639 code Windows uses differs from Whisper's.
    let code = match primary.as_str() {
        "nb" => "no",
        "fil" => "tl",
        "jv" => "jw",
        "iw" => "he",
        "in" => "id",
        other => other,
    };
    LANGUAGES.iter().find(|l| l.0 == code).map(|l| l.0)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn keyboard_locales_map_to_whisper_codes() {
        assert_eq!(whisper_language_for_locale("sk-SK"), Some("sk"));
        assert_eq!(whisper_language_for_locale("en-US"), Some("en"));
        assert_eq!(whisper_language_for_locale("nb-NO"), Some("no"));
        assert_eq!(whisper_language_for_locale("nn-NO"), Some("nn"));
        assert_eq!(whisper_language_for_locale("sr-Latn-RS"), Some("sr"));
        assert_eq!(whisper_language_for_locale("fil-PH"), Some("tl"));
        assert_eq!(whisper_language_for_locale("haw-US"), Some("haw"));
        assert_eq!(whisper_language_for_locale("chr-Cher-US"), None);
        assert_eq!(whisper_language_for_locale(""), None);
    }

    #[test]
    fn names_come_from_the_list() {
        assert_eq!(language_name("sk"), Some("Slovak"));
        assert_eq!(language_name("xx"), None);
    }
}
