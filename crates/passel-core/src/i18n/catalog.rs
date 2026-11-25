//! A compiled gettext catalog (`.mo`), as `msgfmt` writes it.
//!
//! Layout: a header of 32-bit words in the writer's byte order (magic
//! `0x950412de`, revision, string count, offsets of the original and
//! translated string tables), then for each table one (length, offset)
//! pair per string. Originals are sorted; a context is prefixed as
//! `context\x04msgid`; plural messages hold their forms separated by NUL.
//! The empty msgid's translation is the header, with `Plural-Forms`.
//! Passel's catalogs are UTF-8 (`po/update-pot.sh` writes them so).

use std::collections::HashMap;
use std::fmt;

use super::plural::Expr;

const MAGIC: u32 = 0x9504_12de;

#[derive(Debug, PartialEq)]
pub enum CatalogError {
    /// Not a `.mo` file.
    BadMagic,
    /// A table or string lies outside the file.
    Truncated,
    /// The catalog declares a charset other than UTF-8.
    NotUtf8,
    /// The `Plural-Forms` header could not be parsed.
    BadPluralForms,
}

impl fmt::Display for CatalogError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            CatalogError::BadMagic => "not a gettext catalog",
            CatalogError::Truncated => "truncated catalog",
            CatalogError::NotUtf8 => "catalog is not UTF-8",
            CatalogError::BadPluralForms => "invalid Plural-Forms header",
        })
    }
}

impl std::error::Error for CatalogError {}

/// One language's translations.
#[derive(Debug)]
pub struct Catalog {
    /// Original (with `context\x04` prefix and, for plurals, only the
    /// singular) → translated forms.
    messages: HashMap<String, Vec<String>>,
    plural: Expr,
    nplurals: usize,
}

impl Catalog {
    /// Reads a `.mo` file's contents.
    pub fn parse(data: &[u8]) -> Result<Catalog, CatalogError> {
        let word_at = |offset: usize, big_endian: bool| -> Result<u32, CatalogError> {
            let bytes: [u8; 4] = data
                .get(offset..offset.checked_add(4).ok_or(CatalogError::Truncated)?)
                .ok_or(CatalogError::Truncated)?
                .try_into()
                .map_err(|_| CatalogError::Truncated)?;
            Ok(if big_endian {
                u32::from_be_bytes(bytes)
            } else {
                u32::from_le_bytes(bytes)
            })
        };
        let big_endian = match word_at(0, false)? {
            MAGIC => false,
            m if m.swap_bytes() == MAGIC => true,
            _ => return Err(CatalogError::BadMagic),
        };
        let word = |offset: usize| word_at(offset, big_endian).map(|w| w as usize);
        let count = word(8)?;
        let originals = word(12)?;
        let translations = word(16)?;
        let string = |table: usize, index: usize| -> Result<&str, CatalogError> {
            let entry = table
                .checked_add(index.checked_mul(8).ok_or(CatalogError::Truncated)?)
                .ok_or(CatalogError::Truncated)?;
            let len = word(entry)?;
            let start = word(entry + 4)?;
            let bytes = data
                .get(start..start.checked_add(len).ok_or(CatalogError::Truncated)?)
                .ok_or(CatalogError::Truncated)?;
            std::str::from_utf8(bytes).map_err(|_| CatalogError::NotUtf8)
        };

        // Every entry needs 16 table bytes: a bogus count can't make us
        // allocate more than the file justifies.
        if count > data.len() / 16 {
            return Err(CatalogError::Truncated);
        }
        let mut messages = HashMap::with_capacity(count);
        let mut header = "";
        for i in 0..count {
            let original = string(originals, i)?;
            let translated = string(translations, i)?;
            if original.is_empty() {
                header = translated;
                continue;
            }
            // Plural originals are "singular\0plural": key by singular.
            let key = original.split('\0').next().unwrap_or_default();
            let forms = translated.split('\0').map(str::to_owned).collect();
            messages.insert(key.to_owned(), forms);
        }

        let mut nplurals = 2;
        let mut plural = Expr::parse("n != 1").ok_or(CatalogError::BadPluralForms)?;
        for line in header.lines() {
            let Some((name, value)) = line.split_once(':') else {
                continue;
            };
            match name.trim().to_ascii_lowercase().as_str() {
                "content-type" => {
                    let charset = value
                        .split(';')
                        .filter_map(|p| p.trim().strip_prefix("charset="))
                        .next();
                    if charset.is_some_and(|c| !c.eq_ignore_ascii_case("utf-8")) {
                        return Err(CatalogError::NotUtf8);
                    }
                }
                "plural-forms" => {
                    for part in value.split(';') {
                        let Some((key, value)) = part.split_once('=') else {
                            continue;
                        };
                        match key.trim() {
                            "nplurals" => {
                                nplurals = value
                                    .trim()
                                    .parse()
                                    .map_err(|_| CatalogError::BadPluralForms)?;
                            }
                            // split_once: the expression has '=' of its own.
                            "plural" => {
                                plural = Expr::parse(value).ok_or(CatalogError::BadPluralForms)?;
                            }
                            _ => {}
                        }
                    }
                }
                _ => {}
            }
        }
        if nplurals == 0 {
            return Err(CatalogError::BadPluralForms);
        }
        Ok(Catalog {
            messages,
            plural,
            nplurals,
        })
    }

