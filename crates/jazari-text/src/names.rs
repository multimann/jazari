//! Face-name helpers: normalisation, style hints and fallback classes.
//!
//! XFL stores the face as written by the authoring tool. Flash CS5 and JPEXS
//! write PostScript names with the style folded in (`ArialMT`,
//! `TimesNewRomanPS-BoldItalicMT`); older documents may hold family names
//! (`Tahoma`, `Traditional Arabic`). These helpers let the library match both.

/// Lower-cases a name and drops spaces, hyphens and underscores, so that
/// `Times New Roman`, `TimesNewRoman` and `times-new-roman` compare equal.
pub(crate) fn normalize(name: &str) -> String {
    name.chars()
        .filter(|c| !matches!(c, ' ' | '-' | '_'))
        .flat_map(char::to_lowercase)
        .collect()
}

/// The style a face name asks for.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub(crate) struct StyleHint {
    /// The name mentions a bold (or heavier) weight.
    pub bold: bool,
    /// The name mentions italic or oblique.
    pub italic: bool,
}

impl StyleHint {
    fn from_style_words(style: &str) -> Self {
        let s = style.to_ascii_lowercase();
        Self {
            bold: s.contains("bold") || s.contains("black") || s.contains("heavy"),
            italic: s.contains("italic") || s.contains("oblique"),
        }
    }

    /// The OS/2 weight class this hint asks for.
    pub fn weight(self) -> u16 {
        if self.bold { 700 } else { 400 }
    }
}

/// Words that name a style rather than a family when they end a face name.
const STYLE_WORDS: &[&str] = &[
    "bold",
    "italic",
    "oblique",
    "regular",
    "normal",
    "bolditalic",
    "boldoblique",
    "semibold",
    "demibold",
    "light",
    "black",
    "heavy",
    "medium",
    "thin",
];

/// Splits a face name into a normalised family stem and a style hint.
///
/// - `TimesNewRomanPS-BoldItalicMT` gives `timesnewroman` and bold italic;
/// - `Arial Bold` gives `arial` and bold;
/// - `ArialMT` gives `arial` and regular.
pub(crate) fn split_style(face: &str) -> (String, StyleHint) {
    let face = face.trim();
    let (base, style) = match face.rsplit_once('-') {
        Some((base, style)) if !base.is_empty() => (base.to_owned(), style.to_owned()),
        _ => {
            let mut words: Vec<&str> = face.split_whitespace().collect();
            let mut style_words = Vec::new();
            while words.len() > 1 {
                let last = words[words.len() - 1].to_ascii_lowercase();
                if STYLE_WORDS.contains(&last.as_str()) {
                    style_words.push(last);
                    words.pop();
                } else {
                    break;
                }
            }
            (words.join(" "), style_words.join(" "))
        }
    };
    (
        strip_vendor_suffix(&normalize(&base)),
        StyleHint::from_style_words(&style),
    )
}

/// Removes the `PS`/`MT`/`PSMT` decorations of Monotype PostScript names.
pub(crate) fn strip_vendor_suffix(normalized: &str) -> String {
    let mut s = normalized;
    for suffix in ["psmt", "mt", "ps"] {
        if let Some(stem) = s.strip_suffix(suffix)
            && !stem.is_empty()
        {
            s = stem;
        }
    }
    s.to_owned()
}

/// The installed families that stand for one of Flash's device font names
/// (`_sans`, `_serif`, `_typewriter`), most faithful first: Flash Player
/// maps them to Arial, Times New Roman and Courier New on Windows. `None`
/// for any other name.
pub(crate) fn device_font_families(face: &str) -> Option<&'static [&'static str]> {
    const SANS: &[&str] = &[
        "Arial",
        "Helvetica",
        "Liberation Sans",
        "Arimo",
        "DejaVu Sans",
    ];
    const SERIF: &[&str] = &[
        "Times New Roman",
        "Times",
        "Liberation Serif",
        "Tinos",
        "DejaVu Serif",
    ];
    const MONO: &[&str] = &[
        "Courier New",
        "Courier",
        "Liberation Mono",
        "Cousine",
        "DejaVu Sans Mono",
    ];
    match face.trim().to_ascii_lowercase().as_str() {
        "_sans" => Some(SANS),
        "_serif" => Some(SERIF),
        "_typewriter" => Some(MONO),
        _ => None,
    }
}

/// Which embedded font stands in for a face that is not installed.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum FallbackClass {
    /// A Naskh (serif-like) Arabic face: Noto Naskh Arabic.
    Naskh,
    /// A sans-serif face named as Arabic: Noto Sans Arabic.
    SansArabic,
    /// Anything else: Noto Sans.
    Latin,
}

/// Names that point to a Naskh-like (serif) design.
const NASKH_MARKERS: &[&str] = &[
    "naskh",
    "traditionalarabic",
    "simplifiedarabic",
    "arabictypesetting",
    "majalla",
    "amiri",
    "scheherazade",
    "lateef",
    "arabtype",
    "adobearabic",
    "mudir",
    "kacstbook",
    "times",
];

/// Names that point to an Arabic design without being Naskh-like.
const ARABIC_MARKERS: &[&str] = &[
    "arab", "kufi", "farsi", "persian", "urdu", "nastaliq", "ruqaa", "thuluth", "diwani",
];

/// Classifies a face name for the fallback.
pub(crate) fn classify(face: &str) -> FallbackClass {
    let n = normalize(face);
    let serif = n.contains("serif") && !n.contains("sans");
    if serif || NASKH_MARKERS.iter().any(|m| n.contains(m)) {
        FallbackClass::Naskh
    } else if ARABIC_MARKERS.iter().any(|m| n.contains(m)) {
        FallbackClass::SansArabic
    } else {
        FallbackClass::Latin
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn splits_postscript_and_family_style_names() {
        assert_eq!(
            split_style("TimesNewRomanPS-BoldItalicMT"),
            (
                "timesnewroman".to_owned(),
                StyleHint {
                    bold: true,
                    italic: true
                }
            )
        );
        assert_eq!(
            split_style("ArialMT"),
            ("arial".to_owned(), StyleHint::default())
        );
        assert_eq!(
            split_style("Arial Bold"),
            (
                "arial".to_owned(),
                StyleHint {
                    bold: true,
                    italic: false
                }
            )
        );
        assert_eq!(
            split_style("Times New Roman"),
            ("timesnewroman".to_owned(), StyleHint::default())
        );
    }

    #[test]
    fn classifies_fallbacks() {
        assert_eq!(classify("Traditional Arabic"), FallbackClass::Naskh);
        assert_eq!(classify("Simplified Arabic"), FallbackClass::Naskh);
        assert_eq!(classify("Arabic Typesetting"), FallbackClass::Naskh);
        assert_eq!(classify("TimesNewRomanPSMT"), FallbackClass::Naskh);
        assert_eq!(classify("Arabic Transparent"), FallbackClass::SansArabic);
        assert_eq!(classify("Tahoma"), FallbackClass::Latin);
        assert_eq!(classify("Microsoft Sans Serif"), FallbackClass::Latin);
    }

    #[test]
    fn maps_flash_device_font_names() {
        assert_eq!(device_font_families("_sans").map(|f| f[0]), Some("Arial"));
        assert_eq!(
            device_font_families(" _SERIF ").map(|f| f[0]),
            Some("Times New Roman")
        );
        assert_eq!(
            device_font_families("_typewriter").map(|f| f[0]),
            Some("Courier New")
        );
        assert_eq!(device_font_families("sans"), None);
    }
}
