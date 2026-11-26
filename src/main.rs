// Release builds are a GUI program on Windows: no console window
// behind the app. Debug builds keep it for the log.
#![cfg_attr(all(windows, not(debug_assertions)), windows_subsystem = "windows")]

mod helpers;
mod localization;
mod ui;

// The application and store layers live in `passel-core`; re-exported at
// the crate root so UI code keeps using `crate::app::…` / `crate::pass::…`.
// UI messages are translated with `crate::i18n::{gettext, …}`.
use passel_core::{app, i18n, pass};

use std::cell::RefCell;
use std::path::{Path, PathBuf};
use std::rc::Rc;

use adw::prelude::*;
use gtk::gdk;
use gtk::gio;
use gtk::glib;

use crate::app::AppController;
use crate::ui::generate_password_view::GeneratePasswordView;
use crate::ui::window::MainWindow;

const APP_ID: &str = "io.pierrotws.Passel";

fn main() -> glib::ExitCode {
    env_logger::Builder::from_env(env_logger::Env::default().default_filter_or("warn")).init();
    log::debug!("main() start");
    configure_local_schema_dir();
    localization::init();
    if let Some(dir) = pass::store::dev_store_dir() {
        log::info!("Debug build: the first vault defaults to {}", dir.display());
    }

    let resource_path = resource_file();
    let resource = match gio::Resource::load(&resource_path) {
        Ok(res) => res,
        Err(err) => {
            log::error!(
                "Failed to load resources from {}: {err}",
                resource_path.display()
            );
            return glib::ExitCode::FAILURE;
        }
    };
    gio::resources_register(&resource);

    // Register custom widget GTypes that the UI Builder looks up by
    // name when parsing .ui resources. Doing this once here (alongside
    // the resource registration that exposes those .ui files) keeps
    // dialog code self-contained instead of forcing each dialog open
    // path to remember the static_type() ping.
    GeneratePasswordView::static_type();

    let app = adw::Application::builder().application_id(APP_ID).build();

    app.connect_activate(|app| {
        log::debug!("app.activate");
        install_app_css();
        let controller = Rc::new(RefCell::new(AppController::new()));

        let win = MainWindow::new(app, controller);
        log::debug!("window created");

        win.present();
        log::debug!("window presented");
    });

    app.run()
}

fn resource_file() -> PathBuf {
    for path in [
        "assets/resources.gresource",
        "build/assets/resources.gresource",
    ] {
        let resource = Path::new(path);
        if resource.exists() {
            return resource.to_path_buf();
        }
    }

    #[cfg(windows)]
    if let Some(share) = installed_share_dir() {
        return share.join("passel").join("resources.gresource");
    }

    option_env!("RESOURCES_FILE")
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("assets/resources.gresource"))
}

/// The installed `share` folder on Windows. Unix builds are told where
/// their files are at build time (meson's prefix), but a Windows install
/// can be put in any folder: it is found from the program's own place,
/// `<install>\bin\passel.exe` next to `<install>\share`. GLib looks
/// for the settings schemas and icons there the same way.
#[cfg(windows)]
fn installed_share_dir() -> Option<PathBuf> {
    let exe = std::env::current_exe().ok()?;
    Some(exe.parent()?.parent()?.join("share"))
}

/// Loads the small inline stylesheet used by widgets that aren't fully
/// covered by Adwaita's default theme: the unpushed-commit badge on the
/// Changes button, and the entry-pane field styling that keeps
/// read-only and edit mode pixel-identical.
///
/// `field-entry` is the WYSIWYG half of that. Entry-pane fields are
/// frameless in both modes so an editable field is indistinguishable
/// from a read-only one, but Adwaita's `entry.flat` also drops the
/// entry down to `min-height: 0; padding: 2px`. Restoring the framed
/// metrics (without the frame itself) means the widget occupies the
/// same box whether or not it happens to be editable right now, so
/// toggling edit mode moves nothing. Focus still draws Adwaita's
/// accent outline — an outline is painted outside the layout, so it
/// costs no space — plus a faint wash so the field being typed into
/// reads as active.
///
/// `field-key` is the key cell of a plain key/value row: the value's
/// font size, only bold.
///
/// `field-value` is the read-only half of a multiline field. gtk-markdown
/// lays each block out as a `GtkFlowBox` of labels, and Adwaita insets
/// every `flowboxchild` by `margin: 3px; padding: 8px` — 11px of padding
/// on all sides that the editable `GtkTextView` does not have, which
/// offset the rendered text and inflated the block. Zeroing it lines the
/// two up.
///
/// `vault-tree` drops the view background Adwaita gives every `listview`
/// (`listview, list { background-color: var(--view-bg-color) }`), which
/// framed the vault tree as a card floating on the window. The entries
/// list beside it is already transparent by way of
/// `.navigation-sidebar`; this leaves the tree's row and selection
/// styling alone and only takes the panel away.
fn install_app_css() {
    const APP_CSS: &str = "\
        .changes-badge {\
            background-color: @accent_bg_color;\
            color: @accent_fg_color;\
            border-radius: 9999px;\
            padding: 0 5px;\
            font-size: 0.7em;\
            font-weight: bold;\
            min-width: 8px;\
        }\
        entry.field-entry {\
            min-height: 34px;\
            padding: 0 9px;\
            border-radius: 9px;\
            border-spacing: 6px;\
            background-color: transparent;\
        }\
        entry.field-entry:focus-within {\
            background-color: alpha(currentColor, 0.08);\
        }\
        entry.field-key {\
            font-weight: bold;\
        }\
        .field-value flowboxchild {\
            margin: 0;\
            padding: 0;\
        }\
        listview.vault-tree {\
            background-color: transparent;\
        }\
    ";

    let provider = gtk::CssProvider::new();
    provider.load_from_data(APP_CSS);

    let Some(display) = gdk::Display::default() else {
        return;
    };
    gtk::style_context_add_provider_for_display(
        &display,
        &provider,
        gtk::STYLE_PROVIDER_PRIORITY_APPLICATION,
    );
}

fn configure_local_schema_dir() {
    if std::env::var_os("GSETTINGS_SCHEMA_DIR").is_some() {
        return;
    }

    for path in ["assets/schemas", "build/assets/schemas"] {
        if Path::new(path).join("gschemas.compiled").exists() {
            std::env::set_var("GSETTINGS_SCHEMA_DIR", path);
            return;
        }
    }
}
