mod pass_entry;
mod pass_store;
mod window;

use adw::prelude::*;
use gtk::prelude::*;
use gtk::gio;
use gtk::glib;

use window::MainWindow;

const APP_ID: &str = "io.pierrotws.GnomeVault";

fn main() -> glib::ExitCode {
    eprintln!("main() start");
    gio::resources_register_include!("compiled.gresource")
        .expect("Failed to register resources");

    let app = adw::Application::builder()
        .application_id(APP_ID)
        .build();

    app.connect_activate(|app| {
        eprintln!("app.activate");
        let win = MainWindow::new(app);
        eprintln!("window created");
        win.present();
        eprintln!("window presented");
    });

    app.run()
}
