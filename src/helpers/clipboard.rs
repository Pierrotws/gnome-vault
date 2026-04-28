//! Clipboard helpers that auto-clear copied secrets after a timeout.
//!
//! Mirrors the behavior of `pass -c`: the clipboard is wiped after
//! [`CLEAR_AFTER_SECS`] seconds, but only if its content is still what we
//! wrote, so unrelated copies the user makes meanwhile are preserved.

use gtk::{glib, prelude::*};
use zeroize::Zeroizing;

/// How long secrets remain on the clipboard before being wiped.
pub const CLEAR_AFTER_SECS: u32 = 45;

/// Copies a secret to the default clipboard and schedules an auto-clear.
///
/// The clear is conditional: when the timer fires we re-read the clipboard
/// and only wipe it if the content still matches what we wrote.
pub fn copy_secret(text: &str) {
    let Some(display) = gtk::gdk::Display::default() else {
        return;
    };
    let clipboard = display.clipboard();
    clipboard.set_text(text);

    let expected = Zeroizing::new(text.to_owned());
    glib::timeout_add_seconds_local_once(CLEAR_AFTER_SECS, move || {
        glib::spawn_future_local(async move {
            let Ok(current) = clipboard.read_text_future().await else {
                return;
            };
            if current.as_deref() == Some(expected.as_str()) {
                clipboard.set_text("");
            }
        });
    });
}
