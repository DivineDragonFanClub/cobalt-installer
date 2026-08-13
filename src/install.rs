// Installing a downloaded mod into Cobalt's mods folder, and generating a config.yaml for it
// (desktop only).
//
// Cobalt reads mods from <sd>/engage/mods/. Each mod is a folder whose contents mirror the game's
// romfs (a top-level patches/ and/or Data/). Lots of GameBanana archives are wrapped in an extra
// folder, so we detect that and strip it while extracting. If the mod ships its own config.yaml we
// leave it alone, otherwise we write one built from the GameBanana metadata so Cobalt has a name,
// author, and a record of where the mod came from (for its own auto-update later).

use std::collections::HashMap;
use std::io::{Cursor, Read, Write};
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::gamebanana::ModDetail;

// Mirrors Cobalt's own config.yaml shape (see ozone crates/mods/src/manager.rs). We only ever
// write the fields we can fill, but the names and layout have to match exactly or Cobalt won't read
// it. A malformed config.yaml panics the game at boot, so we re-parse what we write before saving.
#[derive(Serialize)]
struct ModConfig {
    id: String,
    name: String,
    description: String,
    author: String,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    dependencies: Vec<String>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    optional_dependencies: Vec<String>,
    // A link to the mod's source code (for plugins), NOT its download/mod page. Where the mod came
    // from and how to update it is recorded in update_sources.
    #[serde(skip_serializing_if = "Option::is_none")]
    source_url: Option<String>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    update_sources: Vec<UpdateSource>,
}

// Where Cobalt can look for a newer version. Serializes as a map keyed by "github"/"gamebanana",
// matching Cobalt. We only generate the gamebanana variant, but github is here too so reading back
// an existing config with a github source doesn't fail.
#[derive(Serialize, Deserialize, Clone)]
enum UpdateSource {
    #[serde(rename = "github")]
    GitHub { user: String, repo: String },
    #[serde(rename = "gamebanana")]
    GameBanana { item_type: String, item_id: u64 },
}

// A lenient view of an already-installed mod's config, just enough to tell where it came from and
// show it in the My Mods list. GameBanana mods are recognized by their update_sources entry, Nexus
// mods by an id we set to "nexus.<mod_id>" (Cobalt has no Nexus update-source variant). Unknown
// fields are ignored.
#[derive(Deserialize, Default)]
struct InstalledConfig {
    #[serde(default)]
    id: String,
    #[serde(default)]
    name: String,
    #[serde(default)]
    author: String,
    #[serde(default)]
    description: String,
    #[serde(default)]
    source_url: String,
    #[serde(default)]
    update_sources: Vec<UpdateSource>,
}

fn mods_dir(sd_root: &Path) -> PathBuf {
    sd_root.join("engage").join("mods")
}

// Scan the mods folder and map each installed GameBanana mod id to its folder. Powers the
// "Installed" badge and lets us replace a mod in place when the user reinstalls or updates it.
pub fn installed_gamebanana_ids(sd_root: &Path) -> HashMap<u64, PathBuf> {
    let mut map = HashMap::new();
    let Ok(entries) = std::fs::read_dir(mods_dir(sd_root)) else {
        return map;
    };
    for entry in entries.flatten() {
        let dir = entry.path();
        if !dir.is_dir() {
            continue;
        }
        let Ok(text) = std::fs::read_to_string(dir.join("config.yaml")) else {
            continue;
        };
        let Ok(config) = serde_yaml::from_str::<InstalledConfig>(&text) else {
            continue;
        };
        for source in config.update_sources {
            if let UpdateSource::GameBanana { item_id, .. } = source {
                map.insert(item_id, dir.clone());
            }
        }
    }
    map
}

// Remove an installed GameBanana mod by its id. Returns whether a folder was actually deleted.
pub fn uninstall_gamebanana_mod(sd_root: &Path, item_id: u64) -> bool {
    match installed_gamebanana_ids(sd_root).get(&item_id) {
        Some(dir) => std::fs::remove_dir_all(dir).is_ok(),
        None => false,
    }
}

// Same idea as installed_gamebanana_ids, but for Nexus mods (found by the "nexus.<id>" config id).
pub fn installed_nexus_ids(sd_root: &Path) -> HashMap<u64, PathBuf> {
    let mut map = HashMap::new();
    let Ok(entries) = std::fs::read_dir(mods_dir(sd_root)) else {
        return map;
    };
    for entry in entries.flatten() {
        let dir = entry.path();
        if !dir.is_dir() {
            continue;
        }
        let Ok(text) = std::fs::read_to_string(dir.join("config.yaml")) else {
            continue;
        };
        let Ok(config) = serde_yaml::from_str::<InstalledConfig>(&text) else {
            continue;
        };
        let id = config
            .id
            .strip_prefix("nexus.")
            .and_then(|n| n.parse::<u64>().ok())
            .or_else(|| nexus_id_from_url(&config.source_url));
        if let Some(id) = id {
            map.insert(id, dir.clone());
        }
    }
    map
}

// Pull the mod id out of a NexusMods page url ("https://www.nexusmods.com/fireemblemengage/
// mods/123"). This is how a mod whose shipped config we annotated with source_url is recognized.
fn nexus_id_from_url(url: &str) -> Option<u64> {
    if !url.contains("nexusmods.com") {
        return None;
    }
    let tail = url.split("/mods/").nth(1)?;
    let digits: String = tail.chars().take_while(char::is_ascii_digit).collect();
    digits.parse().ok()
}

pub fn uninstall_nexus_mod(sd_root: &Path, mod_id: u64) -> bool {
    match installed_nexus_ids(sd_root).get(&mod_id) {
        Some(dir) => std::fs::remove_dir_all(dir).is_ok(),
        None => false,
    }
}

