# Windows

Passel for Windows is the same GTK app, built with [MSYS2](https://www.msys2.org)'s
UCRT64 toolchain and shipped with the GTK runtime it needs in an NSIS installer,
`passel-<version>-x86_64-setup.exe`. Each release gets one: release-please
dispatches `packages.yml`, whose `windows-installer` job attaches it to the release.

## Needs: Gpg4win

Passel encrypts with GnuPG, through gpgme, like on Linux. The installer does not
bundle GnuPG: MSYS2's pinentry would bring all of Qt along, and a second GnuPG
would fight [Gpg4win](https://gpg4win.org)'s over the same keyring
(`%APPDATA%\gnupg`). gpgme finds Gpg4win through the registry
(`HKLM\Software\GnuPG`); the installer offers its download page when it is missing.

Everything else follows pass and OpenSSH conventions: the store is
`%USERPROFILE%\.password-store` (or `PASSWORD_STORE_DIR`), SSH keys are read from
`%USERPROFILE%\.ssh`, and the SSH agent is Windows' OpenSSH agent or Pageant.

## Building

In an MSYS2 UCRT64 shell, from the repository root:

```sh
pacboy -S rust:p meson:p ninja:p pkgconf:p gettext-tools:p \
  gtk4:p libadwaita:p gpgme:p libgit2:p libssh2:p \
  librsvg:p adwaita-icon-theme:p nsis:p
export LIBGIT2_NO_VENDOR=1 LIBSSH2_SYS_USE_PKG_CONFIG=1

meson setup build --prefix="$(cygpath -m "$PWD/dist")" --buildtype=release -Dblueprint=disabled
meson compile -C build && meson install -C build
packaging/windows/bundle.sh dist          # adds the GTK runtime to dist/
makensis -INPUTCHARSET UTF8 -DVERSION=0.2.0 -DDIST="$(cygpath -w "$PWD/dist")" \
  packaging/windows/passel.nsi            # writes packaging/windows/passel-0.2.0-x86_64-setup.exe
```

`dist\bin\passel.exe` runs as is: the app finds its resources, translations,
schemas and icons from its own folder (`..\share`), so the tree can live anywhere.

| File | Role |
| --- | --- |
| `bundle.sh` | Copies the DLLs `passel.exe` loads (found with `objdump`), gdk-pixbuf's PNG/SVG loaders and their cache, GTK's settings schemas, the Adwaita icons and GTK/libadwaita/GLib translations into the install tree. Its header explains running it from Linux with wine. |
| `passel.nsi` | The installer: Program Files, all users, Start menu shortcut, Settings > Apps entry, uninstaller, in the app's five languages. |
| `passel.rc` | The icon and version details linked into `passel.exe` by `build.rs`. |

The installer is not code-signed: SmartScreen warns about it on first download.
