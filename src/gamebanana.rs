// GameBanana API client for the mod browser (desktop only).
//
// GameBanana's public API needs no key or auth. We use the newer apiv11 for browsing and
// searching (one request gives us a full page of records, no per-mod follow up) and the mod's
// ProfilePage for the detail view (name, description, screenshots, and the file list with its
// download links). Everything here is anonymous GETs.

use std::collections::{HashSet, VecDeque};
use std::sync::LazyLock;

use serde::Deserialize;

// Fire Emblem Engage's game id on GameBanana.
pub const GAME_ID: u64 = 17832;

// One shared client so we reuse connections and always send a real User-Agent (GameBanana is
// picky about anonymous clients with no UA).
static CLIENT: LazyLock<reqwest::Client> = LazyLock::new(|| {
    reqwest::Client::builder()
        .user_agent(concat!("CobaltInstaller/", env!("CARGO_PKG_VERSION")))
        .timeout(std::time::Duration::from_secs(30))
        .build()
        .expect("failed to build reqwest client")
});

// A preview image, as GameBanana hands it to us: a base url plus a few pre-rendered sizes.
#[derive(Deserialize, Clone, PartialEq, Debug)]
pub struct Image {
    #[serde(rename = "_sBaseUrl")]
    pub base_url: String,
    #[serde(rename = "_sFile")]
    pub file: String,
    #[serde(rename = "_sFile220", default)]
    pub file220: Option<String>,
    #[serde(rename = "_sFile530", default)]
    pub file530: Option<String>,
    // The modder's note on this screenshot, shown under it in the lightbox.
    #[serde(rename = "_sCaption", default)]
    pub caption: Option<String>,
}

impl Image {
    // A card-sized thumbnail, preferring the 530 then 220 render, falling back to the full image.
    pub fn thumb_url(&self) -> String {
        let file = self.file530.as_ref().or(self.file220.as_ref()).unwrap_or(&self.file);
        format!("{}/{}", self.base_url, file)
    }

    // The original upload, for the lightbox.
    pub fn full_url(&self) -> String {
        format!("{}/{}", self.base_url, self.file)
    }
}

#[derive(Deserialize, Clone, PartialEq, Debug, Default)]
pub struct PreviewMedia {
    #[serde(rename = "_aImages", default)]
    pub images: Vec<Image>,
}

#[derive(Deserialize, Clone, PartialEq, Debug)]
pub struct Submitter {
    #[serde(rename = "_sName")]
    pub name: String,
}

// The mod's top-level category (Skins, Gameplay, ...), shown as a chip on the card.
#[derive(Deserialize, Clone, PartialEq, Debug)]
pub struct RootCategory {
    #[serde(rename = "_sName", default)]
    pub name: String,
    #[serde(rename = "_sIconUrl", default)]
    pub icon_url: String,
}

// One row in a browse/search page. Browse (Subfeed) records are lighter than search records, so
// anything that only shows up in one of them is optional.
#[derive(Deserialize, Clone, PartialEq, Debug)]
pub struct Listing {
    #[serde(rename = "_idRow")]
    pub id: u64,
    // The submission type. GameBanana's game feed mixes in Requests/Questions/WiPs whose ids live in
    // a different id-space, so we keep only "Mod" (see mods_only). Fetching Mod/<id> for a non-Mod id
    // would silently open an unrelated mod from another game.
    #[serde(rename = "_sModelName", default)]
    pub model: String,
    #[serde(rename = "_sName")]
    pub name: String,
    #[serde(rename = "_sProfileUrl", default)]
    pub profile_url: String,
    #[serde(rename = "_aPreviewMedia", default)]
    pub preview: PreviewMedia,
    #[serde(rename = "_aSubmitter", default)]
    pub submitter: Option<Submitter>,
    // GameBanana flags mods that carry content ratings, we use it to blur the thumbnail.
    #[serde(rename = "_bHasContentRatings", default)]
    pub has_content_ratings: bool,
    #[serde(rename = "_bHasFiles", default)]
    pub has_files: bool,
    // Extra card info: likes, views, category, version. All present on both browse and search rows.
    #[serde(rename = "_nLikeCount", default)]
    pub likes: u64,
    #[serde(rename = "_nViewCount", default)]
    pub views: u64,
    #[serde(rename = "_aRootCategory", default)]
    pub category: Option<RootCategory>,
    #[serde(rename = "_sVersion", default)]
    pub version: String,
    #[serde(rename = "_bWasFeatured", default)]
    pub featured: bool,
}

