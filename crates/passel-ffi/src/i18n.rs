//! Translations for the mobile apps: the same gettext catalogs as the
//! desktop (`po/*.po`), compiled into the library by `build.rs`.
//!
//! The app calls [`set_languages`] at startup and whenever the system
//! (or per-app) language changes, then looks its own messages up with
//! [`translate`] & co. Messages coming from Rust (errors, setup steps)
//! follow the same choice. Placeholders (`{name}`) are filled by the
//! caller after lookup.

use passel_core::i18n::{self, Catalog};

include!(concat!(env!("OUT_DIR"), "/catalogs.rs"));

/// Picks the language from `languages` (BCP 47 tags, most preferred
/// first, e.g. Android's `LocaleList.toLanguageTags()` split on commas)
/// and installs its catalog. Returns the language used, `"en"` when none
/// of them has a catalog.
#[uniffi::export]
pub fn set_languages(languages: Vec<String>) -> String {
    let available: Vec<&str> = CATALOGS.iter().map(|(lang, _)| *lang).collect();
    let chosen = i18n::negotiate(&languages, &available);
    let catalog = chosen.and_then(|lang| {
        let (_, data) = CATALOGS.iter().find(|(l, _)| *l == lang)?;
        Catalog::parse(data)
            .inspect_err(|err| log::warn!("Ignoring the {lang} catalog: {err}"))
            .ok()
    });
    let used = match &catalog {
        Some(_) => chosen.unwrap_or(i18n::SOURCE_LANGUAGE),
        None => i18n::SOURCE_LANGUAGE,
    };
    i18n::install(catalog);
    used.to_owned()
}

/// The languages the app is translated into, English first.
#[uniffi::export]
pub fn available_languages() -> Vec<String> {
    std::iter::once(i18n::SOURCE_LANGUAGE)
        .chain(CATALOGS.iter().map(|(lang, _)| *lang))
        .map(str::to_owned)
        .collect()
}

/// The translation of `msgid`.
#[uniffi::export]
pub fn translate(msgid: String) -> String {
    i18n::gettext(&msgid)
}

/// The translation of `msgid` in `context`.
#[uniffi::export]
pub fn translate_context(context: String, msgid: String) -> String {
    i18n::pgettext(&context, &msgid)
}

/// The singular or plural translation for `n`.
#[uniffi::export]
pub fn translate_plural(singular: String, plural: String, n: u64) -> String {
    i18n::ngettext(&singular, &plural, n)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_embedded_catalog_parses() {
        for (lang, data) in CATALOGS {
            Catalog::parse(data).unwrap_or_else(|err| panic!("{lang}: {err}"));
        }
        assert_eq!(available_languages()[0], "en");
    }
}
