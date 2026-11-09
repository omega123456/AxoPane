use std::collections::HashMap;
use std::fs;
use std::os::unix::fs::MetadataExt;
use std::path::{Path, PathBuf};

use super::directory_bin::{self, OriginalLocation, TrashIndex};
use super::{dsstore, manifest, TrashEntry};

fn put_back_index(trash_dir: &Path) -> HashMap<String, dsstore::PutBack> {
    fs::read(trash_dir.join(".DS_Store"))
        .map(|bytes| dsstore::parse_put_back(&bytes))
        .unwrap_or_default()
}

struct CommandIndexes {
    manifest: manifest::Transaction,
    put_back: HashMap<String, dsstore::PutBack>,
}

impl TrashIndex for CommandIndexes {
    fn load(trash_dir: &Path) -> Result<Self, String> {
        Ok(Self {
            manifest: manifest::Transaction::load(trash_dir)?,
            put_back: put_back_index(trash_dir),
        })
    }

    fn original_location(&self, id: &str) -> Option<OriginalLocation> {
        match self.manifest.lookup(id) {
            Some(entry) => Some(OriginalLocation::from(entry)),
            None => self.put_back.get(id).map(|put_back| OriginalLocation {
                original_path: dsstore::resolve_original_path(
                    Path::new("/"),
                    &put_back.original_dir,
                    &put_back.original_name,
                ),
                deleted_at: None,
            }),
        }
    }

    fn fallback_deleted_at(metadata: &fs::Metadata) -> Option<i64> {
        Some(metadata.ctime())
    }

    fn manifest(&mut self) -> &mut manifest::Transaction {
        &mut self.manifest
    }

    fn commit_restores(&mut self) -> Result<(), String> {
        self.manifest.commit()
    }
}

pub fn home_trash_dir() -> Result<PathBuf, String> {
    crate::fs::home_dir()
        .map(|home| home.join(".Trash"))
        .ok_or_else(|| "could not resolve home directory".to_string())
}

pub fn list_trash() -> Result<Vec<TrashEntry>, String> {
    directory_bin::list_trash_in::<CommandIndexes>(&home_trash_dir()?)
}

pub fn restore_from_trash(ids: &[String]) -> Result<(), String> {
    directory_bin::restore_from_trash_in::<CommandIndexes>(ids, &home_trash_dir()?)
}

pub fn empty_trash() -> Result<(), String> {
    directory_bin::empty_trash_in::<CommandIndexes>(&home_trash_dir()?)
}

pub fn delete_from_trash(ids: &[String]) -> Result<(), String> {
    directory_bin::delete_from_trash_in::<CommandIndexes>(ids, &home_trash_dir()?)
}
