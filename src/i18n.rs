//! UI translations via [rust-i18n](https://crates.io/crates/rust-i18n).
//!
//! Call [`activate`] once per frame from the main UI loop so [`tr_msg`] resolves the
//! language from user preferences without threading locale through every widget.

use crate::preferences::Language;

rust_i18n::i18n!("locales", fallback = "en");

pub fn activate(language: Language) {
    let locale = language.locale();
    if &*rust_i18n::locale() != locale {
        rust_i18n::set_locale(locale);
    }
}

/// Translate a stable key such as `repository.new_window` (see shortcut catalog).
pub fn tr_key(key: &str) -> String {
    translate(&rust_i18n::locale(), key)
}

fn translate(locale: &str, key: &str) -> String {
    _rust_i18n_try_translate(locale, key)
        .map(|text| text.into_owned())
        .unwrap_or_else(|| key.to_owned())
}

/// Translate using the English source text as the lookup key.
pub fn tr_msg(message: &str) -> String {
    tr_key(message)
}

pub fn tr_blocker(reason: Option<&str>) -> String {
    reason.map(tr_msg).unwrap_or_default()
}

pub fn tr_or(reason: Option<&str>, default: &str) -> String {
    reason.map(tr_msg).unwrap_or_else(|| tr_msg(default))
}

/// Translate with interpolation placeholders (`%{name}` in YAML).
pub fn tr_args(key: &str, args: &[(&str, &str)]) -> String {
    let mut text = tr_key(key);
    for (name, value) in args {
        text = text.replace(&format!("%{{{name}}}"), value);
    }
    text
}

#[cfg(test)]
pub(crate) fn activate_english() {
    activate(Language::English);
}

#[cfg(test)]
mod tests {
    use super::*;

    fn localized(locale: &str, key: &str) -> String {
        translate(locale, key)
    }

    #[test]
    fn missing_messages_preserve_source_text_without_locale_prefix() {
        for locale in ["en", "ru"] {
            assert_eq!(
                localized(locale, "unknown source message"),
                "unknown source message"
            );
            assert_eq!(
                localized(locale, "%{title} (%{count})"),
                "%{title} (%{count})"
            );
        }
    }

    #[test]
    fn file_counts_are_interpolated_in_each_language() {
        for (locale, expected) in [
            ("en", ["(3 files)", "3 files selected", "3 staged"]),
            ("ru", ["(файлов: 3)", "Выбрано файлов: 3", "В индексе: 3"]),
        ] {
            for (key, value) in [
                "(%{count} files)",
                "%{count} files selected",
                "%{count} staged",
            ]
            .into_iter()
            .zip(expected)
            {
                assert_eq!(localized(locale, key).replace("%{count}", "3"), value);
            }
        }
    }

    #[test]
    fn locale_files_contain_english_and_russian_text() {
        assert_eq!(localized("en", "Branches"), "Branches");
        assert_eq!(localized("ru", "Branches"), "Ветки");
        assert_eq!(localized("en", "Cancel"), "Cancel");
        assert_eq!(localized("ru", "Cancel"), "Отмена");
        assert_eq!(localized("ru", "Loading…"), "Загрузка…");
        assert_eq!(localized("ru", "menu_title.repository"), "Репозиторий");
        assert_eq!(
            localized("ru", "Patch exported to %{path}"),
            "Патч экспортирован в %{path}"
        );
    }

    #[test]
    fn russian_catalog_has_no_corrupted_text() {
        let catalog = include_str!("../locales/ru.yml");
        assert!(
            !catalog.contains("??"),
            "Russian text was replaced with question marks"
        );
        assert!(
            !catalog.contains('\u{fffd}'),
            "Russian catalog contains replacement characters"
        );
    }

    #[test]
    fn every_shortcut_key_has_translations() {
        for locale in ["en", "ru"] {
            for command in crate::shortcuts::catalog() {
                assert_ne!(
                    localized(locale, command.id),
                    command.id,
                    "missing {locale} translation for shortcut {}",
                    command.id
                );
                assert_ne!(
                    localized(locale, command.category_key),
                    command.category_key,
                    "missing {locale} translation for category {}",
                    command.category_key
                );
            }
        }
    }

    #[test]
    fn english_activation_is_neutral() {
        activate_english();
        assert_eq!(tr_msg("Branches"), "Branches");
    }
}
