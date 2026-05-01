# Gnome-Vault — Code Review (round 2)

A pass-compatible password manager: GTK4/Adwaita + Rust, ~8000 LOC, GPGME for crypto, git2 for sync. The previous review's structural issues are largely closed; what's below is a fresh pass that flags issues observed today, grouped by severity.

## Critical

## Important

**New-entry dialog accepts unsanitized folder text.** `src/ui/window/new_entry_dialog.rs:91-96`. `PathBuf::from(folder_row.text().trim())` is forwarded straight to the controller; absolute paths and `..` segments rely on `pass::store::validate_folder_path` to reject. That works today, but the dialog could pre-validate and surface a friendlier message before submit. Also the entry name is passed un-trimmed — `create_button` is gated by trimmed-emptiness but the actual create takes the raw `text()`.

## Minor

- **Best-effort cleanup of the local backup ref.** `src/helpers/git.rs:377-382`. When the remote rejects the rollback backup push, the local backup ref deletion is `let _ = …`. A failure leaves a stale `refs/heads/reset/backup/...` that the next rollback's random-suffix path probably won't collide with, but the silent swallow makes diagnosis hard. Log on failure and surface that the local ref persisted.
- **`restore_previous` doesn't re-stage the restored content.** `src/pass/store/mod.rs:346-371`. After `git::add` fails, we put the previous bytes back on disk but don't re-`add` them; HEAD/index can drift from the working tree. Mirror the rename: re-`add` (or `index.remove_path` if the previous was `None`).
- **`save_current_entry` re-borrows `current_session_mut` three times.** `src/app/controller.rs:348-409`. Each phase does `ok_or(NoEntrySelected)?`. If phase 1 has already proven the session existed, later phases should `expect(...)` so a real bug surfaces instead of being masked by a generic error.
- **`delete_folder` evicts the entire cache.** `src/app/controller.rs:317`. Iterate keys and remove only those starting with the deleted prefix.
- **Per-keystroke allocation in `entry_matches_query`.** `src/app/controller.rs:459`. `display_value().to_lowercase()` allocates per array element per filter per character. Pre-lowercase once, or match per element.
- **`valid_entry_name` does not reject `\n` / `\r`.** `src/pass/store/mod.rs:448-464`. Names with newlines disrupt anything line-parsing pass output. Add to the rejection list.
- **`setup_vault` doesn't validate the recipient against `available_recipients()`.** `src/pass/store/mod.rs:107-126`. We trim emptiness and refuse to overwrite, but a typoed recipient lands in `.gpg-id` and only fails on first encrypt — *after* we've committed the init. Validate before writing.
- **`pgp::decrypt` zeroizes invalid-UTF-8 plaintext only after the error materializes.** `src/helpers/pgp.rs:75-81`. `String::from_utf8(plain_vec)` moves the bytes into the `FromUtf8Error`; we then `into_bytes()` and zeroize. Validate with `str::from_utf8(&plain)` first and only own on success.
- **OTP `percent_decode` does lossy UTF-8.** `src/helpers/otp.rs:191`. Replacement chars produce a downstream `InvalidSecret` rather than a clearer "URI is malformed" error. Return `OtpError::InvalidUrl` (or similar) when bytes aren't valid UTF-8.
- **`EntryData` / `EntryField` `PartialEq` is non-constant-time.** `src/pass/model/entry_field.rs:7`, `src/pass/model/entry_data.rs:7`. We don't compare against user input today, but if revert-vs-disk diff is ever exposed at a UI boundary the timing channel matters. Consider `subtle::ConstantTimeEq` if/when that comes up.
- **`AppState::clear_loaded_entry_state` and `AppState::clear_current_entry` overlap.** `src/app/controller.rs:188` vs `src/app/state/app_state.rs:81`. Two methods clear subtly different subsets. Consolidate.
- **`setup_vault` clears cache before `reload_tree` runs.** `src/app/controller.rs:103-110`. If reload fails, the user is left with no cache and no session even though the previous vault is still on disk. Reload first, swap state on success.
- **`find_tree_row_by_path` and `find_tree_position_by_path` duplicate.** `src/ui/vault_view/mod.rs:251-273`. Same loop; collapse to one helper returning `(u32, TreeListRow)`.
- **`ChangeItem::change()` clones unconditionally.** `src/ui/changes_view/change_item.rs:46-52`. Bind reads four fields; a `with_change(|c| …)` accessor would save one `GitChange` clone per recycle.
- **Magic constants.** `src/ui/changes_view/mod.rs:379` (`24.0` scroll threshold), `src/ui/entry_view/mod.rs:429` (`48` title char clamp). Name them.
- **`tree_view_size_index` silently maps unknown nicks to medium.** `src/ui/window/preferences.rs:226-232`. `log::debug!` on the unknown branch so misconfiguration leaves a trace.
- **Two error UI surfaces with no policy.** `entry_view.show_error` (inline label) vs `show_error_dialog` (Adw alert). The inline label is invisible while `content_stack` is on `empty` or `changes`. Pick one, or branch on visible child.
- **`select_node_by_path("")` for top-level entries.** `src/ui/window/mod.rs:344`. `node.path.parent()` is `Some("")` for top-level entries; the search-results "select parent in tree" path then tries to select the synthetic root. Confirm it lands on something visible (and doesn't unselect everything).
- **`set_changes` does not reset `vadjustment`.** `src/ui/changes_view/mod.rs:289-299`. After a fresh `set_changes`, an in-flight scroll near the bottom can re-trigger load-more before the model height re-grows. Reset to top, or guard `should_request_more` against `model.n_items() == 0`.
- **`PASSWORD_STORE_DIR` `env::set_var`.** `src/pass/store/mod.rs:486-498`. `OnceLock` guarantees a single call, and the call sequence is "main thread before workers spawn"; this is safe in practice on edition 2021. Worth documenting that the OnceLock is the safety guarantee, not the OnceCell semantics.

## Positive

- **Per-ref status verification before destructive force-push.** `src/helpers/git.rs:373-398`, `:509-554`. Backup ref acceptance is verified via `push_update_reference` before the destructive reset; the regression test `rollback_does_not_destroy_remote_when_backup_push_fails` is exactly the right shape.
- **Atomic write + best-effort restore on save.** `src/pass/store/mod.rs:323-419`. `.gpg.tmp` + `sync_all` + `rename`, with `previous`-bytes restoration on post-rename failure, covers the partial-failure modes most pass clones get wrong. Both with- and without-previous-content paths are tested.
- **Save phasing and cache invariants are codified and tested.** `src/app/controller.rs:338-410` plus the four `cache_invariant_*` tests. The "cache holds only clean entry data on disk" rule is a kind of invariant that rots silently — keeping it in test code is the right move.
- **Snapshot-before-setter pattern across the window.** `src/ui/window/mod.rs:318-327, 367-376, 425-432, 515-523`. The repeated `let (is_dirty, is_valid) = { let c = controller.borrow(); … };` *before* any `view.set_*` is exactly right for avoiding setter→signal→`borrow_mut` panics, and the inline comments explaining why are excellent.
- **Drag-and-drop now carries the row GObject.** `src/ui/entry_view/mod.rs:557-579`. Dropping uses identity rather than the captured ordinal — the prior off-by-one bug under reorder is gone.
- **`GroupView` activation emits a path string.** `src/ui/group_view/mod.rs:46-51` and `src/ui/group_view/imp.rs:50-55`. Concurrent mutation between emit and handler can no longer mis-resolve `entry-activated`.
- **OTP timer lifecycle.** `src/ui/entry_view/fields/otp_field_row/mod.rs:90-118`. 1 Hz tick is tied to `connect_map` / `connect_unmap` and `dispose` clears `timer_source` — no orphan timers, no wakeups while off-screen.
- **Password entry IM hardening.** `src/ui/entry_view/fields/password_field_row/imp.rs:42-44`. `PRIVATE | NO_EMOJI | NO_SPELLCHECK` keep input methods and spellcheck from logging the password.
- **`generate_password` returns `Zeroizing<String>`.** `src/helpers/password.rs`. The local that holds the cleartext between random-bytes and the GTK widget wipes its allocation on drop.
- **Popovers `set_parent`-ed are correctly `unparent`-ed on close.** `src/ui/vault_view/mod.rs:459-460`, `src/ui/changes_view/mod.rs:276-281`. The leak that was flagged in round 1 is gone.
- **Page-size + 1 lookahead.** `src/ui/window/mod.rs:794-811`. Asking for `PAGE_SIZE + 1` and using `len() > PAGE_SIZE` for `has_more` is the clean fix to the "exactly N changes total" off-by-one.
- **Backup branch name has a random suffix.** `src/helpers/git.rs::backup_branch_name`. Two rollbacks at the same HEAD within the same wall-clock second can no longer collide.
- **Field-row helpers extracted.** `src/ui/entry_view/fields/mod.rs::wire_delete_button` / `set_entry_editable`. The 12-line `glib::clone!` boilerplate that was duplicated four times is now one call site.
- **`load_dir` skips non-UTF-8 paths with a log warning.** `src/pass/store/mod.rs::load_dir`. Replaces the previous lossy round-trip that silently mangled names.

## Suggested fix order

1. The Minor list in any order; most are <10-line changes.
