//! A vault's history as one file: a git bundle (`git bundle`'s v2
//! format), for storage that only keeps files (Google Drive, a file on
//! a USB drive or in a synced folder). Only the commits and their
//! objects travel, never a repository's config or hooks, and
//! `git clone vault.bundle` on a computer gives back the vault.
//!
//! libgit2 has no bundle support: the header is written and read here,
//! the pack after it by libgit2's pack builder and pack writer.

use std::{
    fs,
    io::{self, BufRead, BufReader, Read, Write},
    path::Path,
};

use git2::{Oid, Repository};

const SIGNATURE_V2: &str = "# v2 git bundle";
const SIGNATURE_V3: &str = "# v3 git bundle";

/// The most a bundle's pack may hold: far more than any vault, but a
/// crafted file cannot fill the device.
const MAX_PACK_BYTES: u64 = 1 << 30;

/// The most a bundle's header may take, and the most refs it may list.
const MAX_HEADER_BYTES: u64 = 64 << 10;
const MAX_REFS: usize = 1000;

/// What a bundle file announces, before its pack.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Header {
    /// `(commit, ref name)` pairs, `HEAD` included when listed.
    pub refs: Vec<(Oid, String)>,
    /// Where the pack starts in the file.
    pack_offset: u64,
}

impl Header {
    /// The branch to take from the bundle and its commit: `preferred`
    /// when the bundle has it, else the branch `HEAD` points at, else
    /// its first branch.
    pub fn branch(&self, preferred: Option<&str>) -> Option<(String, Oid)> {
        let branches = || {
            self.refs.iter().filter_map(|(oid, name)| {
                name.strip_prefix("refs/heads/")
                    .map(|branch| (branch.to_string(), *oid))
            })
        };
        if let Some(found) = preferred.and_then(|wanted| branches().find(|(b, _)| b == wanted)) {
            return Some(found);
        }
        let head = self
            .refs
            .iter()
            .find(|(_, name)| name == "HEAD")
            .map(|(oid, _)| *oid);
        head.and_then(|head| branches().find(|(_, oid)| *oid == head))
            .or_else(|| branches().next())
    }
}

/// Writes the history of `branch` (at `tip`) to `out` as a bundle; `HEAD`
/// names that branch, so `git clone` checks it out.
pub fn write(repo: &Repository, branch: &str, tip: Oid, out: &Path) -> Result<(), git2::Error> {
    let mut walk = repo.revwalk()?;
    walk.push(tip)?;
    let mut builder = repo.packbuilder()?;
    builder.insert_walk(&mut walk)?;
    let mut pack = git2::Buf::new();
    builder.write_buf(&mut pack)?;

    let write = || -> io::Result<()> {
        let mut file = fs::File::create(out)?;
        write!(
            file,
            "{SIGNATURE_V2}\n{tip} refs/heads/{branch}\n{tip} HEAD\n\n"
        )?;
        file.write_all(&pack)?;
        file.sync_all()
    };
    write().map_err(|err| git2::Error::from_str(&format!("writing a bundle: {}", err.kind())))
}

/// Reads the header of the bundle at `path`. Anything but a complete
/// bundle (no prerequisites: this vault's history, not a part of it)
/// is [`io::ErrorKind::InvalidData`].
pub fn read_header(path: &Path) -> io::Result<Header> {
    let file = fs::File::open(path)?;
    let mut reader = BufReader::new(file.take(MAX_HEADER_BYTES));
    let mut line = String::new();
    let mut read_line = |line: &mut String| -> io::Result<usize> {
        line.clear();
        let read = reader.read_line(line)?;
        if read > 0 && !line.ends_with('\n') {
            return Err(invalid("a header line does not end"));
        }
        Ok(read)
    };

    let mut offset = read_line(&mut line)? as u64;
    let version3 = match line.trim_end() {
        SIGNATURE_V2 => false,
        SIGNATURE_V3 => true,
        _ => return Err(invalid("not a git bundle")),
    };
    let mut refs = Vec::new();
    loop {
        let read = read_line(&mut line)?;
        if read == 0 {
            return Err(invalid("no pack after the header"));
        }
        offset += read as u64;
        let line = line.trim_end_matches('\n');
        if line.is_empty() {
            break;
        }
        if let Some(capability) = line.strip_prefix('@') {
            // v3 only says which hash the objects use: SHA-1 is all
            // libgit2 reads here.
            if !version3 || capability != "object-format=sha1" {
                return Err(invalid("an unsupported bundle capability"));
            }
            continue;
        }
        if line.starts_with('-') {
            return Err(invalid("a partial bundle"));
        }
        let (oid, name) = line
            .split_once(' ')
            .ok_or_else(|| invalid("a malformed ref"))?;
        let oid = Oid::from_str(oid).map_err(|_| invalid("a malformed ref"))?;
        if oid.as_bytes().len() != 20 || !valid_ref_name(name) {
            return Err(invalid("a malformed ref"));
        }
        refs.push((oid, name.to_string()));
        if refs.len() > MAX_REFS {
            return Err(invalid("too many refs"));
        }
    }
    Ok(Header {
        refs,
        pack_offset: offset,
    })
}

