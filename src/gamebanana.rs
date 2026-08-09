// GameBanana API client for the mod browser (desktop only).
//
// GameBanana's public API needs no key or auth. We use the newer apiv11 for browsing and
// searching (one request gives us a full page of records, no per-mod follow up) and the mod's
// ProfilePage for the detail view (name, description, screenshots, and the file list with its
// download links). Everything here is anonymous GETs.

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
}

impl Image {
    // A card-sized thumbnail, preferring the 530 then 220 render, falling back to the full image.
    pub fn thumb_url(&self) -> String {
        let file = self.file530.as_ref().or(self.file220.as_ref()).unwrap_or(&self.file);
        format!("{}/{}", self.base_url, file)
    }

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
}

impl ModDetail {
    pub fn author(&self) -> String {
        self.submitter.as_ref().map(|s| s.name.clone()).unwrap_or_else(|| "Unknown".into())
    }
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

// Browse the newest mods for the game, one page at a time (15 per page). Uses Mod/Index, not the
// game Subfeed, because the Subfeed also lists Requests/Questions/WiPs whose ids would open the
// wrong thing in the detail view.
pub async fn browse(page: u32) -> anyhow::Result<Vec<Listing>> {
    let url = format!(
        "https://gamebanana.com/apiv11/Mod/Index?_nPage={page}&_nPerpage=15&_sSort=Generic_LatestModified&_aFilters%5BGeneric_Game%5D={GAME_ID}"
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

// Browse the newest mods in one category. Uses the Mod/Index endpoint, which is the one that
// actually filters server-side (the Subfeed ignores a category filter).
pub async fn by_category(category_id: u64, page: u32) -> anyhow::Result<Vec<Listing>> {
    let url = format!(
        "https://gamebanana.com/apiv11/Mod/Index?_nPage={page}&_nPerpage=15&_sSort=Generic_LatestModified&_aFilters%5BGeneric_Game%5D={GAME_ID}&_aFilters%5BGeneric_Category%5D={category_id}"
    );
    fetch_listings(&url).await
}

// Fetch one mod's full page.
pub async fn detail(id: u64) -> anyhow::Result<ModDetail> {
    let url = format!("https://gamebanana.com/apiv11/Mod/{id}/ProfilePage");
    let detail: ModDetail = CLIENT.get(&url).send().await?.error_for_status()?.json().await?;
    Ok(detail)
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
