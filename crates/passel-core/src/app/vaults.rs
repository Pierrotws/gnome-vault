//! The list of vaults the user opened, and which one is active.
//!
//! Pure data: the window loads it from and saves it to GSettings. A vault
//! is identified by its folder; everything else is a per-vault setting.

use std::path::{Path, PathBuf};

/// One vault in the list, with its own settings.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct VaultConfig {
    pub name: String,
    pub path: PathBuf,
    /// The user answered "Not Now" to turning on history for this pass
    /// store: open it without history and do not ask again.
    pub history_declined: bool,
    /// Branch override for history and pushes; empty follows HEAD.
    pub branch: String,
    pub autopush: bool,
}

impl VaultConfig {
    /// A vault at `path` with default settings.
    pub fn new(name: impl Into<String>, path: impl Into<PathBuf>) -> Self {
        Self {
            name: name.into(),
            path: path.into(),
            history_declined: false,
            branch: String::new(),
            autopush: true,
        }
    }

    /// The branch override, `None` when the vault follows HEAD.
    pub fn branch(&self) -> Option<String> {
        let branch = self.branch.trim();
        (!branch.is_empty()).then(|| branch.to_string())
    }
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct VaultList {
    vaults: Vec<VaultConfig>,
    active: Option<PathBuf>,
}

impl VaultList {
    /// Builds a list from saved data. Duplicate folders keep their first
    /// entry, and an active folder that is not listed is dropped.
    pub fn new(vaults: Vec<VaultConfig>, active: Option<PathBuf>) -> Self {
        let mut list = Self::default();
        for vault in vaults {
            if list.get(&vault.path).is_none() {
                list.vaults.push(vault);
            }
        }
        list.active = active.filter(|path| list.get(path).is_some());
        list
    }

    pub fn vaults(&self) -> &[VaultConfig] {
        &self.vaults
    }

    pub fn is_empty(&self) -> bool {
        self.vaults.is_empty()
    }

    pub fn get(&self, path: &Path) -> Option<&VaultConfig> {
        self.vaults.iter().find(|vault| vault.path == path)
    }

    pub fn get_mut(&mut self, path: &Path) -> Option<&mut VaultConfig> {
        self.vaults.iter_mut().find(|vault| vault.path == path)
    }

    pub fn active(&self) -> Option<&VaultConfig> {
        self.active.as_deref().and_then(|path| self.get(path))
    }

    /// Makes a listed vault the active one. Returns false when the folder
    /// is not in the list.
    pub fn set_active(&mut self, path: &Path) -> bool {
        if self.get(path).is_none() {
            return false;
        }
        self.active = Some(path.to_path_buf());
        true
    }

    /// Adds a vault for `path` (named after its folder, made unique) and
    /// returns its entry. An already listed folder is returned as is.
    pub fn add(&mut self, path: &Path) -> &VaultConfig {
        if let Some(index) = self.vaults.iter().position(|vault| vault.path == path) {
            return &self.vaults[index];
        }
        let name = self.unique_name(&suggested_name(path));
        self.vaults.push(VaultConfig::new(name, path));
        self.vaults.last().expect("just pushed")
    }

    /// Removes a vault from the list (its folder stays on disk). When it
    /// was active, the first remaining vault becomes active.
    pub fn remove(&mut self, path: &Path) {
        self.vaults.retain(|vault| vault.path != path);
        if self.active.as_deref() == Some(path) {
            self.active = self.vaults.first().map(|vault| vault.path.clone());
        }
    }

