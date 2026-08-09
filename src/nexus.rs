// NexusMods API client for the mod browser (desktop only).
//
// Unlike GameBanana, NexusMods needs the user authenticated. This module speaks the v1 REST API,
// which takes either a personal API key (the `apikey` header, self-serve from the account page) or
// an OAuth bearer token. For now the UI uses the API key path, OAuth support layers on later using
// the same `Auth` type.
//
// The v1 API has no full-text search, so browsing is "latest added" / "trending" lists plus looking
// a mod up by id. Fire Emblem Engage only has a few dozen mods on Nexus, so that covers it.

use std::sync::LazyLock;

use serde::Deserialize;

pub const GAME_DOMAIN: &str = "fireemblemengage";
const GAME_ID: u64 = 5939;
const API_BASE: &str = "https://api.nexusmods.com/v1";
// The v2 GraphQL API. Unlike v1 it lists/searches the whole catalog, and browsing it needs no auth.
const GRAPHQL_URL: &str = "https://api.nexusmods.com/v2/graphql";

static CLIENT: LazyLock<reqwest::Client> = LazyLock::new(|| {
    reqwest::Client::builder()
        .user_agent(concat!("CobaltInstaller/", env!("CARGO_PKG_VERSION")))
        .timeout(std::time::Duration::from_secs(30))
        .build()
        .expect("failed to build reqwest client")
});

// How we authenticate to Nexus. API key today, OAuth bearer once that's wired up.
#[derive(Clone, PartialEq, Debug)]
pub enum Auth {
    ApiKey(String),
    // Wired up when OAuth lands (stage B), the download API already accepts a bearer token.
    #[allow(dead_code)]
    Bearer(String),
}

// Build a GET with the auth header Nexus expects, plus the app-identifying headers it asks for.
fn get(path: &str, auth: &Auth) -> reqwest::RequestBuilder {
    let builder = CLIENT
        .get(format!("{API_BASE}{path}"))
        .header("Application-Name", "CobaltInstaller")
        .header("Application-Version", env!("CARGO_PKG_VERSION"))
        .header("Accept", "application/json");
    match auth {
        Auth::ApiKey(key) => builder.header("apikey", key),
        Auth::Bearer(token) => builder.bearer_auth(token),
    }
}

#[derive(Deserialize, Clone, PartialEq, Debug)]
pub struct NexusUser {
    pub user_id: u64,
    pub name: String,
    #[serde(default)]
    pub is_premium: bool,
}

// Confirm an API key works and tell us who it belongs to (and whether they're premium, which decides
// if direct downloads are allowed).
pub async fn validate(auth: &Auth) -> anyhow::Result<NexusUser> {
    let user = get("/users/validate.json", auth).send().await?.error_for_status()?.json().await?;
    Ok(user)
}

#[derive(Deserialize, Clone, PartialEq, Debug)]
pub struct NexusMod {
    pub mod_id: u64,
    #[serde(default)]
    pub name: String,
    #[serde(default)]
    pub summary: String,
    #[serde(default)]
    pub description: String,
    #[serde(default)]
    pub author: String,
    #[serde(default)]
    pub uploaded_by: String,
    #[serde(default)]
    pub version: String,
    #[serde(default)]
    pub picture_url: Option<String>,
    #[serde(default)]
    pub contains_adult_content: bool,
    // Hidden/withheld mods come back with available=false, we filter those out of listings.
    #[serde(default = "yes")]
    pub available: bool,
}

fn yes() -> bool {
    true
}

impl NexusMod {
    // The credited author, falling back to the uploader's name.
    pub fn author(&self) -> String {
        if self.author.is_empty() {
            self.uploaded_by.clone()
        } else {
            self.author.clone()
        }
    }
}

pub fn mod_page_url(mod_id: u64) -> String {
    format!("https://www.nexusmods.com/{GAME_DOMAIN}/mods/{mod_id}")
}

// One page of search results, plus the total match count so the UI knows if there's more.
pub struct SearchPage {
    pub mods: Vec<NexusMod>,
    pub total: u32,
}

