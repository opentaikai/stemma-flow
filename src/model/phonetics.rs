//! Phonetic surname codes: American Soundex, Daitch–Mokotoff and Cologne.

use std::collections::BTreeSet;

/// Codes produced by the supported phonetic methods.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct PhoneticCodes {
    pub soundex: String,
    pub daitch_mokotoff: Vec<String>,
    pub cologne: String,
}

/// Computes all supported codes for a name.
pub fn codes(name: &str) -> PhoneticCodes {
    PhoneticCodes {
        soundex: american_soundex(name),
        daitch_mokotoff: daitch_mokotoff(name),
        cologne: cologne(name),
    }
}

/// Folds common accented Latin letters to plain uppercase A-Z letters.
pub fn normalize_letters(value: &str) -> String {
    let mut result = String::with_capacity(value.len());
    for character in value.to_uppercase().chars() {
        match character {
            'A'..='Z' => result.push(character),
            'À' | 'Á' | 'Â' | 'Ã' | 'Ä' | 'Å' | 'Ą' => result.push('A'),
            'Ç' | 'Ć' | 'Č' => result.push('C'),
            'È' | 'É' | 'Ê' | 'Ë' | 'Ę' | 'Ě' => result.push('E'),
            'Ì' | 'Í' | 'Î' | 'Ï' | 'İ' | 'ı' => result.push('I'),
            'Ł' => result.push('L'),
            'Ñ' | 'Ń' | 'Ň' => result.push('N'),
            'Ò' | 'Ó' | 'Ô' | 'Õ' | 'Ö' | 'Ø' | 'Ő' => result.push('O'),
            'Ř' | 'Ŕ' => result.push('R'),
            'Š' | 'Ś' | 'Ş' => result.push('S'),
            'Ť' | 'Ţ' => result.push('T'),
            'Ù' | 'Ú' | 'Û' | 'Ü' | 'Ů' | 'Ű' => result.push('U'),
            'Ý' | 'Ÿ' => result.push('Y'),
            'Ź' | 'Ž' | 'Ż' => result.push('Z'),
            'Æ' => result.push_str("AE"),
            'Œ' => result.push_str("OE"),
            'ß' => result.push_str("SS"),
            'Ð' | 'Đ' => result.push('D'),
            'Þ' => result.push_str("TH"),
            _ => {}
        }
    }
    result
}

/// American Soundex (NARA), four characters.
pub fn american_soundex(value: &str) -> String {
    let letters: Vec<char> = normalize_letters(value).chars().collect();
    let Some(first) = letters.first().copied() else {
        return String::new();
    };
    let mut result = String::from(first);
    let mut last = soundex_digit(first);
    for letter in letters.into_iter().skip(1) {
        match letter {
            'H' | 'W' => continue,
            'A' | 'E' | 'I' | 'O' | 'U' | 'Y' => {
                last = None;
            }
            _ => {
                let Some(digit) = soundex_digit(letter) else {
                    continue;
                };
                if last != Some(digit) {
                    result.push(digit);
                    last = Some(digit);
                }
            }
        }
        if result.len() == 4 {
            break;
        }
    }
    while result.len() < 4 {
        result.push('0');
    }
    result
}

fn soundex_digit(letter: char) -> Option<char> {
    Some(match letter {
        'B' | 'F' | 'P' | 'V' => '1',
        'C' | 'G' | 'J' | 'K' | 'Q' | 'S' | 'X' | 'Z' => '2',
        'D' | 'T' => '3',
        'L' => '4',
        'M' | 'N' => '5',
        'R' => '6',
        _ => return None,
    })
}

struct DmRule {
    pattern: &'static str,
    start: &'static [&'static str],
    before_vowel: &'static [&'static str],
    other: &'static [&'static str],
    keep_double: bool,
}

const B: &[&str] = &["7"];
const D: &[&str] = &["3"];
const F: &[&str] = &["7"];
const G: &[&str] = &["5"];
const K: &[&str] = &["5"];
const L: &[&str] = &["8"];
const M: &[&str] = &["6"];
const N: &[&str] = &["6"];
const P: &[&str] = &["7"];
const R: &[&str] = &["9"];
const S: &[&str] = &["4"];
const T: &[&str] = &["3"];
const V: &[&str] = &["7"];
const Z: &[&str] = &["4"];

