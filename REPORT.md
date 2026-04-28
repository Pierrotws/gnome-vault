# Gnome-Vault — Full Code Review

A pass-compatible password manager: GTK4/Adwaita + Rust, ~7700 LOC, GPGME for crypto, git2 for sync. Architecture is generally sound — clean layer separation (UI → controller → store → helpers), typed errors with `From` impls, well-tested parser/OTP/git helpers, modern `OnceLock` signal patterns. The issues below are the things to fix before shipping.

## Critical (security, data loss, panics)

**Secrets are never zeroized end-to-end.** `EntryData`, `EntryField`, `pgp::decrypt`'s `String`, `password::generate_password`'s return value all live in plain `String`/`Vec<u8>`. Every `.clone()` (and you clone heavily — `controller.rs:194-207` clones twice per cache miss) leaves another copy on the heap. Wrap with `zeroize::Zeroizing`/`secrecy::SecretString` and manually implement `Debug` to print `"<redacted>"`.

**Clipboard secrets never auto-clear** (`entry_view/mod.rs:41-48`, all four field-row copy handlers). Clipboard managers (Klipper, GNOME Shell) keep history forever. `pass -c` defaults to 45s — match it. Centralize a `copy_secret(text, clear_after)` helper.

**OTP shared secret displayed in plaintext `gtk::Entry`** (`otp_field_row/imp.rs:33`). The `otpauth://...?secret=JBSWY...` URL contains the credential. Visible by default, no reveal toggle, "Copy URL" exfiltrates trivially. Use `PasswordEntryRow` or split into Issuer/Account + a reveal-gated secret.

