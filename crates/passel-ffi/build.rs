//! Compiles the translations (`po/<lang>.po` for each language in
//! `po/LINGUAS`) with msgfmt and embeds them: a phone has no locale
//! directory to install them in. See `src/i18n.rs`.

use std::env;
use std::fmt::Write as _;
use std::fs;
use std::path::PathBuf;
use std::process::Command;

fn main() {
    let po_dir = PathBuf::from(env::var("CARGO_MANIFEST_DIR").unwrap()).join("../../po");
    let out_dir = PathBuf::from(env::var("OUT_DIR").unwrap());
    let linguas = po_dir.join("LINGUAS");
    println!("cargo:rerun-if-changed={}", linguas.display());

    let mut table = String::from("pub(crate) static CATALOGS: &[(&str, &[u8])] = &[\n");
    for lang in fs::read_to_string(&linguas)
        .expect("po/LINGUAS")
        .lines()
        .map(str::trim)
        .filter(|l| !l.is_empty() && !l.starts_with('#'))
    {
        let po = po_dir.join(format!("{lang}.po"));
        let mo = out_dir.join(format!("{lang}.mo"));
        println!("cargo:rerun-if-changed={}", po.display());
        let status = Command::new("msgfmt")
            .arg("--check")
            .arg("--output-file")
            .arg(&mo)
            .arg(&po)
            .status()
            .unwrap_or_else(|err| {
                panic!("msgfmt (GNU gettext) is needed to compile the translations: {err}")
            });
        assert!(status.success(), "msgfmt failed on {}", po.display());
        writeln!(table, "    ({lang:?}, include_bytes!({:?})),", mo.display()).unwrap();
    }
    table.push_str("];\n");
    fs::write(out_dir.join("catalogs.rs"), table).unwrap();
}
