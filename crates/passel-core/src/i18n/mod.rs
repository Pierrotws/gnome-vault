//! Translations: GNU gettext catalogs (`po/*.po`, compiled to `.mo`),
//! read without libintl so every front end shares one implementation.
//!
//! English is the source language: a message's English text is its key
//! (`msgid`), and with no catalog installed every function returns it
//! unchanged. A front end installs the catalog for the user's language
//! once at startup, before showing anything:
//!
//! - the GTK app loads `<localedir>/<lang>/LC_MESSAGES/passel.mo`, the
//!   same files libintl reads for its `.ui` templates
//!   ([`install_from_dir`]);
//! - the mobile library embeds the compiled catalogs and picks one from
//!   the system's locale list ([`negotiate`] + [`install`]).
//!
//! Messages with values in them use named placeholders, `{name}`, filled
//! by [`gettext_f`] / [`ngettext_f`], so a translation can reorder them.
//! The extraction keywords (see `po/update-pot.sh`) are this module's
//! function names.

mod catalog;
mod plural;

use std::path::Path;
use std::sync::{Arc, RwLock};

pub use catalog::{Catalog, CatalogError};

/// The gettext domain: `passel.mo`, `po/passel.pot`.
pub const DOMAIN: &str = "passel";

/// The language messages are written in. It needs no catalog.
pub const SOURCE_LANGUAGE: &str = "en";

static CURRENT: RwLock<Option<Arc<Catalog>>> = RwLock::new(None);

/// Makes `catalog` the one every lookup uses; `None` goes back to
/// English.
pub fn install(catalog: Option<Catalog>) {
    let mut current = CURRENT.write().unwrap_or_else(|e| e.into_inner());
    *current = catalog.map(Arc::new);
}

fn current() -> Option<Arc<Catalog>> {
    CURRENT.read().unwrap_or_else(|e| e.into_inner()).clone()
}

/// Picks the language to show from `wanted` (the user's languages, most
/// preferred first, as POSIX locale names like `fr_FR.UTF-8` or BCP 47
/// tags like `fr-FR`) among `available` (catalog names like `fr`,
/// `pt_BR`). English needs no catalog, so it is always available: an
/// English locale ahead of French one picks English.
///
/// Returns the catalog name, or `None` for English (or nothing matched).
pub fn negotiate<'a>(wanted: &[impl AsRef<str>], available: &[&'a str]) -> Option<&'a str> {
    for tag in wanted {
        let Some((lang, region)) = parse_locale(tag.as_ref()) else {
            continue;
        };
        if lang == SOURCE_LANGUAGE {
            return None;
        }
        // `pt_BR` before `pt`.
        if let Some(region) = &region {
            if let Some(found) = available
                .iter()
                .find(|name| parse_locale(name) == Some((lang.clone(), Some(region.clone()))))
            {
                return Some(found);
            }
        }
        // Then the generic catalog (`pt`), then another region's.
        if let Some(found) = available
            .iter()
            .find(|name| parse_locale(name) == Some((lang.clone(), None)))
            .or_else(|| {
                available
                    .iter()
                    .find(|name| parse_locale(name).is_some_and(|(l, _)| l == lang))
            })
        {
            return Some(found);
        }
    }
    None
}

/// `fr_FR.UTF-8@euro`, `fr-FR`, `fr` → (`fr`, `FR`); `C`/`POSIX` and
/// empty strings are no language.
fn parse_locale(tag: &str) -> Option<(String, Option<String>)> {
    let tag = tag.split(['.', '@']).next().unwrap_or_default();
    if tag.is_empty() || tag == "C" || tag == "POSIX" {
        return None;
    }
    let mut parts = tag.split(['_', '-']);
    let lang = parts.next()?.to_ascii_lowercase();
    if lang.is_empty() || !lang.chars().all(|c| c.is_ascii_alphabetic()) {
        return None;
    }
    // BCP 47 may put a script before the region (`zh-Hant-TW`).
    let region = parts
        .find(|p| p.len() == 2 || p.chars().all(|c| c.is_ascii_digit()))
        .map(str::to_ascii_uppercase);
    Some((lang, region))
}

/// Loads and installs the catalog for the first of `languages` that has
/// one in `localedir` (`<localedir>/<lang>/LC_MESSAGES/passel.mo`), the
/// layout gettext installs and libintl reads. Returns the language
/// installed, or `None` for English.
pub fn install_from_dir(localedir: &Path, languages: &[impl AsRef<str>]) -> Option<String> {
    let available: Vec<String> = std::fs::read_dir(localedir)
        .into_iter()
        .flatten()
        .flatten()
        .filter(|entry| catalog_path(localedir, &entry.file_name().to_string_lossy()).is_file())
        .map(|entry| entry.file_name().to_string_lossy().into_owned())
        .collect();
    let available: Vec<&str> = available.iter().map(String::as_str).collect();
    let lang = negotiate(languages, &available)?;
    let path = catalog_path(localedir, lang);
    match std::fs::read(&path)
        .map_err(|e| e.to_string())
        .and_then(|bytes| Catalog::parse(&bytes).map_err(|e| e.to_string()))
    {
        Ok(catalog) => {
            install(Some(catalog));
            Some(lang.to_owned())
        }
        Err(err) => {
            log::warn!("Ignoring translations in {}: {err}", path.display());
            install(None);
            None
        }
    }
}