**Path traversal in entry name validation is incomplete** (`pass/store/mod.rs:283-289`). Blocks `/` and `\` but allows `..`, `.`, NUL bytes. Also `validate_folder_path` does no symlink resolution — a symlinked group can write outside the store. Canonicalize and assert `starts_with(store_dir.canonicalize()?)`.

**`.gpg-id` resolution ignores per-folder recipients** (`pass/store/mod.rs:271`). `pgp::recipient_ids(&store_dir)` always reads the root; pass(1) walks from `parent` upward. Result: silently encrypting to the wrong key for entries in subfolders. Walk parents.

**`pgp::encrypt` does not reject empty recipients** (`pgp.rs:72`). Depending on GPGME flags, may produce symmetric-only or unencrypted output. Add a guard at function entry.

**Save is non-atomic and has no rollback path** (`pass/store/mod.rs:265-280`). `fs::write` truncates → if encrypt/write/git step fails, the original entry is destroyed and the repo is left dirty. Write to `.gpg.tmp`, fsync, `rename`. On post-write failure, restore from backup. Distinguish push failure from commit failure.

**Rollback's safety branch isn't verified to land on the remote** (`helpers/git.rs:296-329`). The flow pushes the backup ref, then hard-resets and force-pushes. If the backup push silently failed, you've just permanently lost commits. Capture per-ref status via `RemoteCallbacks::push_update_reference` and refuse the destructive reset unless the backup is confirmed.

**Credential callback can infinite-loop** (`helpers/git.rs:401-411`). libgit2 invokes the callback until it returns `Err` or auth succeeds; returning the same SSH key repeatedly hangs. Add a `Cell<u32>` attempt counter, return `Err` after ~3.

**`getrandom::fill(...).unwrap()` panics the app on RNG failure** (`helpers/password.rs:42`). For a password manager that's a denial-of-service. Propagate the error.

**Save partial-failure leaves cache and session inconsistent** (`app/controller.rs:281-311`). `save_current_entry` renames first, then saves body. If the body save fails: file is renamed on disk, `session.node` points to new path, `mark_saved` not called, old cache entry not removed. Either save body first, or unconditionally fix cache state immediately after a successful rename.

**Sync git network ops run on the UI thread** (`window/mod.rs:639-666` rollback, `1204-1213` push). Freezes the window for seconds-to-minutes with no spinner. Move to `gio::spawn_blocking` + `glib::MainContext::channel`, like the autoload pattern already in this file.

**`std::env::set_var` race** (`main.rs:71`, `window/mod.rs:756, 964`). Not thread-safe on Linux (and `unsafe` in Rust 2024). Once `apply_store_dir_setting` runs after the autoload thread has spawned, you have a data race. Set once at startup.

**Rollback UI has no confirmation dialog** (`changes_view/mod.rs:204-213`). Single click on a popover item destroys remote history. Add `adw::AlertDialog` with destructive response, default-focus on Cancel. Same for revert ("Undo Action").

## Important (correctness, UX, architecture)

**Borrow-then-call-widget-setter pattern is a panic waiting to happen** (`window/mod.rs:246-268, 283-303, 343-346, 541-557, 817-820`). Multiple `controller.borrow()`/`borrow_mut()` calls interleave with widget setters that can synchronously emit signals re-entering the same controller. Snapshot bools first, drop borrows, then update widgets.

**Selecting another entry while editing silently discards changes** (`window/mod.rs:209-216, 541-557`). `connect_group_activated` checks for unsaved state; `connect_entry_selected` doesn't. Mirror the dirty check or prompt with Discard/Cancel/Save.

**OTP timer leaks per row, fires post-dispose, accumulates on rebuild** (`otp_field_row/mod.rs:69-77`). `setup_callbacks` can be called multiple times, each spawns a fresh 1Hz timer. Use `Cell<Option<SourceId>>` in imp; start on `connect_map`, stop on `connect_unmap`; `dispose` should also cancel.

**Strong reference cycles parent ↔ child** (all four field-row mod.rs files: `let parent = entry_view.clone()` captured into row closures). `EntryView` owns the rows, rows hold strong `EntryView` via closures. GObject doesn't break cycles. Use `entry_view.downgrade()`/`upgrade()`.

**Missing `ObjectImpl::dispose` everywhere**. Field rows hold timers, template children, event controllers. Without `dispose` you get GTK warnings on drop and the OTP timer leak above.

**Programmatic `set_text` enters undo/IM history** (`password_field_row/mod.rs:31-33`, `plain_field_row/mod.rs:81-83`). Ctrl+Z reveals previous decrypted entries. IBus/Fcitx may persist preedit. Set `InputPurpose::Password` and `InputHints::PRIVATE | NO_EMOJI | NO_SPELLCHECK` on any entry holding decrypted material.

**No validation on save/create** (`controller.rs:222-237, 281-312`). `is_valid()`/`has_valid_changes()` exists but is only checked in the UI path. Enforce in the controller — never persist invalid data.

**Empty cache check on save failure** (`controller.rs:222-249, 240-249`). `entry_session::is_valid()` rejects empty plain/multiline fields, which are common in real pass entries (e.g. empty `comment:`, empty notes block). Decide if empty is parse-time or save-time refusal and document.

**Non-UTF8 entry names silently mangled** (`pass/store/mod.rs:36, 54-58`). `to_string_lossy()` makes `node.name` no longer match the on-disk filename → subsequent rename/delete fails confusingly. Skip with warning or error explicitly.

**`password_store_dir` panics on missing `HOME`** (`pass/store/mod.rs:309`). Library code should not panic. Return `Result` or fall back via `dirs::home_dir()`.

**`setup_vault` overwrites existing `.gpg-id` without warning** (`pass/store/mod.rs:98-124`). Breaks decryption of every existing entry. Refuse unless caller passes a "reinitialize" flag.

**Autoload thread is unstoppable and uses `mpsc` polling** (`window/mod.rs:1128-1202`). The 100ms polling tick wastes CPU; toggling autoload off in preferences mid-run can't cancel it. Use `glib::MainContext::channel`, store the `SourceId`, expose a cancellation flag.

**`ChangesView::set_changes` infers `has_more` from batch length** (`changes_view/mod.rs:41`). If the first page is exactly the page size *and* there's nothing more, you'll falsely fire `load-more-requested`. Make the parent pass `has_more` explicitly.

**No virtualization in changes/group lists** (`changes_view/mod.rs:43-45`, `group_view`). Hand-built `gtk::Box` per commit + gesture controller scales poorly. Use `gtk::ListView` + `gio::ListStore` + `SignalListItemFactory`.

**Git terminology bleeds into UX** (`changes_view/mod.rs:155, 187-190`). "Push", "Rollback", short SHA in the row body. Already noted in TODO.md. Replace with "Sync to remote", "Discard changes after this point", hide SHA behind details.

**`AppError::Display` may leak secrets** (`app/app_error.rs:25-32`). `StoreError`/`io::Error` Display can include paths (which are entry titles) and gpg stderr. Audit, redact paths to `<path>`, never `log::error!("{err}")` an `AppError::Save`.

**Drag-and-drop reorder uses widget index, not stable** (`entry_view/mod.rs:502-507, 543-581`). If rows mutate mid-drag, wrong row moves. Pass `WeakRef<gtk::ListBoxRow>` instead.

**OTP detection auto-promotes any `otpauth://`-shaped paste** (`entry_view/mod.rs:383-395`). If a user pastes a TOTP URL into a password field, it silently becomes an OTP field. Require explicit action.

**UTF-8 BOM not stripped from first line** (`helpers/parser.rs`). The BOM becomes part of the decoded password silently — passwords mysteriously stop matching. `strip_prefix('\u{feff}')` in `parse_entry`.

**`assert!` on user-driven length in `password::generate_password`** (`helpers/password.rs:62-101`). UI-driven length value crashes the process. Return `Result`.

**`window/mod.rs` is doing seven jobs in 1230 lines.** Extract `setup.rs` (vault wizard), `autoload.rs`, `preferences.rs`, `new_entry_dialog.rs` (or its own composite template). The repeated `if controller.borrow().has_unsaved_changes() { ... return; }` (6 occurrences) → one helper `ensure_clean()`.

**`Paned position_notify` recurses** (`window/mod.rs:111-124`). `set_position` re-emits `notify::position`; only the bounds check saves you from infinite recursion. Block the handler before resetting, or use `>=` with early return.

