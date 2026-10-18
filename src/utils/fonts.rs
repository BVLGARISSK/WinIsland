use winisland_core::config::AppConfig;
use winisland_render::text::{FontManager, FontSelection};

pub(crate) const RECOMMENDED_FONT_URL: &str = "https://github.com/ACT-02/PingFangUI-VF";

const REGION_SUFFIXES: [&str; 7] = ["SC", "TC", "HK", "MO", "JA", "JP", "KR"];

struct Region {
    suffixes: &'static [&'static str],
    locale: &'static str,
}

const SIMPLIFIED: Region = Region {
    suffixes: &["SC"],
    locale: "zh-CN",
};

fn current_region() -> Region {
    let language = winisland_core::i18n::current_lang();
    let locale = if language.starts_with("zh") {
        language
    } else {
        crate::platform::shell().locale()
    };
    let mut subtags = locale.split(['-', '_']);
    let primary = subtags.next().unwrap_or_default().to_ascii_lowercase();
    let subtags: Vec<&str> = subtags.collect();
    let has = |tag: &str| {
        subtags
            .iter()
            .any(|subtag| subtag.eq_ignore_ascii_case(tag))
    };
    match primary.as_str() {
        "ja" => Region {
            suffixes: &["JA", "JP"],
            locale: "ja-JP",
        },
        "ko" => Region {
            suffixes: &["KR"],
            locale: "ko-KR",
        },
        "zh" if has("HK") => Region {
            suffixes: &["HK", "TC"],
            locale: "zh-HK",
        },
        "zh" if has("MO") => Region {
            suffixes: &["MO", "HK", "TC"],
            locale: "zh-MO",
        },
        "zh" if has("TW") || has("Hant") => Region {
            suffixes: &["TC"],
            locale: "zh-TW",
        },
        _ => SIMPLIFIED,
    }
}

fn split_region(family: &str) -> Option<&str> {
    let (base, suffix) = family.rsplit_once(' ')?;
    REGION_SUFFIXES.contains(&suffix).then_some(base)
}

/// Lists installed families, folding regional variants such as `PingFang UI SC` and
/// `PingFang UI TC` into one `PingFang UI` entry that follows the interface language.
pub(crate) fn family_choices(families: &[String]) -> Vec<String> {
    let mut choices: Vec<String> = Vec::new();
    for family in families {
        let choice = split_region(family)
            .filter(|base| {
                !families.iter().any(|other| other == base)
                    && families
                        .iter()
                        .filter(|other| split_region(other) == Some(base))
                        .count()
                        > 1
            })
            .unwrap_or(family);
        if !choices.iter().any(|existing| existing == choice) {
            choices.push(choice.to_string());
        }
    }
    choices
}

/// Resolves a folded family to the regional variant matching the interface language.
pub(crate) fn resolve_family(family: &str) -> String {
    let fonts = FontManager::global();
    if fonts.has_family(family) {
        return family.to_string();
    }
    current_region()
        .suffixes
        .iter()
        .chain(SIMPLIFIED.suffixes)
        .chain(&REGION_SUFFIXES)
        .map(|suffix| format!("{family} {suffix}"))
        .find(|variant| fonts.has_family(variant))
        .unwrap_or_else(|| family.to_string())
}

pub(crate) fn apply(config: &AppConfig) {
    let weight_offset = match config.font_weight.as_str() {
        "light" => -100,
        "bold" => 100,
        _ => 0,
    };
    let family = config.font_family.as_deref().map(resolve_family);
    FontManager::global().configure(FontSelection {
        family: family.as_deref(),
        file: config.custom_font_path.as_deref(),
        weight_offset,
        locale: current_region().locale,
    });
}