const DM_RULES: &[DmRule] = &[
    DmRule {
        pattern: "SCHTSCH",
        start: &["2"],
        before_vowel: &["4"],
        other: &["4"],
        keep_double: false,
    },
    DmRule {
        pattern: "SCHTSH",
        start: &["2"],
        before_vowel: &["4"],
        other: &["4"],
        keep_double: false,
    },
    DmRule {
        pattern: "SCHTCH",
        start: &["2"],
        before_vowel: &["4"],
        other: &["4"],
        keep_double: false,
    },
    DmRule {
        pattern: "SHTCH",
        start: &["2"],
        before_vowel: &["4"],
        other: &["4"],
        keep_double: false,
    },
    DmRule {
        pattern: "SHTSH",
        start: &["2"],
        before_vowel: &["4"],
        other: &["4"],
        keep_double: false,
    },
    DmRule {
        pattern: "STSCH",
        start: &["2"],
        before_vowel: &["4"],
        other: &["4"],
        keep_double: false,
    },
    DmRule {
        pattern: "ZDZH",
        start: &["2"],
        before_vowel: &["4"],
        other: &["4"],
        keep_double: false,
    },
    DmRule {
        pattern: "ZHDZH",
        start: &["2"],
        before_vowel: &["4"],
        other: &["4"],
        keep_double: false,
    },
    DmRule {
        pattern: "SCHTSCH",
        start: &["2"],
        before_vowel: &["4"],
        other: &["4"],
        keep_double: false,
    },
    DmRule {
        pattern: "ZDZ",
        start: &["2"],
        before_vowel: &["4"],
        other: &["4"],
        keep_double: false,
    },
    DmRule {
        pattern: "SZCZ",
        start: &["2"],
        before_vowel: &["4"],
        other: &["4"],
        keep_double: false,
    },
    DmRule {
        pattern: "SZCS",
        start: &["2"],
        before_vowel: &["4"],
        other: &["4"],
        keep_double: false,
    },
    DmRule {
        pattern: "STRZ",
        start: &["2"],
        before_vowel: &["4"],
        other: &["4"],
        keep_double: false,
    },
    DmRule {
        pattern: "STRS",
        start: &["2"],
        before_vowel: &["4"],
        other: &["4"],
        keep_double: false,
    },
    DmRule {
        pattern: "STSH",
        start: &["2"],
        before_vowel: &["4"],
        other: &["4"],
        keep_double: false,
    },
    DmRule {
        pattern: "SCHT",
        start: &["2"],
        before_vowel: &["43"],
        other: &["43"],
        keep_double: false,
    },
    DmRule {
        pattern: "SCHD",
        start: &["2"],
        before_vowel: &["43"],
        other: &["43"],
        keep_double: false,
    },
    DmRule {
        pattern: "SHT",
        start: &["2"],
        before_vowel: &["43"],
        other: &["43"],
        keep_double: false,
    },
    DmRule {
        pattern: "SHD",
        start: &["2"],
        before_vowel: &["43"],
        other: &["43"],
        keep_double: false,
    },
    DmRule {
        pattern: "SZD",
        start: &["2"],
        before_vowel: &["43"],
        other: &["43"],
        keep_double: false,
    },
    DmRule {
        pattern: "SD",
        start: &["2"],
        before_vowel: &["43"],
        other: &["43"],
        keep_double: false,
    },
    DmRule {
        pattern: "ZD",
        start: &["2"],
        before_vowel: &["43"],
        other: &["43"],
        keep_double: false,
    },
    DmRule {
        pattern: "ZHD",
        start: &["2"],
        before_vowel: &["43"],
        other: &["43"],
        keep_double: false,
    },
    DmRule {
        pattern: "ST",
        start: &["2"],
        before_vowel: &["43"],
        other: &["43"],
        keep_double: false,
    },
    DmRule {
        pattern: "SZT",
        start: &["2"],
        before_vowel: &["43"],
        other: &["43"],
        keep_double: false,
    },
    DmRule {
        pattern: "SCHTSCH",
        start: &["2"],
        before_vowel: &["4"],
        other: &["4"],
        keep_double: false,
    },
    DmRule {
        pattern: "CHS",
        start: &["5"],
        before_vowel: &["54"],
        other: &["54"],
        keep_double: false,
    },
    DmRule {
        pattern: "SCH",
        start: &["4"],
        before_vowel: &["4"],
        other: &["4"],
        keep_double: false,
    },
    DmRule {
        pattern: "SHT",
        start: &["2"],
        before_vowel: &["43"],
        other: &["43"],
        keep_double: false,
    },
    DmRule {
        pattern: "STCH",
        start: &["2"],
        before_vowel: &["4"],
        other: &["4"],
        keep_double: false,
    },
    DmRule {
        pattern: "SC",
        start: &["2"],
        before_vowel: &["4"],
        other: &["4"],
        keep_double: false,
    },
    DmRule {
        pattern: "SH",
        start: &["4"],
        before_vowel: &["4"],
        other: &["4"],
        keep_double: false,
    },
    DmRule {
        pattern: "SZ",
        start: &["4"],
        before_vowel: &["4"],
        other: &["4"],
        keep_double: false,
    },
    DmRule {
        pattern: "TCH",
        start: &["4"],
        before_vowel: &["4"],
        other: &["4"],
        keep_double: false,
    },
    DmRule {
        pattern: "TTCH",
        start: &["4"],
        before_vowel: &["4"],
        other: &["4"],
        keep_double: false,
    },
    DmRule {
        pattern: "TTSCH",
        start: &["4"],
        before_vowel: &["4"],
        other: &["4"],
        keep_double: false,
    },
    DmRule {
        pattern: "THS",
        start: &["4"],
        before_vowel: &["4"],
        other: &["4"],
        keep_double: false,
    },
    DmRule {
        pattern: "TH",
        start: &["3"],
        before_vowel: &["3"],
        other: &["3"],
        keep_double: false,
    },
    DmRule {
        pattern: "TRZ",
        start: &["4"],
        before_vowel: &["4"],
        other: &["4"],
        keep_double: false,
    },
    DmRule {
        pattern: "TRS",
        start: &["4"],
        before_vowel: &["4"],
        other: &["4"],
        keep_double: false,
    },
    DmRule {
        pattern: "TSCH",
        start: &["4"],
        before_vowel: &["4"],
        other: &["4"],
        keep_double: false,
    },
    DmRule {
        pattern: "TSH",
        start: &["4"],
        before_vowel: &["4"],
        other: &["4"],
        keep_double: false,
    },
    DmRule {
        pattern: "TTS",
        start: &["4"],
        before_vowel: &["4"],
        other: &["4"],
        keep_double: false,
    },
    DmRule {
        pattern: "TTSZ",
        start: &["4"],
        before_vowel: &["4"],
        other: &["4"],
        keep_double: false,
    },
    DmRule {
        pattern: "TC",
        start: &["4"],
        before_vowel: &["4"],
        other: &["4"],
        keep_double: false,
    },
    DmRule {
        pattern: "TZ",
        start: &["4"],
        before_vowel: &["4"],
        other: &["4"],
        keep_double: false,
    },
    DmRule {
        pattern: "TTZ",
        start: &["4"],
        before_vowel: &["4"],
        other: &["4"],
        keep_double: false,
    },
    DmRule {
        pattern: "TZS",
        start: &["4"],
        before_vowel: &["4"],
        other: &["4"],
        keep_double: false,
    },
    DmRule {
        pattern: "TSZ",
        start: &["4"],
        before_vowel: &["4"],
        other: &["4"],
        keep_double: false,
    },
    DmRule {
        pattern: "TS",
        start: &["4"],
        before_vowel: &["4"],
        other: &["4"],
        keep_double: false,
    },
    DmRule {
        pattern: "ZHDZH",
        start: &["2"],
        before_vowel: &["4"],
        other: &["4"],
        keep_double: false,
    },
    DmRule {
        pattern: "ZDZH",
        start: &["2"],
        before_vowel: &["4"],
        other: &["4"],
        keep_double: false,
    },
    DmRule {
        pattern: "ZDZ",
        start: &["2"],
        before_vowel: &["4"],
        other: &["4"],
        keep_double: false,
    },
    DmRule {
        pattern: "ZHD",
        start: &["2"],
        before_vowel: &["43"],
        other: &["43"],
        keep_double: false,
    },
    DmRule {
        pattern: "ZD",
        start: &["2"],
        before_vowel: &["43"],
        other: &["43"],
        keep_double: false,
    },
    DmRule {
        pattern: "ZH",
        start: &["4"],
        before_vowel: &["4"],
        other: &["4"],
        keep_double: false,
    },
    DmRule {
        pattern: "ZS",
        start: &["4"],
        before_vowel: &["4"],
        other: &["4"],
        keep_double: false,
    },
    DmRule {
        pattern: "ZSCH",
        start: &["4"],
        before_vowel: &["4"],
        other: &["4"],
        keep_double: false,
    },
    DmRule {
        pattern: "ZSH",
        start: &["4"],
        before_vowel: &["4"],
        other: &["4"],
        keep_double: false,
    },
    DmRule {
        pattern: "DRZ",
        start: &["4"],
        before_vowel: &["4"],
        other: &["4"],
        keep_double: false,
    },
    DmRule {
        pattern: "DRS",
        start: &["4"],
        before_vowel: &["4"],
        other: &["4"],
        keep_double: false,
    },
    DmRule {
        pattern: "DS",
        start: &["4"],
        before_vowel: &["4"],
        other: &["4"],
        keep_double: false,
    },
    DmRule {
        pattern: "DSH",
        start: &["4"],
        before_vowel: &["4"],
        other: &["4"],
        keep_double: false,
    },
    DmRule {
        pattern: "DSZ",
        start: &["4"],
        before_vowel: &["4"],
        other: &["4"],
        keep_double: false,
    },
    DmRule {
        pattern: "DZ",
        start: &["4"],
        before_vowel: &["4"],
        other: &["4"],
        keep_double: false,
    },
    DmRule {
        pattern: "DZH",
        start: &["4"],
        before_vowel: &["4"],
        other: &["4"],
        keep_double: false,
    },
    DmRule {
        pattern: "DZS",
        start: &["4"],
        before_vowel: &["4"],
        other: &["4"],
        keep_double: false,
    },
    DmRule {
        pattern: "DT",
        start: &["3"],
        before_vowel: &["3"],
        other: &["3"],
        keep_double: false,
    },
    DmRule {
        pattern: "CH",
        start: &["5", "4"],
        before_vowel: &["5", "4"],
        other: &["5", "4"],
        keep_double: false,
    },
    DmRule {
        pattern: "CK",
        start: &["5", "45"],
        before_vowel: &["5", "45"],
        other: &["5", "45"],
        keep_double: false,
    },
    DmRule {
        pattern: "C",
        start: &["5", "4"],
        before_vowel: &["5", "4"],
        other: &["5", "4"],
        keep_double: false,
    },
    DmRule {
        pattern: "J",
        start: &["1", "4"],
        before_vowel: &["1", "4"],
        other: &["1", "4"],
        keep_double: false,
    },
    DmRule {
        pattern: "RZ",
        start: &["94", "4"],
        before_vowel: &["94", "4"],
        other: &["94", "4"],
        keep_double: false,
    },
    DmRule {
        pattern: "RS",
        start: &["94", "4"],
        before_vowel: &["94", "4"],
        other: &["94", "4"],
        keep_double: false,
    },
    DmRule {
        pattern: "AI",
        start: &["0", "1"],
        before_vowel: &["1"],
        other: &[],
        keep_double: false,
    },
    DmRule {
        pattern: "AJ",
        start: &["0", "1"],
        before_vowel: &["1"],
        other: &[],
        keep_double: false,
    },
    DmRule {
        pattern: "AY",
        start: &["0", "1"],
        before_vowel: &["1"],
        other: &[],
        keep_double: false,
    },
    DmRule {
        pattern: "AU",
        start: &["0"],
        before_vowel: &["7"],
        other: &[],
        keep_double: false,
    },
    DmRule {
        pattern: "EI",
        start: &["0", "1"],
        before_vowel: &["1"],
        other: &[],
        keep_double: false,
    },
    DmRule {
        pattern: "EJ",
        start: &["0", "1"],
        before_vowel: &["1"],
        other: &[],
        keep_double: false,
    },
    DmRule {
        pattern: "EY",
        start: &["0", "1"],
        before_vowel: &["1"],
        other: &[],
        keep_double: false,
    },
    DmRule {
        pattern: "EU",
        start: &["1"],
        before_vowel: &["1"],
        other: &[],
        keep_double: false,
    },
    DmRule {
        pattern: "IA",
        start: &["1"],
        before_vowel: &[],
        other: &[],
        keep_double: false,
    },
    DmRule {
        pattern: "IE",
        start: &["1"],
        before_vowel: &[],
        other: &[],
        keep_double: false,
    },
    DmRule {
        pattern: "IO",
        start: &["1"],
        before_vowel: &[],
        other: &[],
        keep_double: false,
    },
    DmRule {
        pattern: "IU",
        start: &["1"],
        before_vowel: &[],
        other: &[],
        keep_double: false,
    },
    DmRule {
        pattern: "OI",
        start: &["0", "1"],
        before_vowel: &["1"],
        other: &[],
        keep_double: false,
    },
    DmRule {
        pattern: "OJ",
        start: &["0", "1"],
        before_vowel: &["1"],
        other: &[],
        keep_double: false,
    },
    DmRule {
        pattern: "OY",
        start: &["0", "1"],
        before_vowel: &["1"],
        other: &[],
        keep_double: false,
    },
    DmRule {
        pattern: "UI",
        start: &["0", "1"],
        before_vowel: &["1"],
        other: &[],
        keep_double: false,
    },
    DmRule {
        pattern: "UJ",
        start: &["0", "1"],
        before_vowel: &["1"],
        other: &[],
        keep_double: false,
    },
    DmRule {
        pattern: "UY",
        start: &["0", "1"],
        before_vowel: &["1"],
        other: &[],
        keep_double: false,
    },
    DmRule {
        pattern: "UE",
        start: &["0"],
        before_vowel: &[],
        other: &[],
        keep_double: false,
    },
    DmRule {
        pattern: "MN",
        start: &["66"],
        before_vowel: &["66"],
        other: &["66"],
        keep_double: true,
    },
    DmRule {
        pattern: "NM",
        start: &["66"],
        before_vowel: &["66"],
        other: &["66"],
        keep_double: true,
    },
    DmRule {
        pattern: "X",
        start: &["5"],
        before_vowel: &["54"],
        other: &["54"],
        keep_double: false,
    },
    DmRule {
        pattern: "H",
        start: &["5"],
        before_vowel: &["5"],
        other: &[],
        keep_double: false,
    },
    DmRule {
        pattern: "A",
        start: &["0"],
        before_vowel: &[],
        other: &[],
        keep_double: false,
    },
    DmRule {
        pattern: "E",
        start: &["0"],
        before_vowel: &[],
        other: &[],
        keep_double: false,
    },
    DmRule {
        pattern: "I",
        start: &["0"],
        before_vowel: &[],
        other: &[],
        keep_double: false,
    },
    DmRule {
        pattern: "O",
        start: &["0"],
        before_vowel: &[],
        other: &[],
        keep_double: false,
    },
    DmRule {
        pattern: "U",
        start: &["0"],
        before_vowel: &[],
        other: &[],
        keep_double: false,
    },
    DmRule {
        pattern: "Y",
        start: &["1"],
        before_vowel: &[],
        other: &[],
        keep_double: false,
    },
    DmRule {
        pattern: "B",
        start: B,
        before_vowel: B,
        other: B,
        keep_double: false,
    },
    DmRule {
        pattern: "D",
        start: D,
        before_vowel: D,
        other: D,
        keep_double: false,
    },
    DmRule {
        pattern: "F",
        start: F,
        before_vowel: F,
        other: F,
        keep_double: false,
    },
    DmRule {
        pattern: "FB",
        start: F,
        before_vowel: F,
        other: F,
        keep_double: false,
    },
    DmRule {
        pattern: "PF",
        start: F,
        before_vowel: F,
        other: F,
        keep_double: false,
    },
    DmRule {
        pattern: "PH",
        start: F,
        before_vowel: F,
        other: F,
        keep_double: false,
    },
    DmRule {
        pattern: "G",
        start: G,
        before_vowel: G,
        other: G,
        keep_double: false,
    },
    DmRule {
        pattern: "K",
        start: K,
        before_vowel: K,
        other: K,
        keep_double: false,
    },
    DmRule {
        pattern: "KH",
        start: K,
        before_vowel: K,
        other: K,
        keep_double: false,
    },
    DmRule {
        pattern: "KS",
        start: &["5"],
        before_vowel: &["54"],
        other: &["54"],
        keep_double: false,
    },
    DmRule {
        pattern: "Q",
        start: G,
        before_vowel: G,
        other: G,
        keep_double: false,
    },
    DmRule {
        pattern: "L",
        start: L,
        before_vowel: L,
        other: L,
        keep_double: false,
    },
    DmRule {
        pattern: "M",
        start: M,
        before_vowel: M,
        other: M,
        keep_double: false,
    },
    DmRule {
        pattern: "N",
        start: N,
        before_vowel: N,
        other: N,
        keep_double: false,
    },
    DmRule {
        pattern: "P",
        start: P,
        before_vowel: P,
        other: P,
        keep_double: false,
    },
    DmRule {
        pattern: "R",
        start: R,
        before_vowel: R,
        other: R,
        keep_double: false,
    },
    DmRule {
        pattern: "S",
        start: S,
        before_vowel: S,
        other: S,
        keep_double: false,
    },
    DmRule {
        pattern: "T",
        start: T,
        before_vowel: T,
        other: T,
        keep_double: false,
    },
    DmRule {
        pattern: "V",
        start: V,
        before_vowel: V,
        other: V,
        keep_double: false,
    },
    DmRule {
        pattern: "W",
        start: V,
        before_vowel: V,
        other: V,
        keep_double: false,
    },
    DmRule {
        pattern: "Z",
        start: Z,
        before_vowel: Z,
        other: Z,
        keep_double: false,
    },
];

