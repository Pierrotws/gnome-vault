//! A whole vault as one file: a `.tar.xz` of its folder, git history
//! included, for storage that only keeps files (Google Drive). Every
//! entry is under `password-store/`, so `tar xf` on a computer gives a
//! folder that `pass` and Passel open as they are.

use std::{
    fs,
    io::{self, Read, Write},
    path::{Component, Path, PathBuf},
};

use liblzma::{read::XzDecoder, write::XzEncoder};

/// The folder every entry of an archive is in.
const ROOT: &str = "password-store";

/// The most an archive may unpack to: far more than any vault, but a
/// crafted archive cannot fill the device.
const MAX_UNPACKED_BYTES: u64 = 1 << 30;

/// xz preset: the default one (its ~100 MB of encoder memory is fine on
/// a phone, and vaults are small).
const XZ_PRESET: u32 = 6;

/// Writes the vault at `dir` (its regular files and folders; symbolic
/// links are left out) to `out` as a `.tar.xz`.
pub fn pack(dir: &Path, out: &Path) -> io::Result<()> {
    let file = fs::File::create(out)?;
    let mut builder = tar::Builder::new(XzEncoder::new(file, XZ_PRESET));
    builder.mode(tar::HeaderMode::Deterministic);
    builder.follow_symlinks(false);
    append_dir(&mut builder, dir, Path::new(ROOT))?;
    let file = builder.into_inner()?.finish()?;
    file.sync_all()
}

fn append_dir<W: Write>(builder: &mut tar::Builder<W>, dir: &Path, name: &Path) -> io::Result<()> {
    builder.append_dir(name, dir)?;
    let mut children = fs::read_dir(dir)?.collect::<Result<Vec<_>, _>>()?;
    children.sort_by_key(|child| child.file_name());
    for child in children {
        let path = child.path();
        let child_name = name.join(child.file_name());
        let kind = child.file_type()?;
        if kind.is_dir() {
            append_dir(builder, &path, &child_name)?;
        } else if kind.is_file() {
            builder.append_path_with_name(&path, &child_name)?;
        }
    }
    Ok(())
}

/// Unpacks an archive made by [`pack`] into `into` (created; it must not
/// exist yet). Only regular files and folders are written, each inside
/// `into`: links, devices and paths leaving it are refused, as is an
/// archive that unpacks to more than 1 GiB.
pub fn unpack(archive: &Path, into: &Path) -> io::Result<()> {
    fs::create_dir(into)?;
    let file = fs::File::open(archive)?;
    let mut reader = tar::Archive::new(XzDecoder::new(file));
    reader.set_preserve_permissions(false);
    reader.set_overwrite(false);
    let mut total = 0u64;
    for entry in reader.entries()? {
        let mut entry = entry?;
        let Some(relative) = inner_path(&entry.path()?) else {
            return Err(invalid("an archive entry is outside the vault"));
        };
        let target = into.join(&relative);
        match entry.header().entry_type() {
            tar::EntryType::Directory => fs::create_dir_all(&target)?,
            tar::EntryType::Regular | tar::EntryType::Continuous => {
                total = total.saturating_add(entry.header().size()?);
                if total > MAX_UNPACKED_BYTES {
                    return Err(invalid("the archive is too large"));
                }
                if let Some(parent) = target.parent() {
                    fs::create_dir_all(parent)?;
                }
                let mut out = fs::OpenOptions::new()
                    .write(true)
                    .create_new(true)
                    .open(&target)?;
                // Never more than the header announced.
                let size = entry.header().size()?;
                io::copy(&mut (&mut entry).take(size), &mut out)?;
            }
            // pax/GNU extension headers are read by `tar` itself.
            tar::EntryType::XGlobalHeader | tar::EntryType::XHeader => {}
            _ => return Err(invalid("the archive holds something other than files")),
        }
    }
    Ok(())
}