impl Listing {
    pub fn thumb_url(&self) -> Option<String> {
        self.preview.images.first().map(Image::thumb_url)
    }

    pub fn author(&self) -> String {
        self.submitter.as_ref().map(|s| s.name.clone()).unwrap_or_else(|| "Unknown".into())
    }

    // The category name, if the mod has a non-empty one.
    pub fn category_name(&self) -> Option<String> {
        self.category.as_ref().map(|c| c.name.clone()).filter(|n| !n.is_empty())
    }
}

// A downloadable file attached to a mod. `_sDownloadUrl` is a redirect to the real archive,
// reqwest follows it for us.
#[derive(Deserialize, Clone, PartialEq, Debug)]
pub struct ModFile {
    #[serde(rename = "_idRow")]
    pub id: u64,
    #[serde(rename = "_sFile")]
    pub filename: String,
    #[serde(rename = "_nFilesize", default)]
    pub filesize: u64,
    #[serde(rename = "_sDownloadUrl")]
    pub download_url: String,
    #[serde(rename = "_sDescription", default)]
    pub description: String,
    // Unix time the file was uploaded. Modders keep superseded zips attached, so this is what
    // decides which file is "the latest" in the detail view.
    #[serde(rename = "_tsDateAdded", default)]
    pub date_added: u64,
    // The modder-typed version tag ("5.4", sometimes "classes only") — free text, NOT semver.
    // Files sharing a tag are one release's alternates and get grouped in the detail view.
    #[serde(rename = "_sVersion", default)]
    pub version: String,
}

// The full mod page: everything the detail view needs in one request.
#[derive(Deserialize, Clone, PartialEq, Debug)]
pub struct ModDetail {
    #[serde(rename = "_idRow")]
    pub id: u64,
    #[serde(rename = "_sName")]
    pub name: String,
    #[serde(rename = "_sProfileUrl", default)]
    pub profile_url: String,
    // The short tagline shown under the title on the site. Only ProfilePage returns it; the
    // browse/search list endpoints never include it, so cards can't have it.
    #[serde(rename = "_sDescription", default)]
    pub subtitle: Option<String>,
    // The long HTML description. We strip it to plain text before it ever hits a config file.
    #[serde(rename = "_sText", default)]
    pub description_html: String,
    #[serde(rename = "_aPreviewMedia", default)]
    pub preview: PreviewMedia,
    #[serde(rename = "_aFiles", default)]
    pub files: Vec<ModFile>,
    #[serde(rename = "_aSubmitter", default)]
    pub submitter: Option<Submitter>,
    #[serde(rename = "_sVersion", default)]
    pub version: String,
    #[serde(rename = "_nLikeCount", default)]
    pub likes: u64,
    #[serde(rename = "_nViewCount", default)]
    pub views: u64,
    #[serde(rename = "_tsDateAdded", default)]
    pub date_added: u64,
    #[serde(rename = "_tsDateModified", default)]
    pub date_modified: u64,
    // Other mods/tools this one needs, each a [name, url] pair. Kept as raw JSON because the shape
    // varies; we only read the url (second element) to spot GameBanana mod links. See
    // requirement_mod_ids / install_engage_requirements.
    #[serde(rename = "_aRequirements", default)]
    pub requirements: Vec<serde_json::Value>,
    // Which game this mod is for, so we can confirm a required mod is actually for Engage.
    #[serde(rename = "_aGame", default)]
    pub game: Option<GameRef>,
}

// Just the game id off a mod's `_aGame` object.
#[derive(Deserialize, Clone, PartialEq, Debug)]
pub struct GameRef {
    #[serde(rename = "_idRow", default)]
    pub id: u64,
}

impl ModDetail {
    pub fn author(&self) -> String {
        self.submitter.as_ref().map(|s| s.name.clone()).unwrap_or_else(|| "Unknown".into())
    }

    // The file to grab for a one-click install where the user doesn't pick one (the starter pack).
    // Modders keep old zips attached, so the newest upload is the safe default for the main release.
    pub fn primary_file(&self) -> Option<&ModFile> {
        self.files.iter().max_by_key(|f| f.date_added)
    }

    pub fn game_id(&self) -> Option<u64> {
        self.game.as_ref().map(|g| g.id)
    }

