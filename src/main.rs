mod app;
mod helpers;
mod pass;
mod ui;

use std::cell::RefCell;
use std::path::Path;
use std::rc::Rc;

use adw::prelude::*;
use gtk::gdk;
use gtk::gio;
use gtk::glib;

use crate::app::AppController;
use crate::ui::window::MainWindow;

const APP_ID: &str = "io.pierrotws.GnomeVault";

fn main() -> glib::ExitCode {
    env_logger::Builder::from_env(env_logger::Env::default().default_filter_or("warn")).init();
    log::debug!("main() start");
    configure_local_schema_dir();

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

fn resource_file() -> std::path::PathBuf {
    for path in [
        "assets/resources.gresource",
        "build/assets/resources.gresource",
    ] {
        let resource = Path::new(path);
        if resource.exists() {
            return resource.to_path_buf();
        }
    }

    option_env!("RESOURCES_FILE")
        .map(std::path::PathBuf::from)
        .unwrap_or_else(|| std::path::PathBuf::from("assets/resources.gresource"))
}

/// Loads the small inline stylesheet used by widgets that aren't fully
/// covered by Adwaita's default theme — currently the unpushed-commit
/// badge on the Changes button.
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