// Where an installed mod came from, worked out from its config.yaml. "Manual" covers anything we
// can't trace back to a source: a hand-dropped folder with no config, or a config we didn't write.
#[derive(Clone, PartialEq, Debug)]
pub enum ModSource {
    GameBanana(u64),
    Nexus(u64),
    Manual,
}

// One entry in the My Mods view: a mod that currently lives in engage/mods, whether we installed it
// or the user dropped it in by hand.
#[derive(Clone, PartialEq, Debug)]
pub struct InstalledMod {
    // The on-disk name (folder name, or file name for a .zip mod). What we delete/open.
    pub folder: String,
    // What to show the user: the config's name if it has one, else the folder name.
    pub name: String,
    pub author: Option<String>,
    // The config's description, if it has one.
    pub description: Option<String>,
    pub source: ModSource,
    // When the mod landed in the folder: creation time where the filesystem
    // has one (macOS/Windows), else mtime. None if the metadata call fails.
    pub installed_at: Option<std::time::SystemTime>,
    // Whether the mod carries a config.yaml at all. Hand-dropped mods often don't.
    pub has_config: bool,
    // File name of the preview image saved at install time ("thumbnail.jpg"), when the folder has
    // one. Feeds the row art through the mod_thumb protocol. Zip mods have nowhere to keep one.
    pub thumb: Option<String>,
    // Full path to the mod folder (or .zip), for opening in the file browser and uninstalling.
    pub path: PathBuf,
}

// Read one config's fields into a source tag. Prefer an explicit gamebanana update_source, then fall
// back to the id prefix we stamp on our own installs.
fn classify_source(config: &InstalledConfig) -> ModSource {
    for source in &config.update_sources {
        if let UpdateSource::GameBanana { item_id, .. } = source {
            return ModSource::GameBanana(*item_id);
        }
    }
    if let Some(id) = config.id.strip_prefix("gamebanana.").and_then(|n| n.parse().ok()) {
        ModSource::GameBanana(id)
    } else if let Some(id) = config.id.strip_prefix("nexus.").and_then(|n| n.parse().ok()) {
        ModSource::Nexus(id)
    } else if let Some(id) = nexus_id_from_url(&config.source_url) {
        ModSource::Nexus(id)
    } else {
        ModSource::Manual
    }
}

// Read a zipped mod's config.yaml without unpacking the archive. Opening the zip only parses the
// central directory, and we search by entry name alone, so exactly one entry is ever decompressed.
//
// Same shallowest-marker rule as root_prefix: a re-zipped mod can bury its config under a wrapper
// folder, and a config nested deeper (a bundled sub-mod) must not win over the real one above it.
fn zip_config(path: &Path) -> Option<InstalledConfig> {
    // A config.yaml is a handful of lines. Anything past this isn't one, so don't spend the memory.
    const MAX_CONFIG_BYTES: u64 = 64 * 1024;

    let mut archive = zip::ZipArchive::new(std::fs::File::open(path).ok()?).ok()?;

    let mut best: Option<(usize, String)> = None;
    for name in archive.file_names() {
        if name.starts_with("__MACOSX/") {
            continue;
        }
        // config.yaml is a file, so it has to be the whole last component. A directory entry ends
        // in '/' and leaves an empty leaf, which never matches.
        let leaf = name.rsplit('/').next().unwrap_or(name);
        if !leaf.eq_ignore_ascii_case("config.yaml") {
            continue;
        }
        let depth = name.matches('/').count();
        let shallower = match &best {
            Some((d, _)) => depth < *d,
            None => true,
        };
        if shallower {
            best = Some((depth, name.to_string()));
        }
    }
    let (_, name) = best?;

    let entry = archive.by_name(&name).ok()?;
    if entry.size() > MAX_CONFIG_BYTES {
        return None;
    }
    // take() on top of the size check: the header's size is a claim until we've actually read it.
    let mut text = String::new();
    entry.take(MAX_CONFIG_BYTES).read_to_string(&mut text).ok()?;
    serde_yaml::from_str::<InstalledConfig>(&text).ok()
}

// List everything currently installed under engage/mods, config or not. Cobalt loads both folder
// mods and <Name>.zip mods, so we show both. Sorted by display name so the list is stable.
pub fn scan_installed_mods(sd_root: &Path) -> Vec<InstalledMod> {
    let mut mods = Vec::new();
    let Ok(entries) = std::fs::read_dir(mods_dir(sd_root)) else {
        return mods;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        let is_dir = path.is_dir();
        let is_zip = path.extension().map(|e| e.eq_ignore_ascii_case("zip")).unwrap_or(false);
        // Skip stray files that aren't a mod folder or a zipped mod.
        if !is_dir && !is_zip {
            continue;
        }
        let folder = entry.file_name().to_string_lossy().to_string();

        // A folder mod keeps its config.yaml on disk, a zipped mod keeps it inside the archive.
        // Either way we read just that one file, so a zip is never unpacked to list it.
        let config = if is_dir {
            std::fs::read_to_string(path.join("config.yaml"))
                .ok()
                .and_then(|text| serde_yaml::from_str::<InstalledConfig>(&text).ok())
        } else {
            zip_config(&path)
        };

        let (name, author, description, source) = match &config {
            Some(c) => {
                let name = if c.name.trim().is_empty() { folder.clone() } else { c.name.clone() };
                let author = (!c.author.trim().is_empty()).then(|| c.author.clone());
                let description = (!c.description.trim().is_empty()).then(|| c.description.clone());
                (name, author, description, classify_source(c))
            }
            None => (folder.clone(), None, None, ModSource::Manual),
        };

        let installed_at = std::fs::metadata(&path)
            .ok()
            .and_then(|md| md.created().or_else(|_| md.modified()).ok());

        // The preview image saved at install time, if any.
        let thumb = is_dir
            .then(|| {
                THUMB_EXTS
                    .iter()
                    .map(|e| format!("thumbnail.{e}"))
                    .find(|n| path.join(n).is_file())
            })
            .flatten();

        mods.push(InstalledMod {
            folder,
            name,
            author,
            description,
            source,
            installed_at,
            has_config: config.is_some(),
            thumb,
            path,
        });
    }
    mods.sort_by(|a, b| a.name.to_lowercase().cmp(&b.name.to_lowercase()));
    mods
}