// Browse/search the whole catalog via GraphQL. Empty query = all mods (sorted by endorsements).
// A non-empty query does a case-insensitive substring match on the mod name. No auth needed.
pub async fn search_mods(query: &str, offset: u32, count: u32) -> anyhow::Result<SearchPage> {
    let name_filter = if query.trim().is_empty() {
        String::new()
    } else {
        // JSON-encode the term so it's a safe GraphQL string literal (the WILDCARD op does a plain
        // substring match, wildcard characters actually break it).
        let term = serde_json::to_string(query.trim())?;
        format!(", name:[{{value:{term},op:WILDCARD}}]")
    };

    let gql = format!(
        "query{{ mods(filter:{{gameId:[{{value:\"{GAME_ID}\",op:EQUALS}}]{name_filter}}}, \
         sort:[{{endorsements:{{direction:DESC}}}}], offset:{offset}, count:{count}){{ \
         totalCount nodes{{ modId name summary author uploader{{name}} version adultContent pictureUrl }} }} }}"
    );

    let resp: GqlResponse = CLIENT
        .post(GRAPHQL_URL)
        .json(&serde_json::json!({ "query": gql }))
        .send()
        .await?
        .error_for_status()?
        .json()
        .await?;

    if let Some(err) = resp.errors.into_iter().next() {
        anyhow::bail!("NexusMods search error: {}", err.message);
    }
    let mods = resp.data.map(|d| d.mods).ok_or_else(|| anyhow::anyhow!("NexusMods returned no data"))?;
    Ok(SearchPage {
        total: mods.total_count,
        mods: mods.nodes.into_iter().map(NexusMod::from).collect(),
    })
}

#[derive(Deserialize)]
struct GqlResponse {
    #[serde(default)]
    data: Option<GqlData>,
    #[serde(default)]
    errors: Vec<GqlError>,
}

#[derive(Deserialize)]
struct GqlError {
    message: String,
}

#[derive(Deserialize)]
struct GqlData {
    mods: GqlMods,
}

#[derive(Deserialize)]
struct GqlMods {
    #[serde(rename = "totalCount")]
    total_count: u32,
    nodes: Vec<GqlMod>,
}

#[derive(Deserialize)]
struct GqlMod {
    #[serde(rename = "modId")]
    mod_id: u64,
    #[serde(default)]
    name: String,
    #[serde(default)]
    summary: String,
    #[serde(default)]
    author: String,
    #[serde(default)]
    uploader: Option<GqlUploader>,
    #[serde(default)]
    version: String,
    #[serde(rename = "adultContent", default)]
    adult_content: bool,
    #[serde(rename = "pictureUrl", default)]
    picture_url: Option<String>,
}

#[derive(Deserialize)]
struct GqlUploader {
    name: String,
}

impl From<GqlMod> for NexusMod {
    fn from(g: GqlMod) -> Self {
        NexusMod {
            mod_id: g.mod_id,
            name: g.name,
            summary: g.summary,
            description: String::new(),
            author: g.author,
            uploaded_by: g.uploader.map(|u| u.name).unwrap_or_default(),
            version: g.version,
            picture_url: g.picture_url,
            contains_adult_content: g.adult_content,
            available: true,
        }
    }
}

#[derive(Deserialize, Clone, PartialEq, Debug)]
pub struct NexusFile {
    pub file_id: u64,
    #[serde(default)]
    pub name: String,
    #[serde(default)]
    pub version: String,
    #[serde(default)]
    pub size_kb: u64,
    #[serde(default)]
    pub category_name: Option<String>,
    #[serde(default)]
    pub is_primary: bool,
}

impl NexusFile {
    pub fn size_bytes(&self) -> u64 {
        self.size_kb * 1024
    }
}

// Fetch one mod's metadata by id (v1, needs the key). Used by the nxm:// flow, where all we get
// from the link is the mod/file ids, so we look up the name/author to build the config.
pub async fn mod_info(auth: &Auth, mod_id: u64) -> anyhow::Result<NexusMod> {
    let m = get(&format!("/games/{GAME_DOMAIN}/mods/{mod_id}.json"), auth)
        .send()
        .await?
        .error_for_status()?
        .json()
        .await?;
    Ok(m)
}

#[derive(Deserialize)]
struct FilesResponse {
    #[serde(default)]
    files: Vec<NexusFile>,
}

pub async fn mod_files(auth: &Auth, mod_id: u64) -> anyhow::Result<Vec<NexusFile>> {
    let resp: FilesResponse = get(&format!("/games/{GAME_DOMAIN}/mods/{mod_id}/files.json"), auth)
        .send()
        .await?
        .error_for_status()?
        .json()
        .await?;
    // Nexus lists old/archived files too, drop the ones with no category (those are removed files).
    Ok(resp.files.into_iter().filter(|f| f.category_name.is_some()).collect())
}

