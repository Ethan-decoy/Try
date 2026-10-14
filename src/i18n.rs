use serde::{Deserialize, Serialize};
use std::cell::Cell;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum InterfaceLanguage {
    #[default]
    System,
    #[serde(rename = "zh_cn")]
    Chinese,
    #[serde(rename = "en")]
    English,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Language {
    Chinese,
    English,
}

impl InterfaceLanguage {
    pub fn index(self) -> i32 {
        match self {
            Self::System => 0,
            Self::Chinese => 1,
            Self::English => 2,
        }
    }

    pub fn from_index(index: i32) -> Option<Self> {
        match index {
            0 => Some(Self::System),
            1 => Some(Self::Chinese),
            2 => Some(Self::English),
            _ => None,
        }
    }

    pub fn resolve(self, locale: Option<&str>) -> Language {
        match self {
            Self::Chinese => Language::Chinese,
            Self::English => Language::English,
            Self::System => {
                let base = locale
                    .unwrap_or_default()
                    .split(['-', '_', '.', '@'])
                    .next()
                    .unwrap_or_default();
                if base.eq_ignore_ascii_case("zh") {
                    Language::Chinese
                } else {
                    Language::English
                }
            }
        }
    }
}

thread_local! {
    // UI state is confined to Slint's event thread. Core tests and helper
    // threads keep their own language, without a process-wide mutable setting.
    static LANGUAGE: Cell<Language> = const { Cell::new(Language::Chinese) };
}

pub fn activate(preference: InterfaceLanguage) -> Language {
    let locale = (preference == InterfaceLanguage::System)
        .then(sys_locale::get_locale)
        .flatten();
    let language = preference.resolve(locale.as_deref());
    LANGUAGE.set(language);
    language
}

pub fn apply_to_slint() -> Result<(), slint::SelectBundledTranslationError> {
    slint::select_bundled_translation(match LANGUAGE.get() {
        Language::Chinese => "",
        Language::English => "en",
    })
}

pub fn text(source: &'static str) -> &'static str {
    match LANGUAGE.get() {
        Language::Chinese => source,
        Language::English => english(source),
    }
}

include!(concat!(env!("OUT_DIR"), "/i18n_catalog.rs"));

/// Substitute catalog placeholders in one pass. Inserted user content is never
/// scanned again, so braces in a pack name or system error stay literal.
pub fn interpolate(source: &'static str, arguments: &[(&str, String)]) -> String {
    let template = text(source);
    let mut output = String::with_capacity(template.len());
    let mut rest = template;
    while let Some(start) = rest.find('{') {
        output.push_str(&rest[..start]);
        let Some(end) = rest[start + 1..].find('}') else {
            output.push_str(&rest[start..]);
            return output;
        };
        let end = start + 1 + end;
        let key = &rest[start + 1..end];
        if let Some((_, value)) = arguments.iter().find(|(name, _)| *name == key) {
            output.push_str(value);
        } else {
            output.push_str(&rest[start..=end]);
        }
        rest = &rest[end + 1..];
    }
    output.push_str(rest);
    output
}

#[macro_export]
macro_rules! tr {
    ($source:literal) => {
        $crate::i18n::text($source)
    };
    ($source:literal, $($name:ident = $value:expr),+ $(,)?) => {
        $crate::i18n::interpolate($source, &[$((stringify!($name), ($value).to_string())),+])
    };
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn locale_detection_and_explicit_choices() {
        for locale in ["zh-CN", "zh_TW", "ZH-hans", "zh_CN.UTF-8"] {
            assert_eq!(
                InterfaceLanguage::System.resolve(Some(locale)),
                Language::Chinese
            );
        }
        for locale in [Some("en-US"), Some("fr_FR"), Some("C"), None] {
            assert_eq!(InterfaceLanguage::System.resolve(locale), Language::English);
        }
        assert_eq!(
            InterfaceLanguage::Chinese.resolve(Some("en-US")),
            Language::Chinese
        );
        assert_eq!(
            InterfaceLanguage::English.resolve(Some("zh-CN")),
            Language::English
        );
    }

    #[test]
    fn translated_errors_preserve_inserted_content() {
        struct Restore(Language);
        impl Drop for Restore {
            fn drop(&mut self) {
                LANGUAGE.set(self.0);
            }
        }
        let _restore = Restore(LANGUAGE.get());
        activate(InterfaceLanguage::English);
        let error = tr!(
            "无法读取知识包 JSON：{error}",
            error = "unexpected {p0}: 中文"
        );
        assert_eq!(
            error,
            "Cannot read knowledge pack JSON: unexpected {p0}: 中文"
        );
        activate(InterfaceLanguage::Chinese);
        assert_eq!(tr!("设置"), "设置");
    }

    #[test]
    fn preferences_are_backward_compatible_and_do_not_translate_content() {
        use crate::knowledge::{KnowledgePack, Settings};
        let old: Settings = serde_json::from_str("{}").unwrap();
        assert_eq!(old.interface_language, InterfaceLanguage::System);
        for language in [
            InterfaceLanguage::System,
            InterfaceLanguage::Chinese,
            InterfaceLanguage::English,
        ] {
            let settings = Settings {
                interface_language: language,
                ..old.clone()
            };
            let restored: Settings =
                serde_json::from_str(&serde_json::to_string(&settings).unwrap()).unwrap();
            assert_eq!(restored, settings);
        }
        assert!(serde_json::from_str::<Settings>(r#"{"interface_language":"unknown"}"#).is_err());
        let pack = KnowledgePack::from_json(
            include_str!("../knowledge_packs/authoring_template.json").as_bytes(),
        )
        .unwrap();
        let before = serde_json::to_string(&pack).unwrap();
        let previous = LANGUAGE.get();
        activate(InterfaceLanguage::English);
        let exported = serde_json::to_string(&pack).unwrap();
        LANGUAGE.set(previous);
        assert_eq!(exported, before);
    }
}
