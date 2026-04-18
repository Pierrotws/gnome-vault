mod pass;
mod ui;

use adw::prelude::*;
use gtk::gio;
use gtk::glib;

use crate::ui::window::MainWindow;

const APP_ID: &str = "io.pierrotws.GnomeVault";

fn main() -> glib::ExitCode {
    eprintln!("main() start");
    let resource = match gio::Resource::load("assets/resources.gresource") {
        Ok(res) => res,
        Err(err) => {
            eprintln!("Failed to load resources: {err}");
            return glib::ExitCode::FAILURE;
        }
    };
    gio::resources_register(&resource);

    let app = adw::Application::builder().application_id(APP_ID).build();

    app.connect_activate(|app| {
        eprintln!("app.activate");
        let win = MainWindow::new(app);
        eprintln!("window created");
        win.present();
        eprintln!("window presented");
    });

    app.run()
}
