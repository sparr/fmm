use anyhow::Result;
use byteorder::{LittleEndian, ReadBytesExt};
use compress::zlib;
use semver::Version;
use std::fs::File;
use std::io::prelude::*;
use std::io::Cursor;
use std::io::Read;
use std::io::SeekFrom;
use std::path::PathBuf;
use thiserror::Error;
use zip::ZipArchive;

use crate::types::ModIdent;

const ONE_MEBIBYTE: usize = 1_048_576;

pub struct SaveFile {
    pub map_version: Version,
    pub mods: Vec<ModIdent>,
    pub path: PathBuf,
    pub scenario_mod_name: String,
    pub scenario: String,
}

impl SaveFile {
    pub fn from(path: PathBuf) -> Result<Self> {
        println!("Reading save file...");
        let mut archive = ZipArchive::new(File::open(&path)?)?;
        let mut compressed = true;
        let filenames: Vec<&str> = archive.file_names().collect();
        let filename = filenames
            .iter()
            .find(|filename| filename.contains("level.dat0"))
            .or_else(|| {
                compressed = false;
                filenames
                    .iter()
                    .find(|filename| filename.contains("level.dat"))
            })
            .map(ToString::to_string)
            .ok_or(SaveFileErr::NoLevelDat)?;
        let file = archive.by_name(&filename)?;

        let decompressed = if compressed {
            // Pre-allocate 1 MiB just in case
            let mut bytes = Vec::with_capacity(ONE_MEBIBYTE);
            zlib::Decoder::new(file).read_exact(&mut bytes)?;
            bytes
        } else {
            // Limit to 1 MiB to avoid problems with giant level.dats
            file.bytes()
                .take(ONE_MEBIBYTE)
                .filter_map(|byte| byte.ok())
                .collect()
        };

        let mut reader = Cursor::new(decompressed);
        let version_major = reader.read_u16::<LittleEndian>()?;
        let version_minor = reader.read_u16::<LittleEndian>()?;
        let version_patch = reader.read_u16::<LittleEndian>()?;
        let _version_build = reader.read_u16::<LittleEndian>()?;

        reader.seek(SeekFrom::Current(2))?;

        let scenario_name = read_string(&mut reader)?;
        let scenario_mod_name = read_string(&mut reader)?;

        // TODO: Handle campaigns
        reader.seek(SeekFrom::Current(14))?;

        let num_mods = reader.read_u8()?;

        let mut mods = Vec::with_capacity(num_mods as usize);
        for _ in 0..num_mods {
            mods.push(read_mod(&mut reader)?);
        }

        Ok(Self {
            mods,
            map_version: Version::new(
                version_major as u64,
                version_minor as u64,
                version_patch as u64,
            ),
            path,
            scenario: scenario_name,
            scenario_mod_name,
        })
    }
}

#[derive(Debug, Error)]
pub enum SaveFileErr {
    #[error("No level.dat was found in the save file")]
    NoLevelDat,
}

fn read_string(reader: &mut Cursor<Vec<u8>>) -> Result<String> {
    let scenario_len = reader.read_u8()?;
    let mut scenario_name = vec![0; scenario_len as usize];
    reader.read_exact(&mut scenario_name)?;

    Ok(String::from_utf8_lossy(&scenario_name).to_string())
}

fn read_mod(reader: &mut Cursor<Vec<u8>>) -> Result<ModIdent> {
    let mod_name = read_string(reader)?;

    // TODO: Read mod versions
    reader.seek(SeekFrom::Current(7))?;

    Ok(ModIdent {
        name: mod_name,
        version_req: None,
    })
}