    /// `base`, or `base 2`, `base 3`… if another vault already has it.
    pub fn unique_name(&self, base: &str) -> String {
        let taken = |name: &str| self.vaults.iter().any(|vault| vault.name == name);
        if !taken(base) {
            return base.to_string();
        }
        (2..)
            .map(|n| format!("{base} {n}"))
            .find(|name| !taken(name))
            .expect("an unused name exists")
    }
}

/// A display name for a vault folder: pass's default folder is
/// "Passwords", others use their folder name without a leading dot.
pub fn suggested_name(path: &Path) -> String {
    let folder = path
        .file_name()
        .map(|name| name.to_string_lossy().into_owned())
        .unwrap_or_default();
    let folder = folder.trim_start_matches('.');
    match folder {
        // Translators: the name given to pass's default vault folder
        // (~/.password-store); the user can rename it.
        "" | "password-store" => crate::i18n::gettext("Passwords"),
        other => other.to_string(),
    }
}

/// A folder for a new vault that is neither listed nor existing:
/// `base`, then `base-2`, `base-3`…
pub fn unused_vault_dir(list: &VaultList, base: &Path, exists: impl Fn(&Path) -> bool) -> PathBuf {
    let free = |path: &Path| list.get(path).is_none() && !exists(path);
    if free(base) {
        return base.to_path_buf();
    }
    let name = base
        .file_name()
        .map(|name| name.to_string_lossy().into_owned())
        .unwrap_or_else(|| "vault".to_string());
    (2..)
        .map(|n| base.with_file_name(format!("{name}-{n}")))
        .find(|path| free(path))
        .expect("an unused folder exists")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn names_vaults_after_their_folder() {
        assert_eq!(
            suggested_name(Path::new("/home/a/.password-store")),
            "Passwords"
        );
        assert_eq!(
            suggested_name(Path::new("/home/a/work-vault")),
            "work-vault"
        );
        assert_eq!(suggested_name(Path::new("/media/usb/.family")), "family");
    }

    #[test]
    fn adds_each_folder_once_with_unique_names() {
        let mut list = VaultList::default();
        assert_eq!(list.add(Path::new("/a/.password-store")).name, "Passwords");
        assert_eq!(
            list.add(Path::new("/b/.password-store")).name,
            "Passwords 2"
        );
        assert_eq!(list.add(Path::new("/a/.password-store")).name, "Passwords");
        assert_eq!(list.vaults().len(), 2);
    }

    #[test]
    fn keeps_a_listed_vault_active() {
        let mut list = VaultList::default();
        list.add(Path::new("/a"));
        list.add(Path::new("/b"));
        assert!(list.active().is_none());
        assert!(!list.set_active(Path::new("/c")));
        assert!(list.set_active(Path::new("/b")));
        assert_eq!(list.active().unwrap().path, Path::new("/b"));

        // Removing the active vault falls back to the first one left.
        list.remove(Path::new("/b"));
        assert_eq!(list.active().unwrap().path, Path::new("/a"));
        list.remove(Path::new("/a"));
        assert!(list.active().is_none());
        assert!(list.is_empty());
    }

    #[test]
    fn loads_saved_data_defensively() {
        let list = VaultList::new(
            vec![
                VaultConfig::new("A", "/a"),
                VaultConfig::new("A again", "/a"),
                VaultConfig::new("B", "/b"),
            ],
            Some(PathBuf::from("/gone")),
        );
        assert_eq!(list.vaults().len(), 2);
        assert_eq!(list.vaults()[0].name, "A");
        assert!(list.active().is_none());
    }

    #[test]
    fn suggests_an_unused_folder_for_a_new_vault() {
        let mut list = VaultList::default();
        let base = Path::new("/home/a/.password-store");
        assert_eq!(unused_vault_dir(&list, base, |_| false), base);

        list.add(base);
        let existing = Path::new("/home/a/.password-store-2");
        assert_eq!(
            unused_vault_dir(&list, base, |path| path == existing),
            Path::new("/home/a/.password-store-3")
        );
    }

    #[test]
    fn empty_branch_follows_head() {
        let mut vault = VaultConfig::new("A", "/a");
        assert_eq!(vault.branch(), None);
        vault.branch = " main ".to_string();
        assert_eq!(vault.branch().as_deref(), Some("main"));
    }
}