#[derive(Deserialize)]
struct DownloadLink {
    #[serde(rename = "URI")]
    uri: String,
}

// Turn a mod file into an actual CDN download URL. Premium accounts can call this directly, free
// accounts must pass the key+expires that come from an nxm:// link (the website Mod Manager button).
pub async fn download_link(
    auth: &Auth,
    mod_id: u64,
    file_id: u64,
    key: Option<&str>,
    expires: Option<&str>,
) -> anyhow::Result<String> {
    let mut path = format!("/games/{GAME_DOMAIN}/mods/{mod_id}/files/{file_id}/download_link.json");
    if let (Some(k), Some(e)) = (key, expires) {
        path.push_str(&format!("?key={k}&expires={e}"));
    }

    let resp = get(&path, auth).send().await?;
    if resp.status() == reqwest::StatusCode::FORBIDDEN {
        anyhow::bail!(
            "NexusMods only allows one-click downloads for Premium accounts. On a free account, use \"Install from downloaded file\" below after downloading the mod from its page."
        );
    }

    let links: Vec<DownloadLink> = resp.error_for_status()?.json().await?;
    links
        .into_iter()
        .next()
        .map(|l| l.uri)
        .ok_or_else(|| anyhow::anyhow!("NexusMods returned no download link"))
}

// A parsed nxm:// download link. The website's "Mod Manager Download" button mints these, carrying
// the one-time key+expires that let even a free account fetch the real download URL.
#[derive(Debug, Clone, PartialEq)]
pub struct NxmLink {
    pub game_domain: String,
    pub mod_id: u64,
    pub file_id: u64,
    pub key: String,
    pub expires: String,
}

// Parse nxm://<game>/mods/<modId>/files/<fileId>?key=<key>&expires=<expires>&user_id=<uid>.
pub fn parse_nxm(input: &str) -> anyhow::Result<NxmLink> {
    let rest = input
        .trim()
        .strip_prefix("nxm://")
        .ok_or_else(|| anyhow::anyhow!("that's not an nxm:// link"))?;
    let (path, query) = rest.split_once('?').unwrap_or((rest, ""));
    let parts: Vec<&str> = path.split('/').collect();
    if parts.len() < 5 || parts[1] != "mods" || parts[3] != "files" {
        anyhow::bail!("this nxm link isn't in the expected format");
    }

    let game_domain = parts[0].to_string();
    let mod_id = parts[2].parse().map_err(|_| anyhow::anyhow!("couldn't read the mod id from the link"))?;
    let file_id = parts[4].parse().map_err(|_| anyhow::anyhow!("couldn't read the file id from the link"))?;

    let mut key = String::new();
    let mut expires = String::new();
    for kv in query.split('&') {
        if let Some(v) = kv.strip_prefix("key=") {
            key = v.to_string();
        } else if let Some(v) = kv.strip_prefix("expires=") {
            expires = v.to_string();
        }
    }
    if key.is_empty() || expires.is_empty() {
        anyhow::bail!("this link has no download key, use the \"Mod Manager Download\" button on the mod page");
    }

    Ok(NxmLink { game_domain, mod_id, file_id, key, expires })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_valid_nxm() {
        let link = parse_nxm("nxm://fireemblemengage/mods/22/files/60?key=abc123&expires=1699&user_id=5").unwrap();
        assert_eq!(link.game_domain, "fireemblemengage");
        assert_eq!(link.mod_id, 22);
        assert_eq!(link.file_id, 60);
        assert_eq!(link.key, "abc123");
        assert_eq!(link.expires, "1699");
    }

    #[test]
    fn reject_bad_nxm() {
        // Not an nxm link.
        assert!(parse_nxm("https://example.com/mods/1").is_err());
        // Missing the download key (a plain deep link, not a Mod Manager one).
        assert!(parse_nxm("nxm://fireemblemengage/mods/22/files/60").is_err());
        // Wrong shape.
        assert!(parse_nxm("nxm://fireemblemengage/mods/22").is_err());
    }
}

// Download a file (a plain CDN URL, no auth needed) fully into memory, reporting progress.
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
