# Compiles po/<lang>.po into <localedir>/<lang>/LC_MESSAGES/passel.mo
# (in the build dir too: build/po, where debug runs find them).
# `meson compile passel-pot passel-update-po` refresh the template and
# catalogs; po/update.sh does the same without a build dir and also
# refreshes POTFILES.
i18n.gettext(
  meson.project_name(),
  # Keep in sync with po/update.sh.
  args: [
    '--from-code=UTF-8',
    '--add-comments=Translators:',
    '--add-location=file',
    '--sort-by-file',
    '--keyword=gettext',
    '--keyword=gettext_f',
    '--keyword=pgettext:1c,2',
    '--keyword=ngettext:1,2',
    '--keyword=ngettext_f:1,2',
    '--keyword=tr',
    '--keyword=trp:1c,2',
    '--keyword=trn:1,2',
  ],
)
