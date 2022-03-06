use crate::dat::PropertyTree;
use crate::mod_settings::ModSettings;
use crate::types::*;
use anyhow::{anyhow, Result};
use console::style;
use semver::Version;
use std::collections::HashMap;
use std::ffi::OsString;
use std::fs;
use std::fs::{DirEntry, File};
use std::io::Read;
use std::path::PathBuf;
use zip::ZipArchive;

pub struct Directory {
    pub mods: HashMap<String, Vec<ModEntry>>,
    // TODO: Mod list and mod settings can both be None
    pub mod_list: Vec<ModListJsonMod>,
    pub mod_list_path: PathBuf,
    pub mod_settings: ModSettings,
}

// This is a mess
// We need to refactor this to retrive ModEntries in a standardized way, with the properties from the info.json lazily loaded when needed
impl Directory {
    pub fn new(path: &PathBuf) -> Result<Self> {
        // Get all mods in the directory
        let mod_entries = fs::read_dir(&path)?
            .filter_map(|entry| {
                let entry = entry.ok()?;

                if let Some((mod_name, version)) = parse_file_name(&entry.file_name()) {
                    Some((
                        mod_name.clone(),
                        ModEntry {
                            entry,
                            ident: ModIdent {
                                name: mod_name,
                                version: Some(version),
                            },
                        },
                    ))
                } else {
                    let info_json = read_info_json(&entry)?;
                    Some((
                        info_json.name.clone(),
                        ModEntry {
                            entry,
                            ident: ModIdent {
                                name: info_json.name,
                                version: Some(info_json.version),
                            },
                        },
                    ))
                }
            })
            .fold(HashMap::new(), |mut directory_mods, (mod_name, version)| {
                let versions = directory_mods.entry(mod_name).or_insert_with(Vec::new);

                let index = versions
                    .binary_search(&version)
                    .unwrap_or_else(|index| index);
                versions.insert(index, version);

                directory_mods
            });

        // Parse mod-list.json
        let mut mlj_path = path.clone();
        mlj_path.push("mod-list.json");
        let enabled_versions = fs::read_to_string(&mlj_path)?;
        let mod_list_json: ModListJson = serde_json::from_str(&enabled_versions)?;

        Ok(Self {
            mods: mod_entries,
            mod_list: mod_list_json.mods,
            mod_list_path: mlj_path,
            mod_settings: ModSettings::new(path)?,
        })
    }

    /// Adds the mod, but keeps it disabled
    pub fn add(&mut self, (mod_name, mod_entry): (String, ModEntry)) {
        // Add or disable mod in mod-list.json
        if let Some(mod_state) = self
            .mod_list
            .iter_mut()
            .find(|mod_state| mod_name == mod_state.name)
        {
            mod_state.enabled = false;
            mod_state.version = None;
        } else {
            self.mod_list.push(ModListJsonMod {
                name: mod_name.clone(),
                enabled: false,
                version: None,
            });
        }

        let entries = self.mods.entry(mod_name).or_default();
        if let Err(index) = entries.binary_search(&mod_entry) {
            entries.insert(index, mod_entry);
        }
    }

    pub fn disable(&mut self, mod_ident: &ModIdent) {
        if mod_ident.name == "base" || self.mods.contains_key(&mod_ident.name) {
            let mod_state = self
                .mod_list
                .iter_mut()
                .find(|mod_state| mod_ident.name == mod_state.name);

            println!("{} {}", style("Disabled").yellow().bold(), &mod_ident);

            if let Some(mod_state) = mod_state {
                mod_state.enabled = false;
                mod_state.version = None;
            }
        } else {
            println!("Could not find {}", &mod_ident);
        }
    }

    pub fn disable_all(&mut self) {
        println!("{}", style("Disabled all mods").yellow().bold());

        for mod_data in self
            .mod_list
            .iter_mut()
            .filter(|mod_state| mod_state.name != "base")
        {
            mod_data.enabled = false;
            mod_data.version = None;
        }
    }

    pub fn enable(&mut self, mod_ident: &ModIdent) -> Result<()> {
        let mod_entry = self
            .mods
            .get(&mod_ident.name)
            .and_then(|mod_entries| crate::get_mod_version(mod_entries, mod_ident))
            .ok_or_else(|| anyhow!("Given mod does not exist"))?;

        let mod_state = self
            .mod_list
            .iter_mut()
            .find(|mod_state| mod_ident.name == mod_state.name);

        let enabled = mod_state
            .as_ref()
            .map(|mod_data| mod_data.enabled)
            .unwrap_or_default();

        if !enabled {
            println!(
                "{} {} v{}",
                style("Enabled").green().bold(),
                mod_ident.name,
                mod_entry.ident.get_guaranteed_version()
            );

            let version = mod_ident
                .version
                .as_ref()
                .map(|_| mod_entry.ident.get_guaranteed_version().clone());

            if let Some(mod_state) = mod_state {
                mod_state.enabled = true;
                mod_state.version = version;
            } else {
                self.mod_list.push(ModListJsonMod {
                    name: mod_ident.name.to_string(),
                    enabled: true,
                    version,
                });
            }
        }

        Ok(())
    }

    pub fn sync_settings(&mut self, save_settings: &PropertyTree) -> Result<()> {
        let startup_settings = self
            .mod_settings
            .settings
            .get_mut("startup")
            .ok_or_else(|| anyhow!("No startup settings in mod-settings.dat"))?
            .as_dictionary_mut()
            .ok_or_else(|| anyhow!("Could not read PropertyTree dictionary."))?;

        for (setting_name, setting_value) in save_settings
            .as_dictionary()
            .ok_or_else(|| anyhow!("Could not read PropertyTree dictionary"))?
        {
            startup_settings.insert(setting_name.clone(), setting_value.clone());
        }

        self.mod_settings.write()?;

        Ok(())
    }

    pub fn save(&self) -> Result<()> {
        fs::write(
            &self.mod_list_path,
            serde_json::to_string_pretty(&ModListJson {
                mods: self.mod_list.clone(),
            })?,
        )?;

        Ok(())
    }

    pub fn contains(&self, mod_ident: &ModIdent) -> bool {
        self.mods
            .get(&mod_ident.name)
            .map(|entries| {
                if let Some(version) = &mod_ident.version {
                    entries
                        .iter()
                        .rev()
                        .any(|entry| entry.ident.get_guaranteed_version() == version)
                } else {
                    !entries.is_empty()
                }
            })
            .unwrap_or(false)
    }
}

fn parse_file_name(file_name: &OsString) -> Option<(String, Version)> {
    let (name, version) = file_name
        .to_str()?
        .trim_end_matches(".zip")
        .rsplit_once('_')?;

    if let Ok(version) = Version::parse(version) {
        Some((name.to_string(), version))
    } else {
        None
    }
}

pub fn read_info_json(entry: &DirEntry) -> Option<InfoJson> {
    let contents = match ModEntryStructure::parse(entry)? {
        ModEntryStructure::Directory | ModEntryStructure::Symlink => {
            let mut path = entry.path();
            path.push("info.json");
            fs::read_to_string(path).ok()?
        }
        ModEntryStructure::Zip => {
            let mut archive = ZipArchive::new(File::open(entry.path()).ok()?).ok()?;
            let filename = archive
                .file_names()
                .find(|name| name.contains("info.json"))
                .map(ToString::to_string)?;
            let mut file = archive.by_name(&filename).ok()?;
            let mut contents = String::new();
            file.read_to_string(&mut contents).ok()?;
            contents
        }
    };

    serde_json::from_str::<InfoJson>(&contents).ok()
}