// Delete an installed mod by its path (a folder or a .zip). Returns whether it was removed.
pub fn remove_installed_mod(path: &Path) -> bool {
    if path.is_dir() {
        std::fs::remove_dir_all(path).is_ok()
    } else {
        std::fs::remove_file(path).is_ok()
    }
}

#[derive(Debug)]
pub enum InstallError {
    NotAZip,
    BadArchive(String),
    Io(String),
    Config(String),
}

impl std::fmt::Display for InstallError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            InstallError::NotAZip => write!(f, "that download isn't a .zip (7z/rar aren't supported yet)"),
            InstallError::BadArchive(e) => write!(f, "couldn't read the archive: {e}"),
            InstallError::Io(e) => write!(f, "file error: {e}"),
            InstallError::Config(e) => write!(f, "couldn't write config.yaml: {e}"),
        }
    }
}

// What a Nexus mod install needs to know to build its config.yaml. Nexus doesn't map onto Cobalt's
// update_sources (only github/gamebanana exist there), so a Nexus mod records its origin in
// source_url only, and we tag its config id "nexus.<mod_id>" so we can find it again later.
pub struct NexusMeta {
    pub mod_id: u64,
    pub name: String,
    pub author: String,
    pub description: String,
    pub source_url: String,
}

// Extract a downloaded mod .zip into engage/mods/<folder>/, un-nesting a wrapper folder if there is
// one. Returns the install folder and whether the archive already carried its own config.yaml.
fn extract_archive(sd_root: &Path, folder: &str, bytes: &[u8]) -> Result<(PathBuf, bool), InstallError> {
    let mut archive = zip::ZipArchive::new(Cursor::new(bytes)).map_err(|_| InstallError::NotAZip)?;

    let names: Vec<String> = archive.file_names().map(String::from).collect();
    let prefix = root_prefix(&names);

    let dest = mods_dir(sd_root).join(folder);
    std::fs::create_dir_all(&dest).map_err(|e| InstallError::Io(e.to_string()))?;

    let mut saw_config = false;
    for i in 0..archive.len() {
        let mut file = archive.by_index(i).map_err(|e| InstallError::BadArchive(e.to_string()))?;
        let Some(name) = file.enclosed_name().map(|p| p.to_string_lossy().to_string()) else {
            continue;
        };

        // Skip the macOS resource-fork junk some archives carry.
        if name.starts_with("__MACOSX/") {
            continue;
        }

        // Drop the wrapper folder so patches/ and Data/ land at the mod root.
        let rel = match name.strip_prefix(&prefix) {
            Some(rest) if !prefix.is_empty() => rest.to_string(),
            _ if prefix.is_empty() => name.clone(),
            _ => continue,
        };
        if rel.is_empty() {
            continue;
        }
        if rel == "config.yaml" {
            saw_config = true;
        }

        let out = dest.join(&rel);
        if file.is_dir() {
            std::fs::create_dir_all(&out).map_err(|e| InstallError::Io(e.to_string()))?;
            continue;
        }
        if let Some(parent) = out.parent() {
            std::fs::create_dir_all(parent).map_err(|e| InstallError::Io(e.to_string()))?;
        }
        let mut buf = Vec::new();
        file.read_to_end(&mut buf).map_err(|e| InstallError::Io(e.to_string()))?;
        std::fs::File::create(&out)
            .and_then(|mut f| f.write_all(&buf))
            .map_err(|e| InstallError::Io(e.to_string()))?;
    }

    Ok((dest, saw_config))
}

// Write a generated config.yaml, but only after checking it re-parses (a broken one crashes the game).
fn write_config(dest: &Path, config: &ModConfig) -> Result<(), InstallError> {
    let yaml = serde_yaml::to_string(config).map_err(|e| InstallError::Config(e.to_string()))?;
    serde_yaml::from_str::<serde_yaml::Value>(&yaml).map_err(|e| InstallError::Config(e.to_string()))?;
    std::fs::write(dest.join("config.yaml"), yaml).map_err(|e| InstallError::Config(e.to_string()))
}

// A mod that ships its own config.yaml usually doesn't record where it came from, which loses the
// Installed badge, replace-on-reinstall, uninstall, and Cobalt's auto-update for that mod. If the
// shipped config carries no gamebanana update_source, add ours. Best-effort by design: a config we
// can't fully parse (including update_sources entries we don't recognize) is left byte-for-byte
// alone — a malformed config.yaml panics the game at boot — and we only save YAML that re-parses.
// The round-trip does drop YAML comments; that's the one cost of stamping.
fn ensure_gamebanana_source(dest: &Path, item_id: u64) {
    let path = dest.join("config.yaml");
    let Ok(text) = std::fs::read_to_string(&path) else { return };
    let Ok(config) = serde_yaml::from_str::<InstalledConfig>(&text) else { return };
    // The modder already declared a GameBanana origin (even a different id): theirs wins.
    if config.update_sources.iter().any(|s| matches!(s, UpdateSource::GameBanana { .. })) {
        return;
    }

    let Ok(mut value) = serde_yaml::from_str::<serde_yaml::Value>(&text) else { return };
    let Some(map) = value.as_mapping_mut() else { return };
    let Ok(entry) = serde_yaml::to_value(UpdateSource::GameBanana { item_type: "Mod".into(), item_id })
    else {
        return;
    };
    let key = serde_yaml::Value::from("update_sources");
    if !map.contains_key(&key) {
        map.insert(key.clone(), serde_yaml::Value::Sequence(Vec::new()));
    }
    match map.get_mut(&key) {
        Some(serde_yaml::Value::Sequence(seq)) => seq.push(entry),
        // A bare `update_sources:` key parses as null; give it its first entry.
        Some(v @ serde_yaml::Value::Null) => *v = serde_yaml::Value::Sequence(vec![entry]),
        // Present but not a list: not a config we should be editing.
        _ => return,
    }

    let Ok(yaml) = serde_yaml::to_string(&value) else { return };
    if serde_yaml::from_str::<InstalledConfig>(&yaml).is_ok() {
        let _ = std::fs::write(&path, yaml);
    }
}

