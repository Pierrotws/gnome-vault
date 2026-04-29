#!/bin/sh
# Recompile the system GSettings schema cache after install or removal so
# the gnome-vault schema is picked up (or cleaned out).
set -e
if command -v glib-compile-schemas >/dev/null 2>&1; then
    glib-compile-schemas /usr/share/glib-2.0/schemas/ 2>/dev/null || true
fi