    // The GameBanana mod ids this mod lists as requirements (deduped, order kept). Requirement
    // entries that aren't GameBanana mod links (GitHub, tools, other sites) are dropped.
    pub fn requirement_mod_ids(&self) -> Vec<u64> {
        let mut ids = Vec::new();
        for entry in &self.requirements {
            if let Some(id) = entry.get(1).and_then(|v| v.as_str()).and_then(requirement_mod_id) {
                if !ids.contains(&id) {
                    ids.push(id);
                }
            }
        }
        ids
    }
}

// The mod id out of a GameBanana mod url like https://gamebanana.com/mods/574346, else None.
fn requirement_mod_id(url: &str) -> Option<u64> {
    let marker = "gamebanana.com/mods/";
    let start = url.find(marker)? + marker.len();
    let digits: String = url[start..].chars().take_while(|c| c.is_ascii_digit()).collect();
    digits.parse().ok()
}

// Work out the FE Engage GameBanana mods that `root` requires, following their requirements too, and
// return each one's detail plus the file to install. Nothing is downloaded or installed here (the
// caller does that so it can be transactional). Requirements that point at GitHub (Cobalt), a tool,
// or another game are skipped, as are mods already in `already_installed`. Tracks what it's seen so
// a requirement loop can't spin forever.
pub async fn resolve_engage_requirements(
    root: &ModDetail,
    already_installed: &HashSet<u64>,
) -> Vec<(ModDetail, ModFile)> {
    let mut out = Vec::new();
    let mut visited: HashSet<u64> = HashSet::new();
    visited.insert(root.id);
    let mut queue: VecDeque<u64> = root.requirement_mod_ids().into_iter().collect();

    while let Some(rid) = queue.pop_front() {
        if !visited.insert(rid) || already_installed.contains(&rid) {
            continue;
        }
        let Ok(req) = detail(rid).await else { continue };
        // Only auto-install actual Engage mods, never another game's mod or a tool page.
        if req.game_id() != Some(GAME_ID) {
            continue;
        }
        let Some(file) = req.primary_file().cloned() else { continue };
        // Follow this requirement's own requirements as well.
        for id in req.requirement_mod_ids() {
            queue.push_back(id);
        }
        out.push((req, file));
    }
    out
}

// A mod category for the filter dropdown (Gameplay, Skins, Map, ...).
#[derive(Deserialize, Clone, PartialEq, Debug)]
pub struct Category {
    #[serde(rename = "_idRow")]
    pub id: u64,
    #[serde(rename = "_sName")]
    pub name: String,
}

// The envelope browse/search/category listings come back in.
#[derive(Deserialize)]
struct Page {
    #[serde(rename = "_aRecords", default)]
    records: Vec<Listing>,
}

// GET a page of records and keep only actual mods. The `default`ed model on records from the search
// endpoint comes back empty, and those are already mods, so we treat empty as "Mod".
async fn fetch_listings(url: &str) -> anyhow::Result<Vec<Listing>> {
    let page: Page = CLIENT.get(url).send().await?.error_for_status()?.json().await?;
    Ok(page
        .records
        .into_iter()
        .filter(|l| l.model.is_empty() || l.model == "Mod")
        .collect())
}

// The sort orders the Mod/Index endpoint accepts, as (api value, label) pairs for the dropdown.
// First entry is the default. These only apply to browse/category listings, a text search sorts by
// relevance instead (the search endpoint ignores these keys).
pub const SORTS: &[(&str, &str)] = &[
    ("Generic_LatestModified", "Latest updated"),
    ("Generic_Newest", "Newest"),
    ("Generic_MostLiked", "Most liked"),
    ("Generic_MostDownloaded", "Most downloaded"),
];

// Browse the game's mods, one page at a time (15 per page), in the given sort order (a `_sSort`
// value from SORTS). Uses Mod/Index, not the game Subfeed, because the Subfeed also lists
// Requests/Questions/WiPs whose ids would open the wrong thing in the detail view.
pub async fn browse(page: u32, sort: &str) -> anyhow::Result<Vec<Listing>> {
    let url = format!(
        "https://gamebanana.com/apiv11/Mod/Index?_nPage={page}&_nPerpage=15&_sSort={sort}&_aFilters%5BGeneric_Game%5D={GAME_ID}"
    );
    fetch_listings(&url).await
}

// Search mods by text. GameBanana wants at least a couple characters to return anything useful.
pub async fn search(query: &str, page: u32) -> anyhow::Result<Vec<Listing>> {
    let q = urlencoding_encode(query);
    let url = format!(
        "https://gamebanana.com/apiv11/Util/Search/Results?_sSearchString={q}&_nPerpage=15&_nPage={page}&_idGameRow={GAME_ID}&_sModelName=Mod"
    );
    fetch_listings(&url).await
}

