//! Picks the user's language at startup.
//!
//! Two readers share the same `passel.mo` files: libintl translates the
//! `.ui` templates (GtkBuilder looks their strings up in the `passel`
//! domain they declare), and [`passel_core::i18n`] translates every
//! message built in Rust, ours and passel-core's. Both follow the
//! LANGUAGE / LC_ALL / LC_MESSAGES / LANG environment.

use std::ffi::{c_char, CString};
use std::path::{Path, PathBuf};

use gtk::glib;

use crate::i18n;

#[cfg(not(windows))]
extern "C" {
    // libintl, part of glibc.
    fn bindtextdomain(domainname: *const c_char, dirname: *const c_char) -> *mut c_char;
    fn bind_textdomain_codeset(domainname: *const c_char, codeset: *const c_char) -> *mut c_char;
}

// GNU libintl's own DLL on Windows, the one GLib uses. Its exported names
// are the `libintl_` ones its header maps the usual names to; the wide
// variant takes the folder as UTF-16, so a profile folder outside the
// ANSI code page still works.
#[cfg(windows)]
#[link(name = "intl")]
extern "C" {
    fn libintl_wbindtextdomain(domainname: *const c_char, wdirname: *const u16) -> *mut u16;
    fn libintl_bind_textdomain_codeset(
        domainname: *const c_char,
        codeset: *const c_char,
    ) -> *mut c_char;
}

pub fn init() {
    let localedir = locale_dir();
    if !bind_domain(&localedir) {
        log::warn!("Locale directory {} is unusable", localedir.display());
    }

    let languages: Vec<String> = glib::language_names()
        .iter()
        .map(|name| name.to_string())
        .collect();
    match i18n::install_from_dir(&localedir, &languages) {
        Some(lang) => log::debug!("Translations: {lang} from {}", localedir.display()),
        None => log::debug!("Translations: English (wanted {languages:?})"),
    }
}

/// Points libintl at `localedir` for the `.ui` translations.
#[cfg(not(windows))]
fn bind_domain(localedir: &Path) -> bool {
    let (Ok(domain), Ok(dir)) = (
        CString::new(i18n::DOMAIN),
        CString::new(localedir.as_os_str().as_encoded_bytes()),
    ) else {
        return false;
    };
    // SAFETY: both are valid NUL-terminated strings; libintl copies them.
    unsafe {
        bindtextdomain(domain.as_ptr(), dir.as_ptr());
        bind_textdomain_codeset(domain.as_ptr(), c"UTF-8".as_ptr());
    }
    true
}

/// Points libintl at `localedir` for the `.ui` translations.
#[cfg(windows)]
fn bind_domain(localedir: &Path) -> bool {
    use std::os::windows::ffi::OsStrExt;
    let Ok(domain) = CString::new(i18n::DOMAIN) else {
        return false;
    };
    let dir: Vec<u16> = localedir.as_os_str().encode_wide().chain([0]).collect();
    if dir[..dir.len() - 1].contains(&0) {
        return false;
    }
    // SAFETY: `domain` is NUL-terminated, `dir` is NUL-terminated UTF-16
    // with no NUL inside; libintl copies both.
    unsafe {
        libintl_wbindtextdomain(domain.as_ptr(), dir.as_ptr());
        libintl_bind_textdomain_codeset(domain.as_ptr(), c"UTF-8".as_ptr());
    }
    true
}

/// Where the compiled catalogs are: the build dir's `po/` when running
/// from a checkout (meson compiles them there in the installed layout),
/// otherwise the install prefix's, given by meson at build time (found
/// next to the program on Windows, see [`crate::installed_share_dir`]).
fn locale_dir() -> PathBuf {
    if let Some(dir) = std::env::var_os("PASSEL_LOCALEDIR") {
        return dir.into();
    }
    // From the repository root or from inside `build/`, like the
    // resources (main.rs).
    if cfg!(debug_assertions) {
        for candidate in ["build/po", "po"] {
            if has_catalogs(Path::new(candidate)) {
                if let Ok(dir) = Path::new(candidate).canonicalize() {
                    return dir;
                }
            }
        }
    }
    #[cfg(windows)]
    if let Some(share) = crate::installed_share_dir() {
        return share.join("locale");
    }
    PathBuf::from(option_env!("LOCALEDIR").unwrap_or("/usr/share/locale"))
}

/// Whether `dir` holds compiled catalogs (`<lang>/LC_MESSAGES/passel.mo`),
/// not only `.po` sources.
fn has_catalogs(dir: &Path) -> bool {
    std::fs::read_dir(dir)
        .into_iter()
        .flatten()
        .flatten()
        .any(|lang| {
            lang.path()
                .join("LC_MESSAGES")
                .join(format!("{}.mo", i18n::DOMAIN))
                .is_file()
        })
}