// Nexus counterpart of ensure_gamebanana_source. Cobalt has no nexus update-source variant (an
// unknown tag would break its config parse), and rewriting the mod's id would break anything that
// depends on it — so the only safe place for provenance is source_url, and only when the modder
// left it empty. A config with its own source_url (a plugin's repo, say) stays untouched and the
// mod stays classified Manual; honest, if imperfect.
fn ensure_nexus_source(dest: &Path, meta: &NexusMeta) {
    if meta.source_url.is_empty() {
        return;
    }
    let path = dest.join("config.yaml");
    let Ok(text) = std::fs::read_to_string(&path) else { return };
    let Ok(config) = serde_yaml::from_str::<InstalledConfig>(&text) else { return };
    if config.id.starts_with("nexus.") || !config.source_url.is_empty() {
        return;
    }

    let Ok(mut value) = serde_yaml::from_str::<serde_yaml::Value>(&text) else { return };
    let Some(map) = value.as_mapping_mut() else { return };
    map.insert(
        serde_yaml::Value::from("source_url"),
        serde_yaml::Value::from(meta.source_url.clone()),
    );

    let Ok(yaml) = serde_yaml::to_string(&value) else { return };
    if serde_yaml::from_str::<InstalledConfig>(&yaml).is_ok() {
        let _ = std::fs::write(&path, yaml);
    }
}

// Preview-image extensions we save and serve, in probe order. The webview renders all of these
// natively, so a download keeps its original bytes instead of pulling in an image-conversion crate.
pub(crate) const THUMB_EXTS: [&str; 5] = ["jpg", "jpeg", "png", "webp", "gif"];

// Save a mod's preview image as thumbnail.<ext> in its install folder. Best-effort by design, like
// the config stamping above: nothing in the thumbnail path may ever fail an install (an Err would
// roll back the whole batch in install_transactional, over a decoration). Leftovers under another
// extension are cleared first so a re-install that changes format can't leave two behind.
pub fn write_thumbnail(dest: &Path, bytes: &[u8], ext: &str) {
    let ext = if THUMB_EXTS.contains(&ext) { ext } else { "jpg" };
    for e in THUMB_EXTS {
        if e != ext {
            let _ = std::fs::remove_file(dest.join(format!("thumbnail.{e}")));
        }
    }
    let _ = std::fs::write(dest.join(format!("thumbnail.{ext}")), bytes);
}

// Resolve a webview /mod_thumb request to a real file, or None. `folder` and `file` arrive
// percent-decoded from the URL. The checks make traversal unreachable: folder must be a plain
// single path component and file exactly thumbnail.<known ext>, so the handler can never serve
// config.yaml or anything outside engage/mods/<folder>/.
pub fn thumb_file_path(sd_root: &Path, folder: &str, file: &str) -> Option<PathBuf> {
    if folder.is_empty() || folder.contains(['/', '\\']) || folder.starts_with("..") {
        return None;
    }
    if !THUMB_EXTS.iter().any(|e| file == format!("thumbnail.{e}")) {
        return None;
    }
    let path = mods_dir(sd_root).join(folder).join(file);
    path.is_file().then_some(path)
}

// Install a downloaded GameBanana mod. `bytes` is the raw .zip we already fetched. Extracts into
// engage/mods/<name>/, replacing any earlier install of the same mod, then writes a config.yaml
// (recording the GameBanana source for auto-updates) when the mod doesn't already carry one.
// Returns the install folder so the caller can decorate it (the preview thumbnail).
pub fn install_gamebanana_mod(sd_root: &Path, detail: &ModDetail, bytes: &[u8]) -> Result<PathBuf, InstallError> {
    if let Some(old) = installed_gamebanana_ids(sd_root).get(&detail.id) {
        let _ = std::fs::remove_dir_all(old);
    }

    let (dest, had_config) = extract_archive(sd_root, &sanitize(&detail.name), bytes)?;

    if !had_config {
        let config = ModConfig {
            id: format!("gamebanana.{}", detail.id),
            name: detail.name.clone(),
            description: strip_html(&detail.description_html),
            author: detail.author(),
            dependencies: Vec::new(),
            optional_dependencies: Vec::new(),
            // source_url is for a plugin's source code, not the mod page. The GameBanana origin (and
            // the link back to its download page) lives in update_sources instead.
            source_url: None,
            update_sources: vec![UpdateSource::GameBanana {
                item_type: "Mod".into(),
                item_id: detail.id,
            }],
        };
        write_config(&dest, &config)?;
    } else {
        ensure_gamebanana_source(&dest, detail.id);
    }

    Ok(dest)
}

// Install a downloaded NexusMods mod, same flow as GameBanana. The generated config records the
// Nexus page as source_url only, since Cobalt has no Nexus update-source to auto-update from.
pub fn install_nexus_mod(sd_root: &Path, meta: &NexusMeta, bytes: &[u8]) -> Result<(), InstallError> {
    if let Some(old) = installed_nexus_ids(sd_root).get(&meta.mod_id) {
        let _ = std::fs::remove_dir_all(old);
    }

    let (dest, had_config) = extract_archive(sd_root, &sanitize(&meta.name), bytes)?;

    if !had_config {
        let config = ModConfig {
            id: format!("nexus.{}", meta.mod_id),
            name: meta.name.clone(),
            description: strip_html(&meta.description),
            author: meta.author.clone(),
            dependencies: Vec::new(),
            optional_dependencies: Vec::new(),
            source_url: (!meta.source_url.is_empty()).then(|| meta.source_url.clone()),
            update_sources: Vec::new(),
        };
        write_config(&dest, &config)?;
    } else {
        ensure_nexus_source(&dest, meta);
    }

    Ok(())
}