/// Daitch–Mokotoff Soundex, returning every distinct six-digit variant.
pub fn daitch_mokotoff(value: &str) -> Vec<String> {
    let letters: Vec<char> = normalize_letters(value).chars().collect();
    if letters.is_empty() {
        return Vec::new();
    }
    let mut variants = BTreeSet::from([String::new()]);
    let mut index = 0;
    while index < letters.len() {
        let rule = best_rule(&letters, index);
        let Some(rule) = rule else {
            index += 1;
            continue;
        };
        let end = index + rule.pattern.chars().count();
        let start = index == 0;
        let before_vowel = letters
            .get(end)
            .is_some_and(|letter| matches!(letter, 'A' | 'E' | 'I' | 'O' | 'U' | 'Y'));
        let codes = if start {
            rule.start
        } else if before_vowel {
            rule.before_vowel
        } else {
            rule.other
        };
        let mut next = BTreeSet::new();
        for prefix in &variants {
            if codes.is_empty() {
                next.insert(prefix.clone());
                continue;
            }
            for code in codes {
                next.insert(append_code(prefix, code, rule.keep_double));
            }
        }
        variants = next;
        index = end;
    }
    variants
        .into_iter()
        .filter(|code| !code.is_empty())
        .map(|code| {
            let mut code = code.chars().take(6).collect::<String>();
            while code.len() < 6 {
                code.push('0');
            }
            code
        })
        .collect()
}

