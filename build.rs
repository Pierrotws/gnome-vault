//! Windows only: links the icon and version details into passel.exe
//! (packaging/windows/passel.rc). Nothing to do elsewhere: Linux desktops
//! find the icon by the application id.

use std::{env, path::Path};

fn main() {
    println!("cargo:rerun-if-changed=build.rs");
    // The target's OS, not the one running this script (cross builds).
    if env::var("CARGO_CFG_TARGET_OS").as_deref() != Ok("windows") {
        return;
    }
    let rc = "packaging/windows/passel.rc";
    let icon = Path::new(&env::var("CARGO_MANIFEST_DIR").unwrap()).join("assets/icons/passel.ico");
    println!("cargo:rerun-if-changed={rc}");
    println!("cargo:rerun-if-changed={}", icon.display());

    let version = env::var("CARGO_PKG_VERSION").unwrap();
    let part = |name| env::var(name).unwrap();
    // Forward slashes: the resource compiler reads backslashes in a
    // string as escapes.
    let icon = icon.to_string_lossy().replace('\\', "/");
    let macros = [
        format!("PASSEL_VERSION=\\\"{version}\\\""),
        format!("PASSEL_VERSION_MAJOR={}", part("CARGO_PKG_VERSION_MAJOR")),
        format!("PASSEL_VERSION_MINOR={}", part("CARGO_PKG_VERSION_MINOR")),
        format!("PASSEL_VERSION_PATCH={}", part("CARGO_PKG_VERSION_PATCH")),
        format!("PASSEL_ICON=\\\"{icon}\\\""),
    ];
    embed_resource::compile(rc, &macros)
        .manifest_optional()
        .unwrap();
}