// Find the mod's real root inside the archive and return the prefix to strip to reach it.
//
// Cobalt's mod root is the folder that directly holds patches/, Data/, a config.yaml, or a plugin
// .nro. Archives often bury that under one or more wrapper folders ("My Mod v1.2/", "My Mod/inner/",
// a doubled folder from a bad re-zip, ...). We locate the shallowest of those markers across all
// entries and strip everything above it, so any depth of intermediate directory is handled. A
// correctly laid out zip has the marker at depth 0 and nothing is stripped. Matching is
// case-insensitive since Cobalt lowercases paths anyway.
fn root_prefix(names: &[String]) -> String {
    // A file whose folder is the mod root: config.yaml, or a plugin .nro (some plugin zips ship just
    // the .nro with no patches/Data/config, so it's the only marker we have to go on).
    let is_file_marker =
        |c: &str| c.eq_ignore_ascii_case("config.yaml") || c.to_ascii_lowercase().ends_with(".nro");

    let mut best: Option<String> = None;

    for name in names {
        if name.starts_with("__MACOSX/") {
            continue;
        }
        let comps: Vec<&str> = name.split('/').collect();

        // The shallowest component in this path that marks a mod root.
        let marker = comps.iter().position(|c| {
            let is_dir = c.eq_ignore_ascii_case("patches") || c.eq_ignore_ascii_case("Data");
            is_dir || is_file_marker(c)
        });

        let Some(i) = marker else { continue };
        // A file marker (config.yaml / .nro) has to be the last component. A patches/Data marker has
        // to have something under it (it's a directory in the path).
        let is_file = is_file_marker(comps[i]);
        if is_file && i + 1 != comps.len() {
            continue;
        }
        if !is_file && i + 1 >= comps.len() {
            continue;
        }

        let prefix = if i == 0 {
            String::new()
        } else {
            format!("{}/", comps[..i].join("/"))
        };
        // Keep the shallowest root we've seen, so a nested folder that happens to be named "data"
        // doesn't win over the real top-level root.
        let shallower = match &best {
            Some(b) => prefix.len() < b.len(),
            None => true,
        };
        if shallower {
            best = Some(prefix);
        }
    }

    best.unwrap_or_default()
}

// Turn a mod name into a safe folder name. Keeps it readable, drops anything a filesystem hates.
fn sanitize(name: &str) -> String {
    let cleaned: String = name
        .chars()
        .map(|c| match c {
            '/' | '\\' | ':' | '*' | '?' | '"' | '<' | '>' | '|' => '_',
            c if c.is_control() => '_',
            c => c,
        })
        .collect();
    let trimmed = cleaned.trim().trim_matches('.').trim();
    if trimmed.is_empty() {
        "mod".into()
    } else {
        trimmed.to_string()
    }
}

// Decode the handful of HTML entities GameBanana/Nexus descriptions actually use.
fn decode_entities(text: &str) -> String {
    text.replace("&amp;", "&")
        .replace("&lt;", "<")
        .replace("&gt;", ">")
        .replace("&quot;", "\"")
        .replace("&#039;", "'")
        .replace("&nbsp;", " ")
}

// Strip HTML tags and decode entities, then collapse ALL whitespace into single spaces so the
// result is one flat line. This is what goes into config.yaml, no HTML parser.
pub(crate) fn strip_html(html: &str) -> String {
    let mut text = String::with_capacity(html.len());
    let mut in_tag = false;
    for c in html.chars() {
        match c {
            '<' => in_tag = true,
            '>' => in_tag = false,
            _ if !in_tag => text.push(c),
            _ => {}
        }
    }
    let text = decode_entities(&text);
    text.split_whitespace().collect::<Vec<_>>().join(" ")
}


#[cfg(test)]
mod tests {
    use super::*;

    // The generated config.yaml has to match Cobalt's format exactly, a bad one panics the game.
    #[test]
    fn config_yaml_shape() {
        let config = ModConfig {
            id: "gamebanana.446023".into(),
            name: "Revival Stone Hell".into(),
            description: "harder mode".into(),
            author: "Rosado".into(),
            dependencies: Vec::new(),
            optional_dependencies: Vec::new(),
            source_url: Some("https://gamebanana.com/mods/446023".into()),
            update_sources: vec![UpdateSource::GameBanana {
                item_type: "Mod".into(),
                item_id: 446023,
            }],
        };
        let yaml = serde_yaml::to_string(&config).unwrap();
        // serde_yaml 0.9 (the exact version Cobalt uses) writes an externally-tagged enum as a YAML
        // tag, so update_sources entries look like `- !gamebanana`. This is what Cobalt reads back.
        assert!(yaml.contains("id: gamebanana.446023"));
        assert!(yaml.contains("update_sources:"));
        assert!(yaml.contains("!gamebanana"));
        assert!(yaml.contains("item_type: Mod"));
        assert!(yaml.contains("item_id: 446023"));
        // Empty lists are skipped, not emitted.
        assert!(!yaml.contains("dependencies:"));
        // And what we write must round-trip back into the lenient reader.
        let read: InstalledConfig = serde_yaml::from_str(&yaml).unwrap();
        assert!(matches!(
            read.update_sources.first(),
            Some(UpdateSource::GameBanana { item_id: 446023, .. })
        ));
    }

