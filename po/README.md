# Translations

Passel is translated with GNU gettext, the same catalogs on every
target: `po/<lang>.po`, compiled to `passel.mo`.

| Target  | Messages in                     | Looked up by                                  | Catalogs come from                         |
|---------|---------------------------------|-----------------------------------------------|--------------------------------------------|
| Desktop | `.blp`/`.ui` templates          | libintl (GtkBuilder, domain `passel`)          | `<localedir>/<lang>/LC_MESSAGES/passel.mo` (meson installs them) |
| Desktop | Rust (`src/`)                   | `passel_core::i18n` (`crate::i18n`)           | the same files                             |
| All     | Rust (`passel-core`, `passel-ffi`) | `passel_core::i18n`                         | installed by the front end                 |
| Android | Kotlin                          | `tr`/`trn`/`trp` (`I18n.kt`) → `passel-ffi`    | embedded in `libpassel_ffi.so` (`build.rs`) |
| iOS     | Swift (to come)                 | `translate`/`translatePlural`/`translateContext` from the UniFFI bindings | embedded, as on Android |

English is the source language: the English text is the message id, and
a missing translation shows the English.

## Writing messages

- Rust: `gettext("Save")`, `pgettext("verb", "Copy")`,
  `gettext_f("Deleted {name}", &[("name", &name)])`,
  `ngettext_f("{n} entry", "{n} entries", n, &[("n", &n.to_string())])`.
- Blueprint: `_("Save")`, `C_("verb", "Copy")`; each file declares
  `translation-domain "passel";`.
- Kotlin: `tr("Save")`, `trp("verb", "Copy")`,
  `tr("Deleted {name}", "name" to name)`,
  `trn("{n} entry", "{n} entries", n, "n" to n)`.
- Whole sentences only, never pieces glued together; values go in named
  `{placeholders}` so a translation can move them.
- The message must be one string literal in the call (no `format!`, no
  Kotlin `$` templates, no `"a " + "b"`: xgettext reads Kotlin as C and
  would keep only `"a "`) or extraction misses it.
- Explain anything ambiguous with a comment starting `Translators:` on
  the line before.
- Not translated: log messages, test strings, data written to files
  (field keys of the `pass` format, git commit messages and branch
  names), and names (Passel, GitHub, KeePassXC…).

## Updating

After changing messages, run `po/update.sh` (GNU gettext ≥ 0.24): it
refreshes `POTFILES`, `passel.pot` and every `<lang>.po`. Then
translate the new entries (empty or `#, fuzzy`) in `<lang>.po`.

## Adding a language

1. `msginit -i po/passel.pot -o po/<lang>.po -l <lang>` and translate.
2. Add `<lang>` to `po/LINGUAS`.
3. Add it to `android/app/src/main/res/xml/locales_config.xml`.

## French

Vault → coffre, entry → entrée, key → clé, passphrase → phrase
secrète, remote → dépôt distant, sync → synchroniser, token → jeton,
one-time password → code à usage unique. Typography: a no-break space
(U+00A0) before `:` and inside `« »`, a narrow no-break space (U+202F)
before `;`, `!` and `?`.