fn best_rule(letters: &[char], index: usize) -> Option<&'static DmRule> {
    DM_RULES
        .iter()
        .filter(|rule| matches_at(letters, index, rule.pattern))
        .max_by_key(|rule| rule.pattern.len())
}

fn matches_at(letters: &[char], index: usize, pattern: &str) -> bool {
    for (offset, expected) in pattern.chars().enumerate() {
        if letters.get(index + offset) != Some(&expected) {
            return false;
        }
    }
    true
}

fn append_code(prefix: &str, code: &str, keep_double: bool) -> String {
    if code.is_empty() {
        return prefix.to_string();
    }
    let mut result = prefix.to_string();
    let mut code = code;
    if !keep_double
        && !result.is_empty()
        && let Some(last) = result.chars().last()
        && code.starts_with(last)
    {
        code = &code[last.len_utf8()..];
    }
    result.push_str(code);
    result
}

/// Cologne Phonetic (Kölner Phonetik).
pub fn cologne(value: &str) -> String {
    let letters: Vec<char> = normalize_letters(value).chars().collect();
    if letters.is_empty() {
        return String::new();
    }
    let mut encoded = String::new();
    for (index, letter) in letters.iter().copied().enumerate() {
        let previous = index.checked_sub(1).and_then(|i| letters.get(i)).copied();
        let next = letters.get(index + 1).copied();
        if let Some(code) = cologne_code(letter, previous, next, index == 0) {
            encoded.push_str(code);
        }
    }
    let mut collapsed = String::new();
    for digit in encoded.chars() {
        if !collapsed.ends_with(digit) {
            collapsed.push(digit);
        }
    }
    let mut result = String::new();
    for (index, digit) in collapsed.chars().enumerate() {
        if digit != '0' || index == 0 {
            result.push(digit);
        }
    }
    result
}

