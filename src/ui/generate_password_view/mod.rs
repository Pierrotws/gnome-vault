mod imp;

use adw::{prelude::*, subclass::prelude::*};
use gtk::glib;
use zeroize::Zeroizing;

use crate::helpers::password::{generate_password, PasswordMode};

glib::wrapper! {
    pub struct GeneratePasswordView(ObjectSubclass<imp::GeneratePasswordView>)
        @extends adw::Bin, gtk::Widget,
        @implements gtk::Accessible, gtk::Buildable, gtk::ConstraintTarget;
}

impl GeneratePasswordView {
    pub fn new() -> Self {
        glib::Object::new()
    }

    pub fn setup(&self) {
        let imp = self.imp();
        let charset_model =
            gtk::StringList::new(&["Numeric", "Alphanumeric", "Limited specials", "All"]);
        imp.charset_row.set_model(Some(&charset_model));
        imp.charset_row.set_selected(2);
        self.regenerate_preview();
        let this = self.clone();
        imp.reload_button.connect_clicked(move |_| {
            this.regenerate_preview();
        });
        let this = self.clone();
        imp.length_spin.connect_value_changed(move |_| {
            this.regenerate_preview();
        });
        let this = self.clone();
        imp.charset_row.connect_selected_notify(move |_| {
            this.regenerate_preview();
        });
    }

    /// Returns the currently displayed password wrapped in
    /// [`Zeroizing`] so its heap allocation is wiped when the caller
    /// drops it.
    pub fn password(&self) -> Zeroizing<String> {
        Zeroizing::new(self.imp().preview_password_row.text().to_string())
    }

    pub fn regenerate_preview(&self) {
        let imp = self.imp();
        let length = imp.length_spin.value() as usize;
        let mode = match imp.charset_row.selected() {
            0 => PasswordMode::Numeric,
            1 => PasswordMode::Alphanumeric,
            2 => PasswordMode::LimitedSpecial,
            _ => PasswordMode::All,
        };
        match generate_password(length, mode) {
            Ok(password) => imp.preview_password_row.set_text(&password),
            // The spin button range is well above the per-mode minimums
            // and getrandom only fails when the OS RNG is unavailable, so
            // both paths here are exceptional. Surface them as a placeholder
            // string in the preview rather than panicking the app.
            Err(err) => {
                log::error!("password generation failed: {err}");
                imp.preview_password_row.set_text("");
            }
        }
    }
}
