// The mod-storage layer: everything that reads or writes files under engage/mods goes through a
// `ModStore` handle so the same install/scan logic works on two very different backends.
//
// - Desktop: a real host directory (`sd_root/engage/mods`), plain `std::fs`.
// - Android: the folder the user granted through the Storage Access Framework, reached over JNI via
//   the `saf` module (there is no host path, only a tree document URI).
//
// The store deals only in bytes and relative paths ("MyMod/config.yaml", "MyMod.zip"). All the smart
// parts — zip un-nesting, config.yaml generation, source detection — live in `install.rs` and are
// shared; the store just moves bytes. Only one of the two impls below compiles per build.

use std::time::SystemTime;

#[cfg(feature = "desktop")]
use std::path::{Path, PathBuf};

// One top-level entry under engage/mods: a mod folder ("MyMod") or a zipped mod ("MyMod.zip").
#[derive(Clone, PartialEq, Debug)]
pub struct StoreEntry {
    pub name: String,
    pub is_dir: bool,
    // When it landed, for the "recently installed" sort. Filesystem ctime/mtime on desktop, the
    // SAF lastModified on Android.
    pub modified: Option<SystemTime>,
}

// A write that failed. Kept as a plain message so it's the same type on both platforms.
#[derive(Debug)]
pub struct StoreError(pub String);

impl std::fmt::Display for StoreError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.0)
    }
}

// ---- Desktop: std::fs over sd_root/engage/mods -----------------------------

#[cfg(feature = "desktop")]
#[derive(Clone, PartialEq)]
pub struct ModStore {
    // The engage/mods directory. Every relative path is joined onto this.
    root: PathBuf,
}

#[cfg(feature = "desktop")]
impl ModStore {
    // Build a store rooted at <sd_root>/engage/mods.
    pub fn new(sd_root: &Path) -> Self {
        Self { root: sd_root.join("engage").join("mods") }
    }

    // A real host path for a relative entry, for the desktop-only "reveal in file browser" actions
    // and the thumbnail protocol handler.
    pub fn path_of(&self, rel: &str) -> PathBuf {
        self.root.join(rel)
    }

    pub fn list(&self) -> Vec<StoreEntry> {
        let Ok(entries) = std::fs::read_dir(&self.root) else {
            return Vec::new();
        };
        entries
            .flatten()
            .map(|e| {
                let path = e.path();
                let modified = std::fs::metadata(&path)
                    .ok()
                    .and_then(|md| md.created().or_else(|_| md.modified()).ok());
                StoreEntry {
                    name: e.file_name().to_string_lossy().to_string(),
                    is_dir: path.is_dir(),
                    modified,
                }
            })
            .collect()
    }

    pub fn read(&self, rel: &str) -> Option<Vec<u8>> {
        std::fs::read(self.root.join(rel)).ok()
    }

    pub fn write(&self, rel: &str, bytes: &[u8]) -> Result<(), StoreError> {
        let path = self.root.join(rel);
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent).map_err(|e| StoreError(e.to_string()))?;
        }
        std::fs::write(&path, bytes).map_err(|e| StoreError(e.to_string()))
    }

    pub fn remove(&self, rel: &str) -> bool {
        let path = self.root.join(rel);
        if path.is_dir() {
            std::fs::remove_dir_all(&path).is_ok()
        } else {
            std::fs::remove_file(&path).is_ok()
        }
    }

    pub fn exists(&self, rel: &str) -> bool {
        self.root.join(rel).exists()
    }
}

// ---- Android: the SAF tree, reached over JNI through the `saf` module -------

#[cfg(target_os = "android")]
#[derive(Clone, PartialEq)]
pub struct ModStore;

#[cfg(target_os = "android")]
impl ModStore {
    // There's only ever one granted tree, so the store carries no state.
    pub fn new() -> Self {
        Self
    }

    // There's no host path on Android (the target is a SAF document tree). This exists only so the
    // shared reveal/open UI compiles; those actions are no-ops on Android.
    pub fn path_of(&self, rel: &str) -> std::path::PathBuf {
        std::path::PathBuf::from(rel)
    }

    pub fn list(&self) -> Vec<StoreEntry> {
        // The bridge hands back a JSON array of { name, isDir, modified } (modified in unix millis,
        // 0 when unknown). A provider that refuses to enumerate returns "[]", so this fails soft.
        #[derive(serde::Deserialize)]
        struct Raw {
            name: String,
            #[serde(rename = "isDir")]
            is_dir: bool,
            #[serde(default)]
            modified: u64,
        }
        let json = crate::saf::list_dir("engage/mods").unwrap_or_default();
        let rows: Vec<Raw> = serde_json::from_str(&json).unwrap_or_default();
        rows.into_iter()
            .map(|r| StoreEntry {
                name: r.name,
                is_dir: r.is_dir,
                modified: (r.modified > 0)
                    .then(|| SystemTime::UNIX_EPOCH + std::time::Duration::from_millis(r.modified)),
            })
            .collect()
    }

    pub fn read(&self, rel: &str) -> Option<Vec<u8>> {
        crate::saf::read_file(&format!("engage/mods/{rel}")).ok().flatten()
    }

    pub fn write(&self, rel: &str, bytes: &[u8]) -> Result<(), StoreError> {
        match crate::saf::write_mod_file(&format!("engage/mods/{rel}"), bytes) {
            Ok(true) => Ok(()),
            Ok(false) => Err(StoreError(format!("SAF refused to write {rel}"))),
            Err(e) => Err(StoreError(e.to_string())),
        }
    }

    pub fn remove(&self, rel: &str) -> bool {
        crate::saf::delete_path(&format!("engage/mods/{rel}")).unwrap_or(false)
    }

    pub fn exists(&self, rel: &str) -> bool {
        crate::saf::path_exists(&format!("engage/mods/{rel}")).unwrap_or(false)
    }
}