fn cologne_code(
    letter: char,
    previous: Option<char>,
    next: Option<char>,
    initial: bool,
) -> Option<&'static str> {
    Some(match letter {
        'A' | 'E' | 'I' | 'J' | 'O' | 'U' | 'Y' => "0",
        'H' => return None,
        'B' => "1",
        'P' if next == Some('H') => "3",
        'P' => "1",
        'D' | 'T' if matches!(next, Some('C' | 'S' | 'Z')) => return None,
        'D' | 'T' => "2",
        'F' | 'V' | 'W' => "3",
        'G' | 'K' | 'Q' => "4",
        'C' if matches!(previous, Some('S' | 'Z')) => "8",
        'C' if initial => {
            if matches!(
                next,
                Some('A' | 'H' | 'K' | 'L' | 'O' | 'Q' | 'R' | 'U' | 'X')
            ) {
                "4"
            } else {
                "8"
            }
        }
        'C' if matches!(next, Some('A' | 'H' | 'K' | 'O' | 'Q' | 'U' | 'X')) => "4",
        'C' => "8",
        'X' if matches!(previous, Some('C' | 'K' | 'Q')) => return None,
        'X' => "48",
        'L' => "5",
        'M' | 'N' => "6",
        'R' => "7",
        'S' | 'Z' => "8",
        _ => return None,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn american_soundex_matches_standard_examples() {
        assert_eq!(american_soundex("Robert"), "R163");
        assert_eq!(american_soundex("Rupert"), "R163");
        assert_eq!(american_soundex("Ashcraft"), "A261");
        assert_eq!(american_soundex("Tymczak"), "T522");
        assert_eq!(american_soundex("Pfister"), "P236");
        assert_eq!(american_soundex("Honeyman"), "H555");
    }

    #[test]
    fn daitch_mokotoff_matches_published_examples() {
        assert_eq!(daitch_mokotoff("Moskowitz"), ["645740"]);
        assert_eq!(daitch_mokotoff("Moskovitz"), ["645740"]);
        assert!(daitch_mokotoff("Auerbach").contains(&"097500".to_string()));
        assert!(daitch_mokotoff("Auerbach").contains(&"097400".to_string()));
        assert!(daitch_mokotoff("Jackson").contains(&"154600".to_string()));
        assert!(daitch_mokotoff("Jackson").contains(&"145460".to_string()));
        assert!(daitch_mokotoff("Peters").contains(&"739400".to_string()));
        assert!(daitch_mokotoff("Peterson").contains(&"739460".to_string()));
    }

    #[test]
    fn cologne_matches_published_examples() {
        assert_eq!(cologne("Müller-Lüdenscheidt"), "65752682");
        assert_eq!(cologne("Wikipedia"), "3412");
        assert_eq!(cologne("Breschnew"), "17863");
        assert_eq!(cologne("Meier"), "67");
        assert_eq!(cologne("Maier"), "67");
        assert_eq!(cologne("Mayer"), "67");
        assert_eq!(cologne("Mayr"), "67");
    }

    #[test]
    fn folds_accents_and_ignores_non_letters() {
        assert_eq!(normalize_letters("Müller-Łódź"), "MULLERLODZ");
        assert!(american_soundex("").is_empty());
        assert!(daitch_mokotoff("").is_empty());
        assert!(cologne("").is_empty());
    }
}
