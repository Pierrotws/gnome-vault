#!/usr/bin/env bash
# Makes the Windows install tree: Passel as meson installed it, plus the
# GTK runtime it needs from MSYS2 (DLLs, image loaders, settings schemas,
# icons, GTK's own translations). passel.nsi packs the result.
#
#   packaging/windows/bundle.sh <dir>
#
# <dir> is a meson install prefix (bin/passel.exe, share/...), completed in
# place. Run from the repository root in an MSYS2 UCRT64 shell, where
# MINGW_PREFIX is the toolchain's root (/ucrt64).
#
# Cross builds from Linux work too: point MINGW_PREFIX at an extracted
# MSYS2 package tree, OBJDUMP at x86_64-w64-mingw32-objdump and RUN_EXE
# at wine (gdk-pixbuf-query-loaders is a Windows program); the schema and
# icon cache compilers then come from the host, their output is the same.
#
# GnuPG is not bundled: Passel uses the user's (Gpg4win), found by gpgme
# through the registry. MSYS2's pinentry would bring all of Qt along.

set -euo pipefail

dist=${1:?usage: bundle.sh <install dir>}
prefix=${MINGW_PREFIX:?MINGW_PREFIX is not set (run from an MSYS2 UCRT64 shell)}
objdump=${OBJDUMP:-objdump}
run_exe=${RUN_EXE:-}
compile_schemas=${GLIB_COMPILE_SCHEMAS:-glib-compile-schemas}
update_icon_cache=${GTK_UPDATE_ICON_CACHE:-gtk4-update-icon-cache}

[[ -f "$dist/bin/passel.exe" ]] || { echo "$dist/bin/passel.exe is missing" >&2; exit 1; }

pixbuf_dir=lib/gdk-pixbuf-2.0/2.10.0
# PNG for raster icons, SVG for the scalable ones.
pixbuf_loaders=(libpixbufloader-png.dll pixbufloader_svg.dll)

# --- Programs besides passel.exe --------------------------------------
# GLib spawns through its helpers; gdbus.exe gives GApplication its
# session bus (one window per launch otherwise).
#
# Not gpgme-w32spawn.exe: when it sits next to libgpgme, gpgme starts gpg
# through it, and MSYS2's build garbles the pipe handles on the way
# (written with printf's %p, which the UCRT prints without "0x", then read
# back with strtoull base 0, which takes the leading 0 for octal), so gpg
# gets a wrong --logger-fd and exits at once: every operation fails.
# Without it, gpgme starts gpg itself and passes the handles in decimal.
for exe in gspawn-win64-helper.exe gspawn-win64-helper-console.exe gdbus.exe; do
  install -m755 "$prefix/bin/$exe" "$dist/bin/$exe"
done
mkdir -p "$dist/$pixbuf_dir/loaders"
for loader in "${pixbuf_loaders[@]}"; do
  install -m755 "$prefix/$pixbuf_dir/loaders/$loader" "$dist/$pixbuf_dir/loaders/$loader"
done

# --- DLLs: everything those load from MSYS2, transitively ------------
# Windows' own DLLs (kernel32, ucrtbase, api-ms-win-*…) are not in
# $prefix/bin and are left out.
declare -A seen=()
queue=("$dist"/bin/*.exe "$dist/$pixbuf_dir"/loaders/*.dll)
while ((${#queue[@]})); do
  file=${queue[0]}
  queue=("${queue[@]:1}")
  while read -r dll; do
    key=${dll,,}
    [[ -n "${seen[$key]:-}" ]] && continue
    seen[$key]=1
    src=$(find "$prefix/bin" -maxdepth 1 -iname "$dll" -print -quit)
    [[ -n "$src" ]] || continue
    install -m755 "$src" "$dist/bin/$(basename "$src")"
    queue+=("$dist/bin/$(basename "$src")")
  done < <("$objdump" -p "$file" | sed -n 's/^\s*DLL Name: //p' | tr -d '\r')
done

# --- Image loaders' cache ---------------------------------------------
# gdk-pixbuf resolves relative paths in it against the install folder,
# so the cache works wherever the app is installed.
query_loaders=$(cd "$prefix/bin" && pwd)/gdk-pixbuf-query-loaders.exe
# Relative paths, from inside the tree: wine has no use for Linux paths.
(cd "$dist" &&
  # shellcheck disable=SC2086 # RUN_EXE is "wine" or nothing.
  $run_exe "$query_loaders" "${pixbuf_loaders[@]/#/$pixbuf_dir/loaders/}") |
  tr -d '\r' |
  sed -E 's#^"[^"]*[/\\](lib[/\\]gdk-pixbuf-2\.0[/\\][^"]*)"#"\1"#' \
    > "$dist/$pixbuf_dir/loaders.cache"
for loader in "${pixbuf_loaders[@]}"; do
  grep -q "$loader" "$dist/$pixbuf_dir/loaders.cache" ||
    { echo "loaders.cache lacks $loader" >&2; exit 1; }
done

# --- Settings schemas: Passel's and GTK's own (file chooser…) --------
schemas="$dist/share/glib-2.0/schemas"
mkdir -p "$schemas"
cp "$prefix"/share/glib-2.0/schemas/org.gtk.gtk4.Settings.*.gschema.xml "$schemas/"
"$compile_schemas" --strict "$schemas"

# --- Icons: Adwaita's (the symbolic icons the UI uses) and hicolor ----
mkdir -p "$dist/share/icons/hicolor"
cp -r "$prefix/share/icons/Adwaita" "$dist/share/icons/"
cp "$prefix/share/icons/hicolor/index.theme" "$dist/share/icons/hicolor/"
for theme in Adwaita hicolor; do
  "$update_icon_cache" --quiet --force --ignore-theme-index "$dist/share/icons/$theme" || true
done

# --- GTK's, libadwaita's and GLib's strings in Passel's languages -----
while read -r lang; do
  [[ -z "$lang" || "$lang" == \#* ]] && continue
  for domain in gtk40 libadwaita glib20; do
    mo="$prefix/share/locale/$lang/LC_MESSAGES/$domain.mo"
    [[ -f "$mo" ]] && install -Dm644 "$mo" "$dist/share/locale/$lang/LC_MESSAGES/$domain.mo"
  done
done < po/LINGUAS

install -Dm644 LICENSE "$dist/share/licenses/passel/LICENSE"

echo "Bundled $(find "$dist" -type f | wc -l) files, $(du -sh "$dist" | cut -f1)"