**Popover leak in row context menu** (`vault_view/mod.rs:299-376`). Each right-click attaches a fresh `gtk::Popover` via `set_parent`; `popdown` doesn't unparent. Repeated right-clicks accumulate popovers. `connect_closed { popover.unparent() }`.

## Minor

- `controller.rs:46-52` `current_entry_mut` exposes raw `&mut EntryData` bypassing `EntrySession` — remove if unused.
- `entry_session.rs:99` `arr.len() == 0` → `arr.is_empty()` (clippy::len_zero).
- `pass/store/mod.rs:236` rename commit message lacks old/new name.
- `pass/store/store_error.rs:46` no `source()` impl — error chain lost.
- `pass/store/mod.rs` stray French comment `// IGNORER les fichiers/dossiers cachés`.
- `pass/model/entry_field.rs:18` unused `'a` lifetime on impl block.
- `pass/model/entry_data.rs:1-2` triple-slash above `use` documents the wrong item.
- `helpers/otp.rs:120-132` `digits > 9` would overflow `10u32.pow(digits)` — bounded by parser today, but add `debug_assert!` to the private helper.
- `helpers/otp.rs:155-184` percent-decode boundary `+2 < len` is `<` not `<=`; trailing truncated escape silently mis-decodes.
- `helpers/git.rs:441-448` `backup_branch_name` truncates timestamp; collisions on broken clocks. Append random suffix.
- `helpers/git.rs:158-184` `commit` fails cryptically without `user.name`/`user.email` — translate to `GitError::NoSignature`.
- `helpers/pgp.rs:101-119` recipient labels can contain control chars / RTL overrides — sanitize before display.
- `helpers/macros.rs` recursion limit fine for 4 args; document.
- `app_error.rs:7` missing `#[derive(Debug)]`. Consider `thiserror`.
- `controller.rs:130-137` warmer's `cache_loaded_entry(&mut self)` requires `borrow_mut()` while UI may hold `borrow()` — document the contract or `try_borrow_mut`.
- `vault_view/mod.rs:104-110` `set_mode_simple/split`/`use_default_factory` are dead.
- `vault_view/mod.rs:188-204` `set_model` duplicates `set_selection_model`.
- `changes_view/mod.rs:23-31` non-weak `self.clone()` into adjustment closure (cycle).
- `generate_password_view/mod.rs:24` magic `set_selected(2)` — name the constant.
- `group_view::set_group` rebuilds even when called with the same group; `entry-activated` uses fragile `row.index()` instead of node identity.
- Field-row code duplication: identical 8-line `delete_button` boilerplate in 4 places; identical `set_entry_editable_mode`; identical `is_empty/key/set_key/drag_handle`. Extend the `EntryFieldRow` trait, or write a `field_row!` macro.
- A11y: copy/delete buttons rely on tooltips alone; set `accessible-name` explicitly. `title_entry` has no `activates-default`/`connect_activate` for Enter-to-save.
- Tests: save/delete/revert/rollback paths in controller are uncovered; pgp encrypt/decrypt round-trip is untested; password generator has no statistical tests; no test for path traversal, per-folder `.gpg-id`, BOM, CRLF, non-UTF8 names. The save/rename atomicity bug (critical) lives in untested code.

## Done well

- Clean layer separation: UI → controller → store; UI never touches files.
- `EntrySession` correctly tracks `original`/`current`/`name` for revert + dirty.
- `pgp.rs` / `git.rs` / `parser.rs` / `otp.rs` have **proper unit tests** including RFC vectors, parser round-trips, git temp-repo lifecycles.
- Error types are typed (no stringly-typed errors), with `From` impls giving clean `?` ergonomics.
- `password::random_index` does textbook-correct rejection sampling — no modulo bias.
- `OnceLock<Vec<Signal>>` and typed `connect_*` wrappers on subclassed widgets are idiomatic.
- `BoxedAnyObject` for non-GObject `PassNode` is the right call.
- `SignalListItemFactory` setup/bind split is correct.
- Autoload uses the worker-thread + main-context-poll pattern (just polish per Important section).

## Suggested fix order

1. Audit secrets: zeroize + redacted Debug + clipboard auto-clear + OTP secret hidden by default.
2. Fix the rename-then-save partial-failure window and adopt atomic write+rename in `write_entry_data`.
3. Verify backup ref accepted before destructive reset; cap credential callback attempts.
4. Move all git network ops off the UI thread; add rollback/revert confirmation dialogs.
5. Fix the borrow-then-widget-setter pattern (snapshot bools, drop borrows).
6. Tie OTP timer lifetime to `connect_map`/`connect_unmap`; fix parent↔child cycles with `downgrade`.
7. Add `ObjectImpl::dispose` to every imp.
8. Path traversal hardening + per-folder `.gpg-id` resolution.
9. Validation enforced in controller for save/create.
10. Test the save/delete/revert/rollback paths.
11. Extract sub-files from `window/mod.rs` (setup, autoload, preferences, new-entry).
12. UX: scrub Git terminology; add cancellation/spinners; virtualize changes list.
