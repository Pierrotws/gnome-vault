# Gnome-Vault — Full Code Review

A pass-compatible password manager: GTK4/Adwaita + Rust, ~7700 LOC, GPGME for crypto, git2 for sync. Architecture is generally sound — clean layer separation (UI → controller → store → helpers), typed errors with `From` impls, well-tested parser/OTP/git helpers, modern `OnceLock` signal patterns. The issues below are the things to fix before shipping.

## Critical (security, data loss, panics)

## Important (correctness, UX, architecture)

**No virtualization in changes/group lists** (`changes_view/mod.rs:43-45`, `group_view`). Hand-built `gtk::Box` per commit + gesture controller scales poorly. Use `gtk::ListView` + `gio::ListStore` + `SignalListItemFactory`.

**OTP detection auto-promotes any `otpauth://`-shaped paste** (`entry_view/mod.rs:383-395`). If a user pastes a TOTP URL into a password field, it silently becomes an OTP field. Require explicit action.

## Minor

- Field-row code duplication: per-row `is_empty`/`key`/`set_key`/`drag_handle` and the copy-button wiring still differ across the four custom rows in template-field name and secondary inspectors; pulling them into a single helper would need a macro that takes `imp.<field_name>` paths. (`delete_button` and the gtk::Entry editable-mode toggle are now centralized in `fields::wire_delete_button` / `fields::set_entry_editable`.)
- Tests: save/delete/revert/rollback orchestration in `AppController` is still uncovered, pgp encrypt/decrypt round-trip is untested, the password generator has no statistical tests, and CRLF / non-UTF8 names are still uncovered. (BOM strip, path-traversal name validation, absolute / parent-dir folder-path rejection, and per-folder `.gpg-id` walk now have unit tests.)

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
