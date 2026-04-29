# Architecture Overview

This document follows the structure from [architecture.md](https://architecture.md/) and is intended as a living map of the codebase. Update it when module boundaries, storage behavior, integrations, or build/deployment assumptions change.

## 1. Project Structure

```text
gnome-vault/
|-- assets/
|   |-- resources.gresource.xml        # GTK resource bundle manifest
|   |-- schemas/                       # GSettings schema and schema build rules
|   `-- ui/                            # Blueprint UI sources and generated .ui fallbacks
|       `-- fields/                    # Entry field row UI sources and generated fallbacks
|-- src/
|   |-- main.rs                        # Application startup, resources, schema lookup
|   |-- app/                           # UI-facing application controller and state
|   |   |-- controller.rs              # Main application use-case API
|   |   |-- app_error.rs               # App-level error translation
|   |   |-- cache_warmup.rs            # Background entry-cache decryption worker
|   |   |-- changes.rs                 # Worker-safe wrappers for revert/rollback/push
|   |   |-- group_preview.rs           # Worker-side decrypt for group-row subtitles
|   |   `-- state/                     # Tree, selected entry, edit session, entry cache
|   |-- helpers/                       # Focused infrastructure/domain helpers
|   |   |-- clipboard.rs               # Centralized "copy secret" entry point
|   |   |-- entry_preview.rs           # Subtitle formatting for group rows
|   |   |-- git.rs                     # Git repository operations through git2
|   |   |-- macros.rs                  # Internal macros
|   |   |-- otp.rs                     # TOTP parsing and generation
|   |   |-- parser.rs                  # pass plaintext <-> EntryData parser/formatter
|   |   |-- password.rs                # Password generation
|   |   `-- pgp.rs                     # GPGME encryption/decryption and recipients
|   |-- pass/                          # password-store domain and persistence boundary
|   |   |-- model/                     # EntryData, EntryField, PassNode
|   |   `-- store/                     # File, GPG, and Git-backed store operations
|   `-- ui/                            # GTK/Adwaita widgets and templates
|       |-- window/                    # Main window orchestration
|       |   |-- mod.rs                 # MainWindow wiring, callbacks, edit-mode glue
|       |   |-- imp.rs                 # Composite-template fields and ObjectSubclass
|       |   |-- autoload.rs            # Background cache-warmup tick handler
|       |   |-- group_content.rs       # Right-pane group/search-results rendering
|       |   |-- new_entry_dialog.rs    # New-entry creation dialog
|       |   |-- preferences.rs         # Preferences dialog wiring
|       |   `-- setup.rs               # Vault-setup view and env propagation
|       |-- vault_view/                # Vault tree, search, programmatic selection
|       |-- group_view/                # Right-pane list of entries in a group
|       |-- entry_view/                # Entry display/editing and field rows
|       |-- changes_view/              # Git history and change actions
|       `-- generate_password_view/    # Password generator widget
|-- vendor/
|   `-- gtk-markdown/                  # Markdown-rendering GTK widget (git submodule)
|-- .gitmodules                        # Submodule pin for vendor/gtk-markdown
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
[AppController in src/app/controller.rs]
  |
  +--> [AppState: selected node, edit session, decrypted cache]
  |
  v
[pass::store facade]
  |
  +--> [helpers::parser] <-> pass plaintext format
  +--> [helpers::pgp]    <-> GPGME / local OpenPGP keyring
  +--> [helpers::git]    <-> git2 repository and remote
  |
  v
[PASSWORD_STORE_DIR or ~/.password-store]
```

The UI never writes password-store files directly. Widgets emit signals, `MainWindow` translates them into controller calls, and the controller delegates persistence to `pass::store`.

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
| `src/ui/window/mod.rs` | Main UI orchestration, signal wiring, edit-mode state. |
| `src/ui/window/setup.rs` | Vault-setup wizard and `PASSWORD_STORE_DIR` propagation. |
| `src/ui/window/preferences.rs` | Preferences dialog (autopush, autoload, group view, store dir, branch). |
| `src/ui/window/autoload.rs` | Background entry-cache warmup tick handler. |
| `src/ui/window/group_content.rs` | Right-pane rendering for group browsing and flat search results. |
| `src/ui/window/new_entry_dialog.rs` | New-entry creation dialog. |
| `assets/ui/window.blp` | Main window layout and stack structure. |
| `src/ui/entry_view/` | Entry display/editing and field row integration. |
| `src/ui/vault_view/` | Vault tree rendering, selection, search, and context actions. |
| `src/ui/group_view/` | Right-pane list of entries inside the selected group. |
| `src/ui/changes_view/` | Change list rendering, lazy history paging, and change context actions. |
| `vendor/gtk-markdown/` | Standalone GTK widget that renders multiline fields as Markdown. |

### 3.2. Application Controller and State

Name: Application layer

Description: Provides a narrow API for the UI. It owns in-memory application state, enforces edit/session rules, and translates UI actions into domain/store operations.

Technologies: Plain Rust structs with GTK-facing ownership handled by `Rc<RefCell<AppController>>`.

Primary files:

| File | Responsibility |
| --- | --- |
| `src/app/controller.rs` | Use-case methods for loading trees, opening entries, saving, creating, deleting, renaming, search filter, and cache warmup. |
| `src/app/changes.rs` | Worker-safe free functions for revert / rollback / push (called from `gio::spawn_blocking`). |
| `src/app/cache_warmup.rs` | Spawns the autoload decrypt worker and streams results back to the main loop. |
| `src/app/group_preview.rs` | Spawns the per-group decrypt worker that fills missing subtitle previews. |
| `src/app/state/app_state.rs` | Current tree, selected node, current edit session, decrypted entry cache. |
| `src/app/state/entry_session.rs` | Dirty tracking, validation, title changes, save/revert session state. |
| `src/app/app_error.rs` | Error type surfaced to the UI. |

### 3.3. Password Store Domain

Name: pass model and store facade

Description: Represents password-store entries and nodes, and exposes filesystem/GPG/Git-backed operations to the controller.

Technologies: Rust domain structs, standard filesystem API, GPGME, git2.

Primary files:

| File | Responsibility |
| --- | --- |
| `src/pass/model/entry_data.rs` | Complete entry model: mandatory first field plus custom fields. |
| `src/pass/model/entry_field.rs` | Typed entry values: password, plain, OTP, array, multiline. |
| `src/pass/model/pass_node.rs` | Vault tree nodes for groups and `.gpg` entries. |
| `src/pass/store/mod.rs` | Store facade: load, setup, save, create, rename, delete, history, push, revert, rollback. |
| `src/pass/store/store_error.rs` | Store-level error type. |

### 3.4. Infrastructure Helpers

Name: helpers

Description: Small modules for implementation-specific concerns that should not pollute UI or state code.

Technologies: git2, gpgme, HMAC-SHA1, Base32, standard Rust.

Primary files:

| File | Responsibility |
| --- | --- |
| `src/helpers/parser.rs` | Parses and formats the pass-compatible YAML subset for custom fields. |
| `src/helpers/pgp.rs` | Decrypts/encrypts `.gpg` files and lists usable local recipients. |
| `src/helpers/git.rs` | Repository initialization, add/remove/rename, commit, push, history, revert, rollback. |
| `src/helpers/otp.rs` | Validates `otpauth://` URLs and generates current TOTP values. |
| `src/helpers/password.rs` | Generates passwords for the new-entry dialog. |
| `src/helpers/clipboard.rs` | Single entry point for copying secrets into the GTK clipboard. |
| `src/helpers/entry_preview.rs` | Formats the first-field preview shown as a group-row subtitle. |

## 4. Data Stores

### 4.1. Password Store Repository

Name: password store

Type: Local Git repository containing GPG-encrypted files.

Location: `PASSWORD_STORE_DIR` when set, otherwise `~/.password-store`.

Purpose: Stores user secrets as pass-compatible `.gpg` files. Directories are represented as groups; `.gpg` files are represented as entries.

Important files:

| Path | Purpose |
| --- | --- |
| `*.gpg` | Encrypted entry payload. |
| `.gpg-id` | Recipient key IDs used when encrypting entries. |
| `.git/` | Change history and synchronization state. |

### 4.2. GSettings

Name: application preferences

Type: GLib GSettings schema.

Location: `assets/schemas/io.pierrotws.GnomeVault.gschema.xml`.

Purpose: Stores application preferences such as automatic push behavior, startup cache autoload, and configured store directory.

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
| gtk-markdown widget | Renders multiline fields as styled Markdown blocks. | Vendored as a git submodule at `vendor/gtk-markdown`; consumed via Cargo `path` dependency. |

## 6. Deployment & Infrastructure

Cloud Provider: None. This is a local desktop application.

Build system: Meson orchestrates optional Blueprint compilation, schema compilation, resource bundling, and Cargo builds. When `blueprint-compiler` is unavailable or `-Dblueprint=disabled` is set, Meson uses the checked-in generated `.ui` files.

Runtime artifact: Rust binary plus compiled assets/schemas.

Development run path: The current development workflow runs the Cargo-built binary from `build/target/debug/gnome-vault`.

Installation: `meson install -C build` installs the Cargo-built binary, resources, and schemas.

Logging: `env_logger` is initialized in `src/main.rs`; default filter is `warn`. Debug logs are available with `RUST_LOG=debug`.

CI/CD Pipeline: Not currently defined in the repository.

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
cargo test
cargo check
cargo fmt -- --check
```

Testing strategy:

| Area | Current approach |
| --- | --- |
| Parser | Unit tests in `src/helpers/parser.rs` cover scalar, array, multiline, folded block, OTP, and formatting roundtrips. |
| Git helper | Unit tests in `src/helpers/git.rs` create temporary repositories and exercise commit, push, rename, delete, revert, rollback, paging, and author time. |
| PGP helper | Unit tests cover `.gpg-id` parsing and missing/empty recipient errors. |
| OTP | Unit tests cover RFC-compatible TOTP generation and validation failures. |
| App state/controller | Unit tests cover cache behavior, search, and edit session validation. |

Code quality tools:

- `cargo fmt -- --check`
- `cargo check`
- `cargo test`
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

Project Name: Gnome Vault

Repository URL: `https://github.com/Pierrotws/gnome-vault`

Application ID: `io.pierrotws.GnomeVault`

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
