use std::process::Command;

fn main() {
    println!("cargo:rerun-if-changed=ui/window.blp");
    println!("cargo:rerun-if-changed=ui/resources.gresource.xml");

    let status = Command::new("blueprint-compiler")
        .args(["compile", "ui/window.blp"])
        .output()
        .expect("failed to run blueprint-compiler");

    if !status.status.success() {
        panic!(
            "blueprint-compiler failed:\n{}",
            String::from_utf8_lossy(&status.stderr)
        );
    }

    std::fs::write("ui/window.ui", status.stdout)
        .expect("failed to write ui/window.ui");

    glib_build_tools::compile_resources(
        &["ui"],
        "ui/resources.gresource.xml",
        "compiled.gresource",
    );
}
