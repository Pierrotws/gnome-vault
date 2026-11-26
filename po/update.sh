#!/usr/bin/env bash
# Refreshes the translation template and catalogs after messages change:
#
#   po/update.sh
#
# 1. po/POTFILES: every source that may hold messages, on all targets
#    (desktop Rust and .ui, passel-core, the mobile API, Android Kotlin;
#    not the i18n code itself, whose tests look messages up).
# 2. po/passel.pot: the messages, extracted by xgettext.
# 3. po/<lang>.po for each language in po/LINGUAS: merged with the new
#    template (new messages empty, changed ones marked fuzzy).
#
# Needs GNU gettext >= 0.24 (xgettext reads Rust since then). meson's
# `passel-pot` and `passel-update-po` targets do steps 2 and 3 with the
# same options (po/meson.build).
set -euo pipefail
cd "$(dirname "$0")/.."

git ls-files \
    'src/*.rs' 'crates/*.rs' \
    'assets/ui/*.ui' 'assets/*.desktop.in' \
    'android/app/src/main/kotlin/*.kt' \
    | grep -v -e '/tests/' -e '/i18n/' -e '/i18n\.rs$' | sort > po/POTFILES

# Keep in sync with po/meson.build.
extract() {
    xgettext \
        --package-name=passel --msgid-bugs-address=https://github.com/Pierrotws/passel/issues \
        --from-code=UTF-8 --add-comments=Translators: --add-location=file --sort-by-file \
        --keyword=gettext --keyword=gettext_f --keyword=pgettext:1c,2 \
        --keyword=ngettext:1,2 --keyword=ngettext_f:1,2 \
        --keyword=tr --keyword=trp:1c,2 --keyword=trn:1,2 \
        --output=po/passel.pot "$@"
}
# xgettext has no Kotlin reader; its C one handles our tr() calls (one
# literal per message, see po/README.md). Saying so avoids a warning
# per file.
extract --files-from=<(grep -v '\.kt$' po/POTFILES)
extract --join-existing --language=C --files-from=<(grep '\.kt$' po/POTFILES)

while read -r lang; do
    [[ -n $lang && $lang != \#* ]] || continue
    msgmerge --quiet --update --backup=none --add-location=file --sort-by-file \
        "po/$lang.po" po/passel.pot
    # Messages gone from the sources leave no `#~` leftovers.
    msgattrib --no-obsolete --sort-by-file --output-file="po/$lang.po" "po/$lang.po"
    msgfmt --check --statistics --output-file=/dev/null "po/$lang.po"
done < po/LINGUAS