// The game's top-level mod categories, for the filter dropdown.
pub async fn categories() -> anyhow::Result<Vec<Category>> {
    let url = format!("https://gamebanana.com/apiv11/Mod/Categories?_idGameRow={GAME_ID}&_sSort=a_to_z&_nPerpage=50");
    let cats: Vec<Category> = CLIENT.get(&url).send().await?.error_for_status()?.json().await?;
    Ok(cats)
}

// Browse the mods in one category, in the given sort order. Uses the Mod/Index endpoint, which is
// the one that actually filters server-side (the Subfeed ignores a category filter).
pub async fn by_category(category_id: u64, page: u32, sort: &str) -> anyhow::Result<Vec<Listing>> {
    let url = format!(
        "https://gamebanana.com/apiv11/Mod/Index?_nPage={page}&_nPerpage=15&_sSort={sort}&_aFilters%5BGeneric_Game%5D={GAME_ID}&_aFilters%5BGeneric_Category%5D={category_id}"
    );
    fetch_listings(&url).await
}

// Fetch one mod's full page.
pub async fn detail(id: u64) -> anyhow::Result<ModDetail> {
    let url = format!("https://gamebanana.com/apiv11/Mod/{id}/ProfilePage");
    let detail: ModDetail = CLIENT.get(&url).send().await?.error_for_status()?.json().await?;
    Ok(detail)
}

// A hand-picked GameBanana collection we offer new users as a "starter pack" (a curated set of
// beginner-friendly mods). It's a real collection on the site, so re-curating the pack is just a
// matter of editing that collection, no new build needed. Change this id to point at a different one.
pub const STARTER_COLLECTION: u64 = 217538;

// The mods inside a GameBanana collection. Records come back in the same shape as a browse page, so
// one call gives us everything the starter-pack cards need. One page (15) is plenty for a pack.
pub async fn collection_items(collection_id: u64, page: u32) -> anyhow::Result<Vec<Listing>> {
    let url = format!(
        "https://gamebanana.com/apiv11/Collection/{collection_id}/Items?_nPage={page}&_nPerpage=15"
    );
    fetch_listings(&url).await
}

// Download an archive fully into memory, reporting (downloaded, total) bytes as it goes. Mods are
// small enough that holding the whole thing in memory is fine, and it keeps us off tokio::fs.
pub async fn download<F: FnMut(u64, u64)>(url: &str, mut on_progress: F) -> anyhow::Result<Vec<u8>> {
    use futures_util::StreamExt;

    let resp = CLIENT.get(url).send().await?.error_for_status()?;
    let total = resp.content_length().unwrap_or(0);
    let mut stream = resp.bytes_stream();
    let mut buf = Vec::with_capacity(total as usize);
    let mut downloaded = 0u64;

    while let Some(chunk) = stream.next().await {
        let chunk = chunk?;
        downloaded += chunk.len() as u64;
        buf.extend_from_slice(&chunk);
        on_progress(downloaded, total);
    }

    Ok(buf)
}

// Short human count for likes/views/downloads: 980, 1.2k, 65.2k, 1.1M. Drops a trailing ".0".
pub fn format_count(n: u64) -> String {
    let (val, suffix) = if n >= 1_000_000 {
        (n as f64 / 1_000_000.0, "M")
    } else if n >= 1_000 {
        (n as f64 / 1_000.0, "k")
    } else {
        return n.to_string();
    };
    let s = format!("{val:.1}");
    let s = s.strip_suffix(".0").map(String::from).unwrap_or(s);
    format!("{s}{suffix}")
}

// Format a GameBanana unix timestamp as "Jan 1, 2026" (UTC). Day-level metadata, so we skip
// timezone handling and a chrono dependency.
pub fn format_date(ts: u64) -> String {
    const MONTHS: [&str; 12] =
        ["Jan", "Feb", "Mar", "Apr", "May", "Jun", "Jul", "Aug", "Sep", "Oct", "Nov", "Dec"];
    let (y, m, d) = civil_from_days((ts / 86_400) as i64);
    format!("{} {}, {}", MONTHS[(m - 1) as usize], d, y)
}

