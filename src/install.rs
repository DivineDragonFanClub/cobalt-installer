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
        if let Some(id) = config.id.strip_prefix("nexus.").and_then(|n| n.parse::<u64>().ok()) {
            map.insert(id, dir.clone());
        }
    }
    map
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
    // Total size on disk (folder contents, or the .zip file's size).
    pub size_bytes: u64,
    // Whether the mod carries a config.yaml at all. Hand-dropped mods often don't.
    pub has_config: bool,
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
    } else {
        ModSource::Manual
    }
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

        // Only folder mods can carry a config.yaml. A zipped mod is opaque, treat it as manual.
        let config = is_dir
            .then(|| std::fs::read_to_string(path.join("config.yaml")).ok())
            .flatten()
            .and_then(|text| serde_yaml::from_str::<InstalledConfig>(&text).ok());

        let (name, author, description, source) = match &config {
            Some(c) => {
                let name = if c.name.trim().is_empty() { folder.clone() } else { c.name.clone() };
                let author = (!c.author.trim().is_empty()).then(|| c.author.clone());
                let description = (!c.description.trim().is_empty()).then(|| c.description.clone());
                (name, author, description, classify_source(c))
            }
            None => (folder.clone(), None, None, ModSource::Manual),
        };

        let size_bytes = if is_dir {
            dir_size(&path)
        } else {
            std::fs::metadata(&path).map(|m| m.len()).unwrap_or(0)
        };

        mods.push(InstalledMod {
            folder,
            name,
            author,
            description,
            source,
            size_bytes,
            has_config: config.is_some(),
            path,
        });
    }
    mods.sort_by(|a, b| a.name.to_lowercase().cmp(&b.name.to_lowercase()));
    mods
}

// Total size of everything under a folder, walked recursively.
fn dir_size(path: &Path) -> u64 {
    let mut total = 0;
    let Ok(entries) = std::fs::read_dir(path) else {
        return 0;
    };
    for entry in entries.flatten() {
        let p = entry.path();
        if p.is_dir() {
            total += dir_size(&p);
        } else if let Ok(meta) = entry.metadata() {
            total += meta.len();
        }
    }
    total
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

// Install a downloaded GameBanana mod. `bytes` is the raw .zip we already fetched. Extracts into
// engage/mods/<name>/, replacing any earlier install of the same mod, then writes a config.yaml
// (recording the GameBanana source for auto-updates) when the mod doesn't already carry one.
pub fn install_gamebanana_mod(sd_root: &Path, detail: &ModDetail, bytes: &[u8]) -> Result<(), InstallError> {
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
    }

    Ok(())
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
    }

    Ok(())
}

// Find the mod's real root inside the archive and return the prefix to strip to reach it.
//
// Cobalt's mod root is the folder that directly holds patches/, Data/, or a config.yaml. Archives
// often bury that under one or more wrapper folders ("My Mod v1.2/", "My Mod/inner/", a doubled
// folder from a bad re-zip, ...). We locate the shallowest of those markers across all entries and
// strip everything above it, so any depth of intermediate directory is handled. A correctly laid
// out zip has the marker at depth 0 and nothing is stripped. Matching is case-insensitive since
// Cobalt lowercases paths anyway.
fn root_prefix(names: &[String]) -> String {
    let mut best: Option<String> = None;

    for name in names {
        if name.starts_with("__MACOSX/") {
            continue;
        }
        let comps: Vec<&str> = name.split('/').collect();

        // The shallowest component in this path that marks a mod root.
        let marker = comps.iter().position(|c| {
            let is_dir = c.eq_ignore_ascii_case("patches") || c.eq_ignore_ascii_case("Data");
            c.eq_ignore_ascii_case("config.yaml") || is_dir
        });

        let Some(i) = marker else { continue };
        // A config.yaml marker has to be the last component (it's a file). A patches/Data marker
        // has to have something under it (it's a directory in the path).
        let is_config = comps[i].eq_ignore_ascii_case("config.yaml");
        if is_config && i + 1 != comps.len() {
            continue;
        }
        if !is_config && i + 1 >= comps.len() {
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

// Same idea as strip_html but keeps the paragraph structure for on-screen display: block tags
// (<br>, </p>, </div>, list items, headings) become line breaks so the description doesn't render
// as one giant wall of text. Horizontal whitespace inside a line is still collapsed, and we cap
// runs of blank lines at one so the spacing stays tidy.
pub(crate) fn strip_html_display(html: &str) -> String {
    let mut text = String::with_capacity(html.len());
    let mut tag = String::new();
    let mut in_tag = false;
    for c in html.chars() {
        match c {
            '<' => {
                in_tag = true;
                tag.clear();
            }
            '>' => {
                in_tag = false;
                // Pull the tag name (drop a leading slash and anything after a space or slash).
                let closing = tag.starts_with('/');
                let name = tag
                    .trim_start_matches('/')
                    .split(|ch: char| ch.is_whitespace() || ch == '/')
                    .next()
                    .unwrap_or("")
                    .to_ascii_lowercase();
                // <br> breaks the line, and so does the end of any block-level element.
                let breaks = name == "br"
                    || (closing
                        && matches!(
                            name.as_str(),
                            "p" | "div"
                                | "li"
                                | "ul"
                                | "ol"
                                | "tr"
                                | "h1"
                                | "h2"
                                | "h3"
                                | "h4"
                                | "h5"
                                | "h6"
                                | "blockquote"
                                | "section"
                                | "header"
                        ));
                if breaks {
                    text.push('\n');
                }
            }
            _ if in_tag => tag.push(c),
            _ => text.push(c),
        }
    }
    let text = decode_entities(&text);

    // Collapse horizontal whitespace on each line, then drop leading/trailing blank lines and
    // squash any run of blank lines down to a single separator.
    let mut lines: Vec<String> = Vec::new();
    let mut blank_run = false;
    for line in text.split('\n') {
        let line = line.split_whitespace().collect::<Vec<_>>().join(" ");
        if line.is_empty() {
            // Only keep one blank line, and never one at the very start.
            if !lines.is_empty() {
                blank_run = true;
            }
        } else {
            if blank_run {
                lines.push(String::new());
            }
            blank_run = false;
            lines.push(line);
        }
    }
    lines.join("\n")
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
    fn strip_html_display_keeps_breaks() {
        // Paragraphs and <br> become newlines instead of being flattened to spaces.
        assert_eq!(
            strip_html_display("<p>First para.</p><p>Second para.</p>"),
            "First para.\nSecond para."
        );
        assert_eq!(
            strip_html_display("Line one<br>Line two<br/>Line three"),
            "Line one\nLine two\nLine three"
        );
        // Inline tags don't break, horizontal whitespace still collapses.
        assert_eq!(
            strip_html_display("<p>Hello &amp;   <b>welcome</b></p>"),
            "Hello & welcome"
        );
        // Runs of empty block tags don't pile up blank lines, and none leads.
        assert_eq!(
            strip_html_display("<div></div><p>only line</p><br><br>"),
            "only line"
        );
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
        // The config file has bytes, so the mod reports a non-zero size.
        assert!(found[0].size_bytes > 0);
        assert_eq!(found[1].name, "MyHandMod");
        assert_eq!(found[1].source, ModSource::Manual);
        assert!(!found[1].has_config);
        assert_eq!(found[1].description, None);
        assert_eq!(found[2].source, ModSource::GameBanana(446023));

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