    #[test]
    fn unnest_wrapper_folder() {
        // Correctly laid out at the root, nothing to strip.
        let flat = vec!["patches/xml/Shop.xml".to_string()];
        assert_eq!(root_prefix(&flat), "");

        // One wrapper folder.
        let wrapped = vec![
            "My Cool Mod/".to_string(),
            "My Cool Mod/patches/xml/Shop.xml".to_string(),
        ];
        assert_eq!(root_prefix(&wrapped), "My Cool Mod/");

        // More than one intermediate directory, the case that used to slip through.
        let deep = vec!["My Mod v1.2/inner/patches/xml/Shop.xml".to_string()];
        assert_eq!(root_prefix(&deep), "My Mod v1.2/inner/");

        // Data/ works as a marker too, and matching is case-insensitive.
        let bundles = vec!["Wrapper/DATA/StreamingAssets/aa/Switch/x.bundle".to_string()];
        assert_eq!(root_prefix(&bundles), "Wrapper/");

        // A nested config.yaml marks the root even without patches/Data next to it in the same path.
        let cfg = vec![
            "pack/My Mod/config.yaml".to_string(),
            "pack/My Mod/patches/xml/Shop.xml".to_string(),
        ];
        assert_eq!(root_prefix(&cfg), "pack/My Mod/");

        // A nested folder that happens to be named "data" must not beat the real top-level root.
        let decoy = vec![
            "patches/scripts/data/foo.lua".to_string(),
            "patches/xml/Shop.xml".to_string(),
        ];
        assert_eq!(root_prefix(&decoy), "");

        // A plugin zip with only a .nro (no patches/Data/config) under a wrapper folder: the folder
        // holding the .nro is the root, so the wrapper gets stripped.
        let plugin = vec!["Skill Commands Plugin/skill_commands.nro".to_string()];
        assert_eq!(root_prefix(&plugin), "Skill Commands Plugin/");

        // A .nro already at the root, nothing to strip (case-insensitive on the extension).
        let flat_plugin = vec!["my_plugin.NRO".to_string()];
        assert_eq!(root_prefix(&flat_plugin), "");

        // A .nro buried under patches/ is just a data file, patches/ still marks the root.
        let nro_in_patches = vec!["Wrapper/patches/bin/thing.nro".to_string()];
        assert_eq!(root_prefix(&nro_in_patches), "Wrapper/");

        // No recognizable mod root, left alone (extracted as-is).
        let unknown = vec!["stuff/readme.txt".to_string()];
        assert_eq!(root_prefix(&unknown), "");
    }

    #[test]
    fn sanitize_folder_names() {
        assert_eq!(sanitize("Nice: Mod?"), "Nice_ Mod_");
        assert_eq!(sanitize("  ..  "), "mod");
    }

    #[test]
    fn strip_html_plain_text() {
        assert_eq!(
            strip_html("<p>Hello &amp; <b>welcome</b></p>"),
            "Hello & welcome"
        );
        // The flat version has no line breaks, block boundaries just disappear.
        assert_eq!(strip_html("<p>one</p> <p>two</p>"), "one two");
    }