// Days-since-epoch to (year, month, day), Howard Hinnant's civil-calendar algorithm.
fn civil_from_days(z: i64) -> (i64, u32, u32) {
    let z = z + 719_468;
    let era = if z >= 0 { z } else { z - 146_096 } / 146_097;
    let doe = (z - era * 146_097) as u64;
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = (doy - (153 * mp + 2) / 5 + 1) as u32;
    let m = if mp < 10 { mp + 3 } else { mp - 9 } as u32;
    let y = yoe as i64 + era * 400 + if m <= 2 { 1 } else { 0 };
    (y, m, d)
}

pub fn format_filesize(bytes: u64) -> String {
    if bytes >= 1_000_000 {
        format!("{:.1} MB", bytes as f64 / 1_000_000.0)
    } else if bytes >= 1_000 {
        format!("{:.0} KB", bytes as f64 / 1_000.0)
    } else {
        format!("{bytes} B")
    }
}

// Tiny percent-encoder for the search query so we don't pull in a crate just for this. Encodes
// everything that isn't an unreserved URL character.
fn urlencoding_encode(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for b in s.bytes() {
        match b {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => out.push(b as char),
            _ => out.push_str(&format!("%{b:02X}")),
        }
    }
    out
}

// ---- Description sanitizing -------------------------------------------------
//
// Mod descriptions are third-party HTML. Rather than flattening them to plain text (which kills
// the links modders rely on), the detail view renders a rebuilt subset: line breaks, links,
// simple emphasis, lists, and GameBanana's `<span class="GreenColor">`-style colored text.
// Nothing from the input passes through unexamined — unknown tags vanish (their text stays),
// attributes are reconstructed from scratch, hrefs must be http(s), and `<script>`/`<style>`
// lose their contents too. Text nodes keep their original entity encoding.

// The GameBanana description colors we map to theme colors in CSS. Anything else loses its class.
const GB_COLOR_CLASSES: [&str; 6] =
    ["GreenColor", "RedColor", "BlueColor", "OrangeColor", "YellowColor", "PurpleColor"];

// Tags that pass through as-is (no attributes) and participate in open/close tracking.
const PLAIN_TAGS: [&str; 9] = ["b", "strong", "i", "em", "u", "s", "p", "ul", "ol"];

pub fn sanitize_description(html: &str) -> String {
    let mut out = String::with_capacity(html.len());
    let mut stack: Vec<String> = Vec::new();
    let mut rest = html;

    while let Some(lt) = rest.find('<') {
        let (text, tail) = rest.split_at(lt);
        out.push_str(text);
        let Some(gt) = tail.find('>') else {
            // Dangling "<" with no close: drop the remainder rather than emit half a tag.
            rest = "";
            break;
        };
        let token = &tail[1..gt];
        rest = &tail[gt + 1..];

        let closing = token.starts_with('/');
        let token = token.trim_start_matches('/');
        let name_end = token
            .find(|c: char| !c.is_ascii_alphanumeric())
            .unwrap_or(token.len());
        let name = token[..name_end].to_ascii_lowercase();
        let attrs = &token[name_end..];

        match name.as_str() {
            "br" => out.push_str("<br>"),
            "li" if closing => close_up_to(&mut stack, &mut out, "li"),
            "li" => {
                out.push_str("<li>");
                stack.push("li".into());
            }
            // Their contents must never render as text.
            "script" | "style" if !closing => {
                let close = format!("</{name}");
                match rest.to_ascii_lowercase().find(&close) {
                    Some(pos) => {
                        let after = &rest[pos..];
                        rest = match after.find('>') {
                            Some(g) => &after[g + 1..],
                            None => "",
                        };
                    }
                    None => rest = "",
                }
            }
            "a" if closing => close_up_to(&mut stack, &mut out, "a"),
            "a" => {
                if let Some(href) = attr_value(attrs, "href") {
                    let lower = href.to_ascii_lowercase();
                    if lower.starts_with("https://") || lower.starts_with("http://") {
                        let safe = href.replace('"', "&quot;").replace('<', "&lt;");
                        out.push_str(&format!(
                            "<a href=\"{safe}\" target=\"_blank\" rel=\"noopener noreferrer\">"
                        ));
                        stack.push("a".into());
                    }
                }
                // No usable href: the tag vanishes, its text keeps flowing.
            }
            "span" if closing => close_up_to(&mut stack, &mut out, "span"),
            "span" => {
                let class = attr_value(attrs, "class")
                    .filter(|c| GB_COLOR_CLASSES.contains(&c.trim()));
                match class {
                    Some(c) => out.push_str(&format!("<span class=\"{}\">", c.trim())),
                    None => out.push_str("<span>"),
                }
                stack.push("span".into());
            }
            n if PLAIN_TAGS.contains(&n) => {
                if closing {
                    close_up_to(&mut stack, &mut out, n);
                } else {
                    out.push_str(&format!("<{n}>"));
                    stack.push(n.to_string());
                }
            }
            // Everything else (img, iframe, div, table, ...) is dropped; its inner text survives
            // because only the tag token is consumed.
            _ => {}
        }
    }
    out.push_str(rest);

    while let Some(name) = stack.pop() {
        out.push_str(&format!("</{name}>"));
    }
    out
}