    /// Number of translated messages.
    pub fn len(&self) -> usize {
        self.messages.len()
    }

    pub fn is_empty(&self) -> bool {
        self.messages.is_empty()
    }

    fn lookup(&self, key: &str) -> Option<&[String]> {
        self.messages
            .get(key)
            .map(Vec::as_slice)
            // msgfmt leaves untranslated messages out, but an empty
            // string must never replace English.
            .filter(|forms| forms.first().is_some_and(|f| !f.is_empty()))
    }

    pub fn gettext<'a>(&'a self, msgid: &'a str) -> &'a str {
        self.lookup(msgid).map_or(msgid, |forms| &forms[0])
    }

    pub fn pgettext<'a>(&'a self, context: &str, msgid: &'a str) -> &'a str {
        self.lookup(&format!("{context}\x04{msgid}"))
            .map_or(msgid, |forms| &forms[0])
    }

    pub fn ngettext<'a>(&'a self, singular: &'a str, plural: &'a str, n: u64) -> &'a str {
        match self.lookup(singular) {
            Some(forms) => {
                let index = usize::try_from(self.plural.eval(n))
                    .unwrap_or(0)
                    .min(self.nplurals - 1);
                forms
                    .get(index)
                    .filter(|f| !f.is_empty())
                    .unwrap_or(&forms[0])
            }
            None if n == 1 => singular,
            None => plural,
        }
    }
}

#[cfg(test)]
pub(super) mod tests {
    use super::*;

    /// Writes a `.mo` file the way msgfmt lays it out (sorted originals).
    pub(crate) fn mo(entries: &[(&str, &str)], big_endian: bool) -> Vec<u8> {
        let mut entries: Vec<_> = entries.to_vec();
        entries.sort();
        let n = entries.len();
        let originals = 28;
        let translations = originals + 8 * n;
        let mut strings_at = translations + 8 * n;
        let mut header = Vec::new();
        let mut tables = Vec::new();
        let mut strings = Vec::new();
        let put = |out: &mut Vec<u8>, w: usize| {
            let w = w as u32;
            out.extend(if big_endian {
                w.to_be_bytes()
            } else {
                w.to_le_bytes()
            });
        };
        for w in [MAGIC as usize, 0, n, originals, translations, 0, 0] {
            put(&mut header, w);
        }
        let mut translated_table = Vec::new();
        for (original, _) in &entries {
            put(&mut tables, original.len());
            put(&mut tables, strings_at);
            strings.extend(original.as_bytes());
            strings.push(0);
            strings_at += original.len() + 1;
        }
        for (_, translated) in &entries {
            put(&mut translated_table, translated.len());
            put(&mut translated_table, strings_at);
            strings.extend(translated.as_bytes());
            strings.push(0);
            strings_at += translated.len() + 1;
        }
        [header, tables, translated_table, strings].concat()
    }

    const HEADER: &str =
        "Content-Type: text/plain; charset=UTF-8\nPlural-Forms: nplurals=2; plural=(n > 1);\n";

