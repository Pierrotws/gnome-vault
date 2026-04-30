# Gnome-Vault — Full Code Review

A pass-compatible password manager: GTK4/Adwaita + Rust, ~7700 LOC, GPGME for crypto, git2 for sync. Architecture is generally sound — clean layer separation (UI → controller → store → helpers), typed errors with `From` impls, well-tested parser/OTP/git helpers, modern `OnceLock` signal patterns. The issues below are the things to fix before shipping.

## Critical (security, data loss, panics)

**Clipboard secrets never auto-clear** (`entry_view/mod.rs:41-48`, all four field-row copy handlers). Clipboard managers (Klipper, GNOME Shell) keep history forever. `pass -c` defaults to 45s — match it. Centralize a `copy_secret(text, clear_after)` helper.

**OTP shared secret displayed in plaintext `gtk::Entry`** (`otp_field_row/imp.rs:33`). The `otpauth://...?secret=JBSWY...` URL contains the credential. Visible by default, no reveal toggle, "Copy URL" exfiltrates trivially. Use `PasswordEntryRow` or split into Issuer/Account + a reveal-gated secret.

**Rollback's safety branch isn't verified to land on the remote** (`helpers/git.rs:296-329`). The flow pushes the backup ref, then hard-resets and force-pushes. If the backup push silently failed, you've just permanently lost commits. Capture per-ref status via `RemoteCallbacks::push_update_reference` and refuse the destructive reset unless the backup is confirmed.

**`std::env::set_var` race** (`main.rs:71`, `window/mod.rs:756, 964`). Not thread-safe on Linux (and `unsafe` in Rust 2024). Once `apply_store_dir_setting` runs after the autoload thread has spawned, you have a data race. Set once at startup.

## Important (correctness, UX, architecture)

**Borrow-then-call-widget-setter pattern is a panic waiting to happen** (`window/mod.rs:246-268, 283-303, 343-346, 541-557, 817-820`). Multiple `controller.borrow()`/`borrow_mut()` calls interleave with widget setters that can synchronously emit signals re-entering the same controller. Snapshot bools first, drop borrows, then update widgets.

**Empty cache check on save failure** (`controller.rs:222-249, 240-249`). `entry_session::is_valid()` rejects empty plain/multiline fields, which are common in real pass entries (e.g. empty `comment:`, empty notes block). Decide if empty is parse-time or save-time refusal and document.

**Autoload thread is unstoppable and uses `mpsc` polling** (`window/mod.rs:1128-1202`). The 100ms polling tick wastes CPU; toggling autoload off in preferences mid-run can't cancel it. Use `glib::MainContext::channel`, store the `SourceId`, expose a cancellation flag.

**No virtualization in changes/group lists** (`changes_view/mod.rs:43-45`, `group_view`). Hand-built `gtk::Box` per commit + gesture controller scales poorly. Use `gtk::ListView` + `gio::ListStore` + `SignalListItemFactory`.

**Git terminology bleeds into UX** (`changes_view/mod.rs:155, 187-190`). "Push", "Rollback", short SHA in the row body. Already noted in TODO.md. Replace with "Sync to remote", "Discard changes after this point", hide SHA behind details.

**`AppError::Display` may leak secrets** (`app/app_error.rs:25-32`). `StoreError`/`io::Error` Display can include paths (which are entry titles) and gpg stderr. Audit, redact paths to `<path>`, never `log::error!("{err}")` an `AppError::Save`.

**Drag-and-drop reorder uses widget index, not stable** (`entry_view/mod.rs:502-507, 543-581`). If rows mutate mid-drag, wrong row moves. Pass `WeakRef<gtk::ListBoxRow>` instead.

**OTP detection auto-promotes any `otpauth://`-shaped paste** (`entry_view/mod.rs:383-395`). If a user pastes a TOTP URL into a password field, it silently becomes an OTP field. Require explicit action.

## Minor

- `helpers/pgp.rs:101-119` recipient labels can contain control chars / RTL overrides — sanitize before display.
- `helpers/macros.rs` recursion limit fine for 4 args; document.
- `controller.rs:130-137` warmer's `cache_loaded_entry(&mut self)` requires `borrow_mut()` while UI may hold `borrow()` — document the contract or `try_borrow_mut`.
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