// Close `name` if it's open, emitting closes for anything opened inside it on the way (HTML in
// the wild interleaves tags freely; the webview only gets balanced output).
fn close_up_to(stack: &mut Vec<String>, out: &mut String, name: &str) {
    if !stack.iter().any(|n| n == name) {
        return;
    }
    while let Some(top) = stack.pop() {
        out.push_str(&format!("</{top}>"));
        if top == name {
            break;
        }
    }
}

// Pull one attribute's value out of a tag's attribute blob, quoted or bare. Case-insensitive on
// the name; the value keeps its source entity encoding.
fn attr_value<'a>(attrs: &'a str, name: &str) -> Option<&'a str> {
    let lower = attrs.to_ascii_lowercase();
    let mut from = 0;
    while let Some(found) = lower[from..].find(name) {
        let at = from + found;
        // Must be a standalone attribute name followed by '='.
        let before_ok = at == 0 || !lower.as_bytes()[at - 1].is_ascii_alphanumeric();
        let after = &attrs[at + name.len()..];
        let after_eq = after.trim_start();
        if before_ok && after_eq.starts_with('=') {
            let value = after_eq[1..].trim_start();
            return Some(match value.as_bytes().first() {
                Some(&q @ (b'"' | b'\'')) => {
                    let inner = &value[1..];
                    &inner[..inner.find(q as char).unwrap_or(inner.len())]
                }
                _ => &value[..value
                    .find(|c: char| c.is_ascii_whitespace())
                    .unwrap_or(value.len())],
            });
        }
        from = at + name.len();
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sanitize_keeps_links_breaks_and_colors() {
        let input = "<span class=\"GreenColor\">This is a Cobalt mod, please \
            <a href=\"https://github.com/Raytwo/Cobalt#cobalt\" target=\"_blank\">install it</a> \
            to use this!</span><br><br>Thanks &amp; enjoy.";
        let out = sanitize_description(input);
        assert!(out.contains("<span class=\"GreenColor\">"));
        assert!(out.contains(
            "<a href=\"https://github.com/Raytwo/Cobalt#cobalt\" target=\"_blank\" rel=\"noopener noreferrer\">"
        ));
        assert!(out.contains("install it</a>"));
        assert!(out.contains("<br><br>"));
        // Entities pass through untouched.
        assert!(out.contains("Thanks &amp; enjoy."));
    }

    #[test]
    fn sanitize_defuses_hostile_input() {
        let out = sanitize_description(
            "<script>alert(1)</script>Hi \
             <a href=\"javascript:boom()\" onclick=\"p()\">link</a> \
             <img src=x onerror=steal()> <div style=\"position:fixed\">boxed</div>",
        );
        assert!(!out.contains("script"));
        assert!(!out.contains("alert"));
        assert!(!out.contains("onclick"));
        assert!(!out.contains("onerror"));
        assert!(!out.contains("javascript"));
        assert!(!out.contains("<img"));
        assert!(!out.contains("<div"));
        // The readable text survives it all.
        assert!(out.contains("Hi"));
        assert!(out.contains("link"));
        assert!(out.contains("boxed"));
    }

    #[test]
    fn sanitize_balances_and_filters() {
        // Unclosed tags get closed at the end.
        assert_eq!(sanitize_description("<b>bold"), "<b>bold</b>");
        // Unknown span classes drop to a bare span.
        assert_eq!(
            sanitize_description("<span class=\"SparklyColor\">x</span>"),
            "<span>x</span>"
        );
        // A stray close with no open is ignored.
        assert_eq!(sanitize_description("plain</a> text"), "plain text");
        // Lists come through.
        assert_eq!(
            sanitize_description("<ul><li>one</li><li>two</li></ul>"),
            "<ul><li>one</li><li>two</li></ul>"
        );
    }
}