    #[test]
    fn stamp_gamebanana_source_into_shipped_config() {
        let tmp = std::env::temp_dir().join(format!("cobalt_stamp_gb_test_{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&tmp);
        std::fs::create_dir_all(&tmp).unwrap();
        let cfg = tmp.join("config.yaml");

        // No update_sources at all: gains ours, keeps the modder's fields and id.
        std::fs::write(&cfg, "id: my-cool-mod\nname: Cool Mod\nauthor: Someone\n").unwrap();
        ensure_gamebanana_source(&tmp, 446023);
        let read: InstalledConfig =
            serde_yaml::from_str(&std::fs::read_to_string(&cfg).unwrap()).unwrap();
        assert_eq!(read.id, "my-cool-mod");
        assert_eq!(read.author, "Someone");
        assert!(matches!(
            read.update_sources.first(),
            Some(UpdateSource::GameBanana { item_id: 446023, .. })
        ));
        assert_eq!(classify_source(&read), ModSource::GameBanana(446023));

        // Already has a gamebanana source (any id): file is left byte-for-byte alone, comments and all.
        let theirs = "# hand-written\nid: another\nupdate_sources:\n- !gamebanana\n  item_type: Mod\n  item_id: 111\n";
        std::fs::write(&cfg, theirs).unwrap();
        ensure_gamebanana_source(&tmp, 446023);
        assert_eq!(std::fs::read_to_string(&cfg).unwrap(), theirs);

        // A github entry is kept and ours is appended after it.
        std::fs::write(
            &cfg,
            "id: plugin\nupdate_sources:\n- !github\n  user: doge\n  repo: cool-plugin\n",
        )
        .unwrap();
        ensure_gamebanana_source(&tmp, 446023);
        let read: InstalledConfig =
            serde_yaml::from_str(&std::fs::read_to_string(&cfg).unwrap()).unwrap();
        assert_eq!(read.update_sources.len(), 2);
        assert!(matches!(read.update_sources[0], UpdateSource::GitHub { .. }));
        assert!(matches!(
            read.update_sources[1],
            UpdateSource::GameBanana { item_id: 446023, .. }
        ));

        // A bare `update_sources:` key (parses as null) gets its first entry.
        std::fs::write(&cfg, "id: bare\nupdate_sources:\n").unwrap();
        ensure_gamebanana_source(&tmp, 446023);
        let read: InstalledConfig =
            serde_yaml::from_str(&std::fs::read_to_string(&cfg).unwrap()).unwrap();
        assert_eq!(read.update_sources.len(), 1);

        // Garbage we can't parse is never touched.
        let garbage = "id: [unclosed\n  ???";
        std::fs::write(&cfg, garbage).unwrap();
        ensure_gamebanana_source(&tmp, 446023);
        assert_eq!(std::fs::read_to_string(&cfg).unwrap(), garbage);

        let _ = std::fs::remove_dir_all(&tmp);
    }

    #[test]
    fn stamp_nexus_source_into_shipped_config() {
        let tmp = std::env::temp_dir().join(format!("cobalt_stamp_nx_test_{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&tmp);
        std::fs::create_dir_all(&tmp).unwrap();
        let cfg = tmp.join("config.yaml");
        let meta = NexusMeta {
            mod_id: 5150,
            name: "Cool Nexus Mod".into(),
            author: "Someone".into(),
            description: String::new(),
            source_url: "https://www.nexusmods.com/fireemblemengage/mods/5150".into(),
        };

        // Empty source_url: gains the page url, and the scanners recognize it from there.
        std::fs::write(&cfg, "id: their-id\nname: Cool Nexus Mod\n").unwrap();
        ensure_nexus_source(&tmp, &meta);
        let read: InstalledConfig =
            serde_yaml::from_str(&std::fs::read_to_string(&cfg).unwrap()).unwrap();
        assert_eq!(read.id, "their-id");
        assert_eq!(read.source_url, meta.source_url);
        assert_eq!(classify_source(&read), ModSource::Nexus(5150));

        // An existing source_url (a plugin's repo) is never overwritten.
        let theirs = "id: plugin\nsource_url: https://github.com/doge/cool-plugin\n";
        std::fs::write(&cfg, theirs).unwrap();
        ensure_nexus_source(&tmp, &meta);
        assert_eq!(std::fs::read_to_string(&cfg).unwrap(), theirs);

        let _ = std::fs::remove_dir_all(&tmp);
    }

    #[test]
    fn nexus_ids_from_urls() {
        assert_eq!(
            nexus_id_from_url("https://www.nexusmods.com/fireemblemengage/mods/5150"),
            Some(5150)
        );
        // Trailing junk after the id is fine, other hosts and shapes are not.
        assert_eq!(
            nexus_id_from_url("https://www.nexusmods.com/fireemblemengage/mods/5150?tab=files"),
            Some(5150)
        );
        assert_eq!(nexus_id_from_url("https://github.com/doge/mods/5150"), None);
        assert_eq!(nexus_id_from_url("https://www.nexusmods.com/fireemblemengage"), None);
        assert_eq!(nexus_id_from_url(""), None);
    }

    #[test]
    fn scan_finds_and_classifies_installed_mods() {
        // Lay out a fake engage/mods with one GameBanana mod, one Nexus mod, and one hand-dropped
        // folder with no config, then confirm scan reports all three with the right source.
        let tmp = std::env::temp_dir().join(format!("cobalt_scan_test_{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&tmp);
        let mods = mods_dir(&tmp);

        let gb = mods.join("Revival Stone Hell");
        std::fs::create_dir_all(&gb).unwrap();
        std::fs::write(
            gb.join("config.yaml"),
            "id: gamebanana.446023\nname: Revival Stone Hell\nauthor: Rosado\nupdate_sources:\n- !gamebanana\n  item_type: Mod\n  item_id: 446023\n",
        )
        .unwrap();

        let nx = mods.join("Cool Nexus Mod");
        std::fs::create_dir_all(&nx).unwrap();
        std::fs::write(
            nx.join("config.yaml"),
            "id: nexus.5150\nname: Cool Nexus Mod\nauthor: Someone\ndescription: A cool mod\n",
        )
        .unwrap();

        // Hand-dropped, no config at all.
        std::fs::create_dir_all(mods.join("MyHandMod").join("patches")).unwrap();

        let found = scan_installed_mods(&tmp);
        assert_eq!(found.len(), 3);
        // Sorted by name: "Cool Nexus Mod", "MyHandMod", "Revival Stone Hell".
        assert_eq!(found[0].source, ModSource::Nexus(5150));
        assert_eq!(found[0].author.as_deref(), Some("Someone"));
        assert_eq!(found[0].description.as_deref(), Some("A cool mod"));
        assert_eq!(found[1].name, "MyHandMod");
        assert_eq!(found[1].source, ModSource::Manual);
        assert!(!found[1].has_config);
        assert_eq!(found[1].description, None);
        assert_eq!(found[2].source, ModSource::GameBanana(446023));

        let _ = std::fs::remove_dir_all(&tmp);
    }

    #[test]
    fn scan_reads_config_out_of_a_zipped_mod() {
        // A zipped mod whose config.yaml sits under a wrapper folder, plus a deeper one from a
        // bundled sub-mod. The shallower config is the mod's own, so that's the one that wins.
        let mut zip = zip::ZipWriter::new(Cursor::new(Vec::new()));
        let opts = zip::write::SimpleFileOptions::default();
        zip.start_file("Wrapper/inner/config.yaml", opts).unwrap();
        zip.write_all(b"id: nexus.999\nname: Bundled Sub Mod\n").unwrap();
        zip.start_file("Wrapper/config.yaml", opts).unwrap();
        zip.write_all(
            b"id: gamebanana.446023\nname: Zipped Mod\nauthor: Rosado\ndescription: In a zip\n",
        )
        .unwrap();
        zip.start_file("Wrapper/patches/thing.bin", opts).unwrap();
        zip.write_all(b"data").unwrap();
        let bytes = zip.finish().unwrap().into_inner();

        let tmp = std::env::temp_dir().join(format!("cobalt_zipcfg_test_{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&tmp);
        let mods = mods_dir(&tmp);
        std::fs::create_dir_all(&mods).unwrap();
        std::fs::write(mods.join("Zipped Mod.zip"), &bytes).unwrap();

        let found = scan_installed_mods(&tmp);
        assert_eq!(found.len(), 1);
        assert_eq!(found[0].name, "Zipped Mod");
        assert_eq!(found[0].author.as_deref(), Some("Rosado"));
        assert_eq!(found[0].description.as_deref(), Some("In a zip"));
        assert_eq!(found[0].source, ModSource::GameBanana(446023));
        assert!(found[0].has_config);
        // The on-disk name stays the .zip itself, that's what we'd delete or open.
        assert_eq!(found[0].folder, "Zipped Mod.zip");

        // A zip with no config at all still lists, just as a manual mod.
        let mut plain = zip::ZipWriter::new(Cursor::new(Vec::new()));
        plain.start_file("patches/thing.bin", opts).unwrap();
        plain.write_all(b"data").unwrap();
        let plain = plain.finish().unwrap().into_inner();
        std::fs::write(mods.join("Plain.zip"), &plain).unwrap();

        // Sorted by name, so the config-less zip lands first.
        let found = scan_installed_mods(&tmp);
        assert_eq!(found.len(), 2);
        assert_eq!(found[0].name, "Plain.zip");
        assert_eq!(found[0].source, ModSource::Manual);
        assert!(!found[0].has_config);
        assert_eq!(found[1].name, "Zipped Mod");

        let _ = std::fs::remove_dir_all(&tmp);
    }

    #[test]
    fn thumbnail_write_probe_and_serve() {
        let tmp = std::env::temp_dir().join(format!("cobalt_thumb_test_{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&tmp);
        let mods = mods_dir(&tmp);

        // A mod folder that gets a thumbnail — awkward name on purpose (space, #, unicode).
        let with = mods.join("My #1 Mod ✨");
        std::fs::create_dir_all(&with).unwrap();
        std::fs::write(with.join("config.yaml"), "id: gamebanana.1\nname: Number One\n").unwrap();
        write_thumbnail(&with, b"png-bytes", "png");
        assert!(with.join("thumbnail.png").is_file());

        // A re-install that switches formats replaces the old file instead of leaving both.
        write_thumbnail(&with, b"jpg-bytes", "jpg");
        assert!(!with.join("thumbnail.png").exists());
        assert_eq!(std::fs::read(with.join("thumbnail.jpg")).unwrap(), b"jpg-bytes");

        // Unknown extensions clamp to jpg rather than ever writing a thumbnail.exe.
        write_thumbnail(&with, b"clamped", "exe");
        assert!(!with.join("thumbnail.exe").exists());
        assert_eq!(std::fs::read(with.join("thumbnail.jpg")).unwrap(), b"clamped");

        // A folder without one, and a zipped mod (nowhere to keep a thumbnail).
        std::fs::create_dir_all(mods.join("Bare")).unwrap();
        let mut plain = zip::ZipWriter::new(Cursor::new(Vec::new()));
        let opts = zip::write::SimpleFileOptions::default();
        plain.start_file("patches/thing.bin", opts).unwrap();
        plain.write_all(b"data").unwrap();
        let plain = plain.finish().unwrap().into_inner();
        std::fs::write(mods.join("Zipped.zip"), &plain).unwrap();

        // Scan reports the file name only where one exists.
        let found = scan_installed_mods(&tmp);
        assert_eq!(found.len(), 3);
        assert_eq!(found[0].name, "Bare");
        assert_eq!(found[0].thumb, None);
        assert_eq!(found[1].thumb.as_deref(), Some("thumbnail.jpg"));
        assert_eq!(found[2].name, "Zipped.zip");
        assert_eq!(found[2].thumb, None);

        // Serve-side resolution accepts exactly the real thumbnail...
        let served = thumb_file_path(&tmp, "My #1 Mod ✨", "thumbnail.jpg").unwrap();
        assert_eq!(served, with.join("thumbnail.jpg"));
        // ...and rejects traversal, other files, unknown extensions, and misses.
        assert!(thumb_file_path(&tmp, "..", "thumbnail.jpg").is_none());
        assert!(thumb_file_path(&tmp, "a/b", "thumbnail.jpg").is_none());
        assert!(thumb_file_path(&tmp, "a\\b", "thumbnail.jpg").is_none());
        assert!(thumb_file_path(&tmp, "", "thumbnail.jpg").is_none());
        assert!(thumb_file_path(&tmp, "My #1 Mod ✨", "config.yaml").is_none());
        assert!(thumb_file_path(&tmp, "My #1 Mod ✨", "thumbnail.exe").is_none());
        assert!(thumb_file_path(&tmp, "Bare", "thumbnail.jpg").is_none());

        let _ = std::fs::remove_dir_all(&tmp);
    }

    // Real end-to-end run against GameBanana: fetch a mod, download its file, install it into a temp
    // folder, and confirm the mod's files landed at the root and a valid config.yaml is there.
    // Ignored by default (needs network), run with: cargo test live_install_roundtrip -- --ignored
    #[test]
    #[ignore = "hits the GameBanana network"]
    fn live_install_roundtrip() {
        let rt = tokio::runtime::Runtime::new().unwrap();
        rt.block_on(async {
            let detail = crate::gamebanana::detail(446023).await.unwrap();
            let file = detail.files.first().expect("mod should have a file");
            let bytes = crate::gamebanana::download(&file.download_url, |_, _| {}).await.unwrap();

            let tmp = std::env::temp_dir().join(format!("cobalt_install_test_{}", std::process::id()));
            let _ = std::fs::remove_dir_all(&tmp);
            install_gamebanana_mod(&tmp, &detail, &bytes).unwrap();

            let mods = tmp.join("engage").join("mods");
            let dir = std::fs::read_dir(&mods).unwrap().next().unwrap().unwrap().path();
            // Un-nesting worked: the romfs-mirror tree is at the mod root, not one folder deep.
            assert!(dir.join("patches").exists() || dir.join("Data").exists(), "mod files at root");
            // A config.yaml exists and is valid YAML (Cobalt would panic on a broken one).
            let cfg = std::fs::read_to_string(dir.join("config.yaml")).unwrap();
            serde_yaml::from_str::<serde_yaml::Value>(&cfg).unwrap();

            let _ = std::fs::remove_dir_all(&tmp);
        });
    }
}