    fn french(big_endian: bool) -> Catalog {
        Catalog::parse(&mo(
            &[
                ("", HEADER),
                ("Save", "Enregistrer"),
                ("menu\x04Open", "Ouvrir"),
                ("{n} entry\0{n} entries", "{n} entrée\0{n} entrées"),
                ("Untranslated", ""),
            ],
            big_endian,
        ))
        .unwrap()
    }

    #[test]
    fn looks_up_messages_contexts_and_plurals() {
        for big_endian in [false, true] {
            let fr = french(big_endian);
            assert_eq!(fr.gettext("Save"), "Enregistrer");
            assert_eq!(fr.gettext("Missing"), "Missing");
            assert_eq!(fr.gettext("Untranslated"), "Untranslated");
            assert_eq!(fr.pgettext("menu", "Open"), "Ouvrir");
            assert_eq!(fr.gettext("Open"), "Open");
            assert_eq!(fr.ngettext("{n} entry", "{n} entries", 0), "{n} entrée");
            assert_eq!(fr.ngettext("{n} entry", "{n} entries", 1), "{n} entrée");
            assert_eq!(fr.ngettext("{n} entry", "{n} entries", 2), "{n} entrées");
            assert_eq!(fr.ngettext("{n} file", "{n} files", 0), "{n} files");
        }
    }

    #[test]
    fn rejects_bad_files() {
        assert_eq!(Catalog::parse(b"").unwrap_err(), CatalogError::Truncated);
        assert_eq!(
            Catalog::parse(&[0; 32]).unwrap_err(),
            CatalogError::BadMagic
        );
        let mut data = mo(&[("", HEADER), ("a", "b")], false);
        data.truncate(data.len() - 3);
        assert_eq!(Catalog::parse(&data).unwrap_err(), CatalogError::Truncated);
        let latin1 = mo(
            &[("", "Content-Type: text/plain; charset=ISO-8859-1\n")],
            false,
        );
        assert_eq!(Catalog::parse(&latin1).unwrap_err(), CatalogError::NotUtf8);
        let bad_plural = mo(&[("", "Plural-Forms: nplurals=2; plural=n >;\n")], false);
        assert_eq!(
            Catalog::parse(&bad_plural).unwrap_err(),
            CatalogError::BadPluralForms
        );
    }

    #[test]
    fn reads_a_catalog_compiled_by_msgfmt() {
        let Ok(msgfmt) = which("msgfmt") else {
            eprintln!("msgfmt not installed: skipped");
            return;
        };
        let dir = std::env::temp_dir().join(format!("passel-i18n-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let po = dir.join("fr.po");
        std::fs::write(
            &po,
            concat!(
                "msgid \"\"\nmsgstr \"\"\n",
                "\"Content-Type: text/plain; charset=UTF-8\\n\"\n",
                "\"Plural-Forms: nplurals=2; plural=(n > 1);\\n\"\n\n",
                "msgid \"Save\"\nmsgstr \"Enregistrer\"\n\n",
                "msgctxt \"menu\"\nmsgid \"Open\"\nmsgstr \"Ouvrir\"\n\n",
                "msgid \"{n} entry\"\nmsgid_plural \"{n} entries\"\n",
                "msgstr[0] \"{n} entrée\"\nmsgstr[1] \"{n} entrées\"\n",
            ),
        )
        .unwrap();
        let out = dir.join("fr.mo");
        let status = std::process::Command::new(msgfmt)
            .arg("-o")
            .arg(&out)
            .arg(&po)
            .status()
            .unwrap();
        assert!(status.success());
        let fr = Catalog::parse(&std::fs::read(&out).unwrap()).unwrap();
        std::fs::remove_dir_all(&dir).ok();
        assert_eq!(fr.len(), 3);
        assert_eq!(fr.gettext("Save"), "Enregistrer");
        assert_eq!(fr.pgettext("menu", "Open"), "Ouvrir");
        assert_eq!(fr.ngettext("{n} entry", "{n} entries", 0), "{n} entrée");
        assert_eq!(fr.ngettext("{n} entry", "{n} entries", 3), "{n} entrées");
    }

    fn which(name: &str) -> Result<std::path::PathBuf, ()> {
        std::env::var_os("PATH")
            .and_then(|paths| {
                std::env::split_paths(&paths)
                    .map(|dir| dir.join(name))
                    .find(|p| p.is_file())
            })
            .ok_or(())
    }
}
