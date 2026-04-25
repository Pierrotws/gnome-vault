mod app;
mod helpers;
mod pass;
mod ui;

use std::cell::RefCell;
use std::rc::Rc;

use adw::prelude::*;
use gtk::gio;
use gtk::glib;

use crate::app::AppController;
use crate::ui::window::MainWindow;

const APP_ID: &str = "io.pierrotws.GnomeVault";

fn main() -> glib::ExitCode {
    env_logger::Builder::from_env(env_logger::Env::default().default_filter_or("warn")).init();
    log::debug!("main() start");

    let resource = match gio::Resource::load("assets/resources.gresource") {
        Ok(res) => res,
        Err(err) => {
            log::error!("Failed to load resources: {err}");
            return glib::ExitCode::FAILURE;
        }
    };
    gio::resources_register(&resource);

    let app = adw::Application::builder().application_id(APP_ID).build();

    app.connect_activate(|app| {
        log::debug!("app.activate");
        let controller = Rc::new(RefCell::new(AppController::new()));

        let win = MainWindow::new(app, controller);
        log::debug!("window created");

        win.present();
        log::debug!("window presented");
    });

    app.run()
}
