//! The device's vaults: core's [`VaultList`], kept as JSON next to the
//! keyring (`vaults.json`). A vault is identified by its folder relative
//! to the app's data folder (`vaults/work`), so ids never leave it.

use std::{
    fs, io,
    path::{Component, Path, PathBuf},
};

use passel_core::app::vaults::{unused_vault_dir, VaultList};
use passel_core::i18n::gettext;
use serde_json::{json, Value};

/// Where the first version kept its only vault.
const LEGACY_DIR: &str = "vault";

pub(crate) struct Vaults {
    data_dir: PathBuf,
    list: VaultList,
}

/// A vault as the list describes it.
pub(crate) struct Listed {
    pub id: String,
    pub name: String,
    pub dir: PathBuf,
    pub active: bool,
}

impl Vaults {
    /// Reads the list, or starts one: with the first version's single
    /// vault (as "Passwords") when there is one.
    pub fn load(data_dir: &Path) -> Self {
        let mut vaults = Self {
            data_dir: data_dir.to_path_buf(),
            list: VaultList::default(),
        };
        match fs::read(vaults.file()) {
            Ok(bytes) => vaults.read_json(&bytes),
            Err(_) => {
                let legacy = data_dir.join(LEGACY_DIR);
                if legacy.join(".git").is_dir() {
                    vaults.add(&legacy, &gettext("Passwords"));
                    if let Err(err) = vaults.save() {
                        log::warn!("could not save the vault list: {err}");
                    }
                }
            }
        }
        vaults
    }

    fn file(&self) -> PathBuf {
        self.data_dir.join("vaults.json")
    }

    fn read_json(&mut self, bytes: &[u8]) {
        let Ok(value) = serde_json::from_slice::<Value>(bytes) else {
            log::warn!("unreadable vault list, starting a new one");
            return;
        };
        for vault in value["vaults"].as_array().into_iter().flatten() {
            let (Some(name), Some(id)) = (vault["name"].as_str(), vault["path"].as_str()) else {
                continue;
            };
            if let Some(dir) = self.dir_for_id(id) {
                self.add(&dir, name);
            }
        }
        match value["active"].as_str().and_then(|id| self.dir_for_id(id)) {
            Some(dir) => {
                self.list.set_active(&dir);
            }
            None => self.activate_first(),
        }
    }

    /// Writes the list (atomically: a crash keeps the previous one).
    pub fn save(&self) -> io::Result<()> {
        let vaults = self
            .list
            .vaults()
            .iter()
            .map(|vault| json!({"name": vault.name, "path": self.id(&vault.path)}))
            .collect::<Vec<_>>();
        let active = self.list.active().map(|vault| self.id(&vault.path));
        let text = serde_json::to_vec_pretty(&json!({"active": active, "vaults": vaults}))?;
        let tmp = self.file().with_extension("json.tmp");
        fs::write(&tmp, text)?;
        fs::rename(tmp, self.file())
    }

    pub fn listed(&self) -> Vec<Listed> {
        let active = self.list.active().map(|vault| vault.path.clone());
        self.list
            .vaults()
            .iter()
            .map(|vault| Listed {
                id: self.id(&vault.path),
                name: vault.name.clone(),
                dir: vault.path.clone(),
                active: active.as_ref() == Some(&vault.path),
            })
            .collect()
    }

    pub fn active_dir(&self) -> Option<PathBuf> {
        self.list.active().map(|vault| vault.path.clone())
    }

    /// A folder for a new vault named `name`, unused so far.
    pub fn new_dir(&self, name: &str) -> PathBuf {
        let slug: String = name
            .trim()
            .to_lowercase()
            .chars()
            .map(|c| if c.is_alphanumeric() { c } else { '-' })
            .collect();
        let slug = slug.trim_matches('-');
        let base = self
            .data_dir
            .join("vaults")
            .join(if slug.is_empty() { "vault" } else { slug });
        unused_vault_dir(&self.list, &base, Path::exists)
    }