/// Adds the objects of the bundle at `path` (with `header`, from
/// [`read_header`]) to `repo`.
pub fn import_pack(repo: &Repository, path: &Path, header: &Header) -> Result<(), git2::Error> {
    let io_error =
        |err: io::Error| git2::Error::from_str(&format!("reading a bundle: {}", err.kind()));
    let mut file = fs::File::open(path).map_err(io_error)?;
    let size = file.metadata().map_err(io_error)?.len();
    if size.saturating_sub(header.pack_offset) > MAX_PACK_BYTES {
        return Err(git2::Error::from_str("the bundle is too large"));
    }
    io::Seek::seek(&mut file, io::SeekFrom::Start(header.pack_offset)).map_err(io_error)?;
    let odb = repo.odb()?;
    let mut writer = odb.packwriter()?;
    io::copy(&mut file.take(MAX_PACK_BYTES), &mut writer).map_err(io_error)?;
    writer.commit()?;
    Ok(())
}

fn invalid(what: &str) -> io::Error {
    io::Error::new(io::ErrorKind::InvalidData, format!("bundle: {what}"))
}

/// `HEAD` or a ref under `refs/`, in git's rules (no `..`, no control
/// characters, …).
fn valid_ref_name(name: &str) -> bool {
    name == "HEAD" || (name.starts_with("refs/") && git2::Reference::is_valid_name(name))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp_dir(name: &str) -> std::path::PathBuf {
        let dir = std::env::temp_dir().join(format!("passel-bundle-{name}-{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        dir
    }

    fn write_file(dir: &Path, name: &str, bytes: &[u8]) -> std::path::PathBuf {
        let path = dir.join(name);
        fs::write(&path, bytes).unwrap();
        path
    }

    #[test]
    fn header_lists_refs_and_finds_the_pack() {
        let dir = temp_dir("header");
        let oid = "0123456789abcdef0123456789abcdef01234567";
        let path = write_file(
            &dir,
            "a.bundle",
            format!("{SIGNATURE_V2}\n{oid} refs/heads/main\n{oid} HEAD\n\nPACK").as_bytes(),
        );
        let header = read_header(&path).unwrap();
        assert_eq!(header.refs.len(), 2);
        assert_eq!(
            header.branch(None),
            Some(("main".to_string(), Oid::from_str(oid).unwrap()))
        );
        assert_eq!(header.branch(Some("other")).unwrap().0, "main");
        assert_eq!(
            fs::read(&path).unwrap()[header.pack_offset as usize..],
            *b"PACK"
        );
        fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn header_refuses_what_is_not_a_whole_bundle() {
        let dir = temp_dir("invalid");
        let oid = "0123456789abcdef0123456789abcdef01234567";
        for (name, text) in [
            ("junk", "hello\n".to_string()),
            ("tar", "password-store/\0\0\0".to_string()),
            (
                "partial",
                format!("{SIGNATURE_V2}\n-{oid} base\n{oid} HEAD\n\nPACK"),
            ),
            ("no pack", format!("{SIGNATURE_V2}\n{oid} HEAD\n")),
            (
                "bad ref",
                format!("{SIGNATURE_V2}\n{oid} refs/heads/a..b\n\nPACK"),
            ),
            ("not refs", format!("{SIGNATURE_V2}\n{oid} config\n\nPACK")),
            (
                "sha256",
                format!("{SIGNATURE_V3}\n@object-format=sha256\n{oid} HEAD\n\nPACK"),
            ),
        ] {
            let path = write_file(&dir, name, text.as_bytes());
            let err = read_header(&path).unwrap_err();
            assert_eq!(err.kind(), io::ErrorKind::InvalidData, "{name}");
        }
        fs::remove_dir_all(dir).unwrap();
    }
}