fn catalog_path(localedir: &Path, lang: &str) -> std::path::PathBuf {
    localedir
        .join(lang)
        .join("LC_MESSAGES")
        .join(format!("{DOMAIN}.mo"))
}

/// The translation of `msgid`.
pub fn gettext(msgid: &str) -> String {
    match current() {
        Some(catalog) => catalog.gettext(msgid).to_owned(),
        None => msgid.to_owned(),
    }
}

/// The translation of `msgid` in `context`, for an English text that
/// needs different translations in different places.
pub fn pgettext(context: &str, msgid: &str) -> String {
    match current() {
        Some(catalog) => catalog.pgettext(context, msgid).to_owned(),
        None => msgid.to_owned(),
    }
}

/// The singular or plural translation for `n`, by the target language's
/// plural rules (French counts 0 as singular, English does not).
pub fn ngettext(singular: &str, plural: &str, n: u64) -> String {
    match current() {
        Some(catalog) => catalog.ngettext(singular, plural, n).to_owned(),
        None => english_plural(singular, plural, n).to_owned(),
    }
}

fn english_plural<'a>(singular: &'a str, plural: &'a str, n: u64) -> &'a str {
    if n == 1 {
        singular
    } else {
        plural
    }
}

/// [`gettext`], then fills each `{name}` placeholder from `args`.
pub fn gettext_f(msgid: &str, args: &[(&str, &str)]) -> String {
    format_named(&gettext(msgid), args)
}

/// [`ngettext`], then fills each `{name}` placeholder from `args`.
pub fn ngettext_f(singular: &str, plural: &str, n: u64, args: &[(&str, &str)]) -> String {
    format_named(&ngettext(singular, plural, n), args)
}

/// Replaces `{name}` with its value from `args`, in one pass (a value
/// containing `{other}` is left alone). Unknown placeholders stay as
/// they are, so a translation mistake shows up instead of vanishing.
pub fn format_named(template: &str, args: &[(&str, &str)]) -> String {
    let mut out = String::with_capacity(template.len());
    let mut rest = template;
    while let Some(open) = rest.find('{') {
        out.push_str(&rest[..open]);
        let after = &rest[open + 1..];
        let replaced = after.find('}').and_then(|close| {
            let name = &after[..close];
            args.iter()
                .find(|(key, _)| *key == name)
                .map(|(_, value)| (close, *value))
        });
        match replaced {
            Some((close, value)) => {
                out.push_str(value);
                rest = &after[close + 1..];
            }
            None => {
                out.push('{');
                rest = after;
            }
        }
    }
    out.push_str(rest);
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn negotiates_posix_and_bcp47_names() {
        let available = ["fr", "pt_BR", "pt"];
        assert_eq!(negotiate(&["fr_FR.UTF-8"], &available), Some("fr"));
        assert_eq!(negotiate(&["fr-CA"], &available), Some("fr"));
        assert_eq!(negotiate(&["pt-BR"], &available), Some("pt_BR"));
        assert_eq!(negotiate(&["pt_PT"], &available), Some("pt"));
        assert_eq!(negotiate(&["de_DE", "fr"], &available), Some("fr"));
        assert_eq!(negotiate(&["C"], &available), None);
        assert_eq!(negotiate(&[] as &[&str], &available), None);
    }

    #[test]
    fn english_ahead_of_french_picks_english() {
        assert_eq!(negotiate(&["en_GB", "fr_FR"], &["fr"]), None);
        assert_eq!(negotiate(&["en-US"], &["fr"]), None);
    }

    #[test]
    fn fills_named_placeholders() {
        assert_eq!(
            format_named("{count} sur {total}", &[("total", "5"), ("count", "2")]),
            "2 sur 5"
        );
        assert_eq!(format_named("{a}", &[("a", "{b}"), ("b", "x")]), "{b}");
        assert_eq!(format_named("{missing} {", &[]), "{missing} {");
        assert_eq!(format_named("é{x}è", &[("x", "à")]), "éàè");
    }

    #[test]
    fn english_plurals_without_a_catalog() {
        assert_eq!(english_plural("1 entry", "{n} entries", 1), "1 entry");
        assert_eq!(english_plural("1 entry", "{n} entries", 0), "{n} entries");
    }
}
