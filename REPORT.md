# Gnome-Vault — Code Review (round 2)

A pass-compatible password manager: GTK4/Adwaita + Rust, ~8000 LOC, GPGME for crypto, git2 for sync. The previous review's structural issues are largely closed; what's below is a fresh pass that flags issues observed today, grouped by severity.

## Minor

- **`EntryData` / `EntryField` `PartialEq` is non-constant-time.** `src/pass/model/entry_field.rs:7`, `src/pass/model/entry_data.rs:7`. We don't compare against user input today, but if revert-vs-disk diff is ever exposed at a UI boundary the timing channel matters. Consider `subtle::ConstantTimeEq` if/when that comes up.

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
