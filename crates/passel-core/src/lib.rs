//! Passel's password-store logic, with no UI toolkit.
//!
//! The GTK app (the `passel` crate at the repository root) and any future
//! front end (mobile bindings, a CLI) sit on top of this crate. It must
//! never depend on GTK, GLib or any other UI library: CI builds it on its
//! own to keep it that way.
//!
//! - [`pass`]: the password-store domain (entries, nodes) and the store
//!   facade that reads and writes a vault folder.
//! - [`helpers`]: infrastructure the store builds on (OpenPGP, git,
//!   the `pass` plaintext format, OTP, password generation).
//! - [`app`]: the application layer a UI drives (open entry, edit
//!   session, decrypted cache, vault list). Its functions are blocking:
//!   each front end decides which thread runs them.

pub mod app;
pub mod helpers;
pub mod pass;
