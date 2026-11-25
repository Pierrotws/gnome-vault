# Architecture Overview

This document follows the structure from [architecture.md](https://architecture.md/) and is intended as a living map of the codebase. Update it when module boundaries, storage behavior, integrations, or build/deployment assumptions change.

## 1. Project Structure

```text
passel/
|-- assets/
|   |-- resources.gresource.xml        # GTK resource bundle manifest
|   |-- schemas/                       # GSettings schema and schema build rules
|   `-- ui/                            # Blueprint UI sources and generated .ui fallbacks
|       `-- fields/                    # Entry field row UI sources and generated fallbacks
|-- crates/passel-core/                # UI-free library: no GTK/GLib (CI checks)
|   `-- src/
|       |-- lib.rs                     # Re-exported by the app as crate::{app, pass, helpers}
|       |-- app/                       # (tree below)
|       |-- helpers/                   # (tree below, all but clipboard.rs)
|       `-- pass/                      # (tree below)
|-- src/                               # The GTK app (`passel` binary)
|   |-- main.rs                        # Application startup, resources, schema lookup
|   |-- app/                           # [passel-core] UI-facing application controller and state
|   |   |-- controller.rs              # Main application use-case API
|   |   |-- app_error.rs               # App-level error translation
|   |   |-- cache_warmup.rs            # Background entry-cache decryption worker
|   |   |-- changes.rs                 # Worker-safe wrappers for revert/rollback/push
|   |   |-- vaults.rs                  # The list of vaults and their settings (pure data)
|   |   |-- group_preview.rs           # Worker-side decrypt for group-row subtitles
|   |   |-- setup.rs                   # Worker-safe wrappers for the setup wizard
|   |   `-- state/                     # Tree, selected entry, edit session, entry cache
|   |-- helpers/                       # [passel-core] helpers; clipboard.rs stays in the app
|   |   |-- clipboard.rs               # Centralized "copy secret" entry point
|   |   |-- entry_preview.rs           # Subtitle formatting for group rows
|   |   |-- git.rs                     # Git repository operations through git2
|   |   |-- macros.rs                  # Internal macros
|   |   |-- otp.rs                     # TOTP parsing and generation
|   |   |-- parser.rs                  # pass plaintext <-> EntryData parser/formatter
|   |   |-- password.rs                # Password generation
|   |   `-- pgp/                       # OpenPGP: PgpBackend + gpgme.rs (desktop) / rpgp.rs (mobile)
|   |-- pass/                          # [passel-core] password-store domain and persistence boundary
|   |   |-- model/                     # EntryData, EntryField, PassNode
|   |   |-- import/                    # Readers for KeePassXC (.kdbx) and Buttercup (.bcup) vaults
|   |   `-- store/                     # File, GPG, and Git-backed store operations
|   `-- ui/                            # GTK/Adwaita widgets and templates
|       |-- window/                    # Main window orchestration
|       |   |-- mod.rs                 # MainWindow wiring, callbacks, edit-mode glue
|       |   |-- imp.rs                 # Composite-template fields and ObjectSubclass
|       |   |-- autoload.rs            # Background cache-warmup tick handler
|       |   |-- group_content.rs       # Right-pane group/search-results rendering
|       |   |-- new_entry_dialog.rs    # New-entry creation dialog
|       |   |-- preferences.rs         # Preferences dialog wiring
|       |   |-- import_dialog.rs       # Import Passwords dialog
|       |   |-- setup.rs               # Setup wizard dialog, turn-on-history page
|       |   |-- unsaved.rs             # "Discard changes?" before leaving an edited entry
|       |   `-- vaults.rs              # Vault list, switcher, vault settings, add folder
|       |-- vault_view/                # Vault tree, search, programmatic selection
|       |-- group_view/                # Right-pane list of entries in a group
|       |-- entry_view/                # Entry display/editing and field rows
|       |-- changes_view/              # Git history and change actions
|       |-- setup_wizard/              # First-run wizard: pages, plan, step progress
|       `-- generate_password_view/    # Password generator widget
|-- packaging/
|   |-- arch/passel/PKGBUILD      # Arch source build
|   |-- arch/passel-bin/PKGBUILD  # Arch binary (consumes release tarball)
|   `-- debian/postinst.sh             # .deb post-install GSettings recompile
|-- .github/workflows/packages.yml     # Build .deb + binary tarball on release
|-- Cargo.toml                         # Rust dependencies
|-- meson.build                        # Meson project entry point
`-- README.md                          # User/developer quick start
```

## 2. High-Level System Diagram

```text
[User]
  |
  v
[GTK/Adwaita UI in src/ui]
  |
  v
[AppController in crates/passel-core/src/app/controller.rs]
  |
  +--> [AppState: selected node, edit session, decrypted cache]
  |
  v
[pass::store facade]
  |
  +--> [helpers::parser] <-> pass plaintext format
  +--> [helpers::pgp]    <-> PgpBackend: GPGME + GnuPG keyring (desktop) or rPGP + app keyring (mobile)
  +--> [helpers::git]    <-> git2 repository and remote
  |
  v
[the open vault's folder (one of several listed vaults)]
```

The UI never writes password-store files directly. Widgets emit signals, `MainWindow` translates them into controller calls, and the controller delegates persistence to `pass::store`. Every store operation takes the vault folder explicitly: the controller holds the open vault's folder and hands it to background workers, so switching vaults needs no restart.

## 3. Core Components

### 3.1. Desktop Application UI

Name: GTK/Adwaita application

Description: The user interface for browsing entries, opening a cached entry view, editing fields, creating/deleting entries, configuring preferences, and viewing Git-backed change history.

Technologies: GTK4, libadwaita, gtk-rs, Blueprint templates, GResource.

Deployment: Built as a Rust binary through Meson/Cargo. UI templates and schemas are compiled into build assets and installed by Meson.

Primary files:

| File | Responsibility |
| --- | --- |
| `src/main.rs` | Initializes logging, GResources, local schema lookup, and the Adwaita application. |
| `src/ui/window/mod.rs` | Main UI orchestration and signal wiring. Edit mode belongs to the entry view. |
| `src/ui/window/setup.rs` | The setup wizard's dialog with the "No Vault Yet" page behind it, and the "Turn On History" page for a pass store without git. |
| `src/ui/window/import_dialog.rs` | "Import Passwords" dialog (vault menu): source, file, master password, destination folder; preview, then import in one commit. |
| `src/ui/window/vaults.rs` | The vault list in GSettings (with the one-time migration from `store-dir`), opening and switching vaults, the header-bar switcher, each vault's settings dialog (name, history, remote, branch, auto-push, remove), and adding an existing folder. |
| `src/ui/setup_wizard/` | First-run wizard (`Adw.NavigationView`): collects a `SetupPlan`, then runs its steps on a worker with per-step progress, retry and skip. `key.rs` holds the encryption key page (use, create or import a GPG key, save a backup); `sync.rs` holds the sync pages (online copy, folder/drive copy, this computer only); `hosts.rs` lists the hosting services with free private repositories and how to create an empty one on each. |
| `src/ui/window/preferences.rs` | Preferences dialog (autoload, group view, Markdown, tree size). |
| `src/ui/window/autoload.rs` | Background entry-cache warmup tick handler. |
| `src/ui/window/group_content.rs` | Right-pane rendering for group browsing and flat search results. |
| `src/ui/window/new_entry_dialog.rs` | New-entry creation dialog. |
| `src/ui/window/unsaved.rs` | Asks "Discard changes?" before selecting another entry or group, switching vault or closing the window with unsaved edits, and puts the tree's selection back when they are kept. |
| `assets/ui/window.blp` | Main window layout and stack structure. |
| `src/ui/entry_view/` | Entry display/editing and field row integration. |
| `src/ui/vault_view/` | Vault tree rendering, selection, search, and context actions. |
| `src/ui/group_view/` | Right-pane list of entries inside the selected group. |
| `src/ui/changes_view/` | Change list rendering, lazy history paging, and change context actions. |

### 3.2. Application Controller and State

Name: Application layer

Description: Provides a narrow API for the UI. It owns in-memory application state, enforces edit/session rules, and translates UI actions into domain/store operations.

Technologies: Plain Rust structs with GTK-facing ownership handled by `Rc<RefCell<AppController>>`.

Primary files:

| File | Responsibility |
| --- | --- |
| `crates/passel-core/src/app/controller.rs` | Use-case methods for loading trees, opening entries, saving, creating, deleting, renaming, search filter, and cache warmup. |
| `crates/passel-core/src/app/changes.rs` | Worker-safe free functions for revert / rollback / push (called from `gio::spawn_blocking`). |
| `crates/passel-core/src/app/import.rs` | Worker-safe wrappers for importing: decrypt a KeePassXC/Buttercup file, write its entries into the open vault. |
| `crates/passel-core/src/app/vaults.rs` | `VaultList` / `VaultConfig`: the listed vaults, the active one, unique names and folders for new vaults. |
| `crates/passel-core/src/app/cache_warmup.rs` | Spawns the autoload decrypt worker and streams results back to the main loop. |
| `crates/passel-core/src/app/group_preview.rs` | Spawns the per-group decrypt worker that fills missing subtitle previews. |
| `crates/passel-core/src/app/setup.rs` | Worker-safe wrappers the setup wizard calls: plan check, setup steps, key listing, creation, import and backup, folder status, remote check, copy-folder status, public SSH keys. |
| `crates/passel-core/src/app/state/app_state.rs` | Current tree, selected node, current edit session, decrypted entry cache. |
| `crates/passel-core/src/app/state/entry_session.rs` | Dirty tracking, validation, title changes, save/revert session state. |
| `crates/passel-core/src/app/app_error.rs` | Error type surfaced to the UI. |

### 3.3. Password Store Domain

Name: pass model and store facade

Description: Represents password-store entries and nodes, and exposes filesystem/GPG/Git-backed operations to the controller.

Technologies: Rust domain structs, standard filesystem API, GPGME, git2.

Primary files:

| File | Responsibility |
| --- | --- |
| `crates/passel-core/src/pass/model/entry_data.rs` | Complete entry model: mandatory first field plus custom fields. |
| `crates/passel-core/src/pass/model/entry_field.rs` | Typed entry values: password, plain, OTP, array, multiline. |
| `crates/passel-core/src/pass/model/pass_node.rs` | Vault tree nodes for groups and `.gpg` entries. |
| `crates/passel-core/src/pass/store/mod.rs` | Store facade: load, store status check, setup plan and steps, enable history, save, create, rename, delete, history, push, revert, rollback. |
| `crates/passel-core/src/pass/store/store_error.rs` | Store-level error type. |
| `crates/passel-core/src/pass/store/import.rs` | Writes imported entries under a destination folder (valid, unique names) in one commit. |
| `crates/passel-core/src/pass/import/` | Decrypts and maps other password managers' vaults to pass entries (`keepass.rs`, `buttercup.rs`); the trash is skipped. |

### 3.4. Infrastructure Helpers

Name: helpers

Description: Small modules for implementation-specific concerns that should not pollute UI or state code.

Technologies: git2, gpgme, HMAC-SHA1, Base32, standard Rust.

Primary files:

| File | Responsibility |
| --- | --- |
| `crates/passel-core/src/helpers/parser.rs` | Parses and formats the pass-compatible YAML subset for custom fields. |
| `crates/passel-core/src/helpers/pgp/` | Decrypts/encrypts `.gpg` files, lists usable local recipients, reads `.gpg-id`. The crypto goes through the process-wide `PgpBackend`: `gpgme.rs` (feature `gpgme`, the default: GnuPG keyring, agent, pinentry) or `rpgp.rs` (feature `rpgp`: pure Rust, keyring in a folder the app picks, passphrases through a `PassphraseProvider` callback and cached until `lock()`), set with `pgp::install_backend`. Tests check rPGP against the real gpg in both directions. |
| `crates/passel-core/src/helpers/git.rs` | Repository initialization, add/remove/rename, commit, push, history, revert, rollback. |
| `crates/passel-core/src/helpers/git_credentials.rs` | How remotes authenticate, through the process-wide `CredentialProvider`: `SystemCredentials` (default: ssh-agent, then git's credential helpers) or `StoredCredentials` (credentials the app keeps per host: HTTPS token or in-memory SSH key, offered once per operation), set with `install_credential_provider`. |
| `crates/passel-core/src/helpers/otp.rs` | Validates `otpauth://` URLs and generates current TOTP values. |
| `crates/passel-core/src/helpers/password.rs` | Generates passwords for the new-entry dialog. |
| `src/helpers/clipboard.rs` | Single entry point for copying secrets into the GTK clipboard. |
| `crates/passel-core/src/helpers/entry_preview.rs` | Formats the first-field preview shown as a group-row subtitle. |

## 4. Data Stores

### 4.1. Password Store Repository

Name: password store

Type: Local folder of GPG-encrypted files, usually a Git repository.

Location: any number of folders listed in the `vaults` setting. The first one defaults to `PASSWORD_STORE_DIR` when set, otherwise `~/.password-store` (debug builds: the dev store). A vault without history (no `.git`) is saved without commits; pushes happen only when a remote is configured.

Purpose: Stores user secrets as pass-compatible `.gpg` files. Directories are represented as groups; `.gpg` files are represented as entries.

Important files:

| Path | Purpose |
| --- | --- |
| `*.gpg` | Encrypted entry payload. |
| `.gpg-id` | Recipient key IDs used when encrypting entries. |
| `.git/` | Change history and synchronization state (optional). |

### 4.2. GSettings

Name: application preferences

Type: GLib GSettings schema.

Location: `assets/schemas/io.pierrotws.Passel.gschema.xml`.

Purpose: Stores the vault list (`vaults`: name, folder, history answer, branch, auto-push per vault) and the active vault, plus application preferences such as startup cache autoload. `store-dir`, `branch` and `autopush` are deprecated: read once to build the first vault, and `store-dir` keeps following the active vault for older versions. Debug builds keep their settings in a keyfile next to the dev store, so `cargo run` never changes the installed app's settings.

### 4.3. In-Memory Entry Cache

Name: decrypted entry cache

Type: `HashMap<PathBuf, EntryData>` in `AppState`.

Purpose: Avoids reopening and decrypting a `.gpg` file after an entry has already been opened or successfully saved during the current process lifetime. Optional autoload warms this cache asynchronously at startup.

## 5. External Integrations / APIs

| Integration | Purpose | Method |
| --- | --- | --- |
| GPG / OpenPGP keyring | Encrypt/decrypt password-store entries and discover usable recipients. | GPGME via the `gpgme` crate. |
| Git repository | Track all entry changes and provide push, undo, rollback, and history view. | `git2` crate. |
| Git remote | Optional synchronization with GitHub, GitLab, or custom remotes. | Git remote configured as `origin`; push through git2. |
| pass file format | Compatibility with the standard Unix password manager layout. | Filesystem layout plus `.gpg-id` and encrypted `.gpg` files. |
| GSettings | GNOME-standard preference storage. | `gio::Settings`. |
| KeePassXC / KeePass files | Import entries from `.kdbx` (KDBX 3/4) with the master password. | `keepass` crate (`pass::import::keepass`). |
| Buttercup files | Import entries from `.bcup` (formats a and b, iocane AES-256-CBC+HMAC or GCM, PBKDF2-SHA256). | Own reader following buttercup-core and iocane sources (`pass::import::buttercup`). |
| gtk-markdown widget | Renders multiline fields as styled Markdown blocks. | Published crate `gtk-markdown` from crates.io. |

## 6. Deployment & Infrastructure

Cloud Provider: None. This is a local desktop application.

Build system: Meson orchestrates optional Blueprint compilation, schema compilation, resource bundling, and Cargo builds. When `blueprint-compiler` is unavailable or `-Dblueprint=disabled` is set, Meson uses the checked-in generated `.ui` files.

Runtime artifact: Rust binary plus compiled assets/schemas.

Development run path: The current development workflow runs the Cargo-built binary from `build/target/debug/passel`.

Installation: `meson install -C build` installs the Cargo-built binary, resources, and schemas.

Logging: `env_logger` is initialized in `src/main.rs`; default filter is `warn`. Debug logs are available with `RUST_LOG=debug`.

CI/CD Pipeline: `.github/workflows/packages.yml` builds a Ubuntu (24.04, x86_64) `.deb` and a binary tarball on each published release, and attaches both as release assets. Arch Linux PKGBUILDs live under `packaging/arch/`: `passel/PKGBUILD` builds from the tagged source, `passel-bin/PKGBUILD` consumes the release tarball.

## 7. Security Considerations

Authentication: The application does not implement user login. Access is governed by the local desktop session, file permissions, and the user's GPG agent/keyring.

Authorization: No in-app permission model. The active user can read/write the configured password store if filesystem and GPG permissions allow it.

Data Encryption: Entry files are encrypted with OpenPGP through GPGME. Recipient IDs are read from `.gpg-id`.

Secret Handling:

- Decrypted entries exist in memory while opened and may also exist in the process-local cache.
- Cached entries are not persisted by the app.
- OTP URLs are treated as secrets and can be stored either as the mandatory first field or as custom OTP fields.
- The application should avoid logging plaintext secrets. Current logging should remain structural only.

Git Safety:

- Save/create/delete/rename operations create commits.
- `Undo Action` creates a revert commit.
- `Rollback` creates and pushes a backup branch before hard-resetting and force-pushing the current branch.

## 8. Development & Testing Environment

Local setup is documented in `README.md`.

Primary commands:

```bash
meson setup build
meson compile -C build
cargo test --workspace
cargo check
cargo fmt -- --check
```

Testing strategy:

| Area | Current approach |
| --- | --- |
| Parser | Unit tests in `crates/passel-core/src/helpers/parser.rs` cover scalar, array, multiline, folded block, OTP, and formatting roundtrips. |
| Git helper | Unit tests in `crates/passel-core/src/helpers/git.rs` create temporary repositories and exercise commit, push, rename, delete, revert, rollback, paging, and author time. A local smart-HTTP server checks that a stored token reaches the remote and that a rejected one is not retried. |
| PGP helper | Unit tests cover `.gpg-id` parsing and missing/empty recipient errors. |
| OTP | Unit tests cover RFC-compatible TOTP generation and validation failures. |
| App state/controller | Unit tests cover cache behavior, search, and edit session validation. |

Code quality tools:

- `cargo fmt -- --check`
- `cargo check`
- `cargo test --workspace`
- `meson compile -C build`

## 9. Future Considerations / Roadmap

Known architectural follow-ups:

- Harden install behavior and schema/resource installation paths.
- Improve wording so user-facing labels do not expose Git terminology unnecessarily.
- Improve handling for users without usable GPG keys.
- Consider synchronization backends beyond Git, such as sFTP, rsync, or S3.
- Consider an optional 3-pane layout with separate folder, entry list, and entry detail panes.
- Continue moving static UI structure into Blueprint files where it reduces Rust UI construction code.
- Keep decrypted cache behavior explicit because it trades fewer GPG reads for longer-lived plaintext in memory.

## 10. Project Identification

Project Name: Passel

Repository URL: `https://github.com/Pierrotws/passel`

Application ID: `io.pierrotws.Passel`

Primary Language: Rust

Primary UI Toolkit: GTK4 with libadwaita

Date of Last Update: 2026-04-29

## 11. Glossary / Acronyms

| Term | Meaning |
| --- | --- |
| pass | The standard Unix password manager, which stores one encrypted file per secret. |
| password store | The local directory containing `.gpg` entries, `.gpg-id`, and usually a Git repository. |
| entry | A single password-store `.gpg` file represented as `EntryData`. |
| group | A directory in the password store tree. |
| GPG / OpenPGP | Encryption technology used for password-store files. |
| GPGME | Library used by the app to interact with GPG. |
| OTP | One-time password. This app supports TOTP URLs starting with `otpauth://`. |
| TOTP | Time-based one-time password. |
| GSettings | GNOME preference storage system used for app settings. |
| Blueprint | Declarative GTK UI language compiled into `.ui` templates. |
| GResource | GLib resource bundle containing compiled UI resources. |