    /// Adds the vault at `dir` (named `name`, made unique) and opens it.
    pub fn add(&mut self, dir: &Path, name: &str) {
        let name = name.trim();
        let default_name = gettext("Passwords");
        let name = self
            .list
            .unique_name(if name.is_empty() { &default_name } else { name });
        self.list.add(dir);
        if let Some(vault) = self.list.get_mut(dir) {
            vault.name = name;
        }
        self.list.set_active(dir);
    }

    pub fn activate(&mut self, id: &str) -> bool {
        self.dir_for_id(id)
            .is_some_and(|dir| self.list.set_active(&dir))
    }

    /// Forgets a vault and returns its folder (to delete); the next one
    /// in the list becomes the open one.
    pub fn remove(&mut self, id: &str) -> Option<PathBuf> {
        let dir = self.dir_for_id(id)?;
        self.list.get(&dir)?;
        self.list.remove(&dir);
        if self.list.active().is_none() {
            self.activate_first();
        }
        Some(dir)
    }

    fn activate_first(&mut self) {
        if let Some(first) = self.list.vaults().first().map(|vault| vault.path.clone()) {
            self.list.set_active(&first);
        }
    }

    fn id(&self, dir: &Path) -> String {
        dir.strip_prefix(&self.data_dir)
            .unwrap_or(dir)
            .components()
            .filter_map(|c| match c {
                Component::Normal(part) => Some(part.to_string_lossy()),
                _ => None,
            })
            .collect::<Vec<_>>()
            .join("/")
    }

    /// The folder of an id, refusing anything outside the data folder.
    fn dir_for_id(&self, id: &str) -> Option<PathBuf> {
        let relative = Path::new(id);
        let inside = !id.is_empty()
            && relative
                .components()
                .all(|c| matches!(c, Component::Normal(_)));
        inside.then(|| self.data_dir.join(relative))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp_dir(name: &str) -> PathBuf {
        let unique = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let dir = std::env::temp_dir().join(format!("passel-vaults-{name}-{unique}"));
        fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[test]
    fn keeps_vaults_and_the_open_one_across_loads() {
        let data = temp_dir("persist");
        let mut vaults = Vaults::load(&data);
        assert!(vaults.listed().is_empty());

        let work = vaults.new_dir("Work stuff");
        assert_eq!(work, data.join("vaults/work-stuff"));
        vaults.add(&work, "Work stuff");
        fs::create_dir_all(&work).unwrap();
        let home = vaults.new_dir("Work stuff");
        assert_eq!(
            home,
            data.join("vaults/work-stuff-2"),
            "folders never collide"
        );
        vaults.add(&home, "Work stuff");
        assert!(vaults.activate("vaults/work-stuff"));
        vaults.save().unwrap();

        let reloaded = Vaults::load(&data);
        let listed = reloaded.listed();
        assert_eq!(
            listed
                .iter()
                .map(|v| (v.id.as_str(), v.name.as_str(), v.active))
                .collect::<Vec<_>>(),
            vec![
                ("vaults/work-stuff", "Work stuff", true),
                ("vaults/work-stuff-2", "Work stuff 2", false),
            ]
        );

        let mut reloaded = reloaded;
        assert_eq!(reloaded.remove("vaults/work-stuff"), Some(work));
        assert_eq!(reloaded.active_dir(), Some(home));
        fs::remove_dir_all(data).unwrap();
    }

    #[test]
    fn adopts_the_first_versions_vault() {
        let data = temp_dir("legacy");
        fs::create_dir_all(data.join("vault/.git")).unwrap();
        let vaults = Vaults::load(&data);
        let listed = vaults.listed();
        assert_eq!(listed.len(), 1);
        assert_eq!(
            (listed[0].id.as_str(), listed[0].name.as_str()),
            ("vault", "Passwords")
        );
        assert!(data.join("vaults.json").exists());
        fs::remove_dir_all(data).unwrap();
    }

    #[test]
    fn ids_cannot_leave_the_data_folder() {
        let data = temp_dir("escape");
        let mut vaults = Vaults::load(&data);
        for id in ["../etc", "/etc", "", "vaults/../../x"] {
            assert!(!vaults.activate(id), "{id}");
            assert!(vaults.remove(id).is_none(), "{id}");
        }
        fs::remove_dir_all(data).unwrap();
    }
}