/// `password-store/a/b` → `a/b`; `None` for anything that is not a plain
/// relative path inside the root folder (`..`, absolute paths).
fn inner_path(path: &Path) -> Option<PathBuf> {
    let mut components = path.components().filter(|c| *c != Component::CurDir);
    match components.next()? {
        Component::Normal(root) if root == ROOT => {}
        _ => return None,
    }
    let mut inner = PathBuf::new();
    for component in components {
        match component {
            Component::Normal(part) => inner.push(part),
            _ => return None,
        }
    }
    Some(inner)
}

fn invalid(message: &str) -> io::Error {
    io::Error::new(io::ErrorKind::InvalidData, message)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp_dir(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!(
            "passel-archive-{name}-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[test]
    fn pack_then_unpack_gives_back_the_vault() {
        let base = temp_dir("roundtrip");
        let vault = base.join("vault");
        fs::create_dir_all(vault.join("web/deep")).unwrap();
        fs::create_dir_all(vault.join(".git/refs")).unwrap();
        fs::create_dir_all(vault.join("empty")).unwrap();
        fs::write(vault.join(".gpg-id"), b"ABCD\n").unwrap();
        fs::write(vault.join("web/deep/site.gpg"), [0u8, 1, 2, 255]).unwrap();
        fs::write(vault.join(".git/HEAD"), b"ref: refs/heads/main\n").unwrap();
        #[cfg(unix)]
        std::os::unix::fs::symlink("/etc/passwd", vault.join("link")).unwrap();

        let archive = base.join("vault.tar.xz");
        pack(&vault, &archive).unwrap();
        let out = base.join("out");
        unpack(&archive, &out).unwrap();

        assert_eq!(fs::read(out.join(".gpg-id")).unwrap(), b"ABCD\n");
        assert_eq!(
            fs::read(out.join("web/deep/site.gpg")).unwrap(),
            [0u8, 1, 2, 255]
        );
        assert_eq!(
            fs::read(out.join(".git/HEAD")).unwrap(),
            b"ref: refs/heads/main\n"
        );
        assert!(out.join("empty").is_dir());
        assert!(!out.join("link").exists(), "symbolic links are left out");
        fs::remove_dir_all(base).unwrap();
    }

    #[test]
    fn unpack_refuses_links_and_paths_outside() {
        let base = temp_dir("hostile");
        for (name, kind, path) in [
            ("symlink", tar::EntryType::Symlink, "password-store/evil"),
            ("outside", tar::EntryType::Regular, "elsewhere/file"),
        ] {
            let archive = base.join(format!("{name}.tar.xz"));
            let mut builder =
                tar::Builder::new(XzEncoder::new(fs::File::create(&archive).unwrap(), 1));
            let mut header = tar::Header::new_gnu();
            header.set_entry_type(kind);
            header.set_size(0);
            header.set_mode(0o644);
            if kind == tar::EntryType::Symlink {
                header.set_link_name("/etc").unwrap();
            }
            header.set_path(path).unwrap();
            header.set_cksum();
            builder.append(&header, io::empty()).unwrap();
            builder.into_inner().unwrap().finish().unwrap();

            let out = base.join(format!("{name}-out"));
            let err = unpack(&archive, &out).unwrap_err();
            assert_eq!(err.kind(), io::ErrorKind::InvalidData, "{name}");
        }
        fs::remove_dir_all(base).unwrap();
    }

    #[test]
    fn inner_path_stays_inside_the_root() {
        assert_eq!(
            inner_path(Path::new("password-store/a/b")),
            Some(PathBuf::from("a/b"))
        );
        assert_eq!(
            inner_path(Path::new("./password-store/a")),
            Some(PathBuf::from("a"))
        );
        assert_eq!(
            inner_path(Path::new("password-store")),
            Some(PathBuf::new())
        );
        assert_eq!(inner_path(Path::new("password-store/../x")), None);
        assert_eq!(inner_path(Path::new("/password-store/x")), None);
        assert_eq!(inner_path(Path::new("other/x")), None);
    }
}
