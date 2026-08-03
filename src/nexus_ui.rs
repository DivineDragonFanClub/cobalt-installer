// The NexusMods mod browser UI (desktop only).
//
// Nexus needs the user signed in, so this starts with an API-key setup step (OAuth comes later),
// then browses the Latest / Trending feeds (the v1 API has no text search) and installs the same
// way GameBanana mods do. The download step differs: we ask Nexus for a CDN link first, which only
// works directly for Premium accounts (free accounts get a clear message pointing at the website).

use std::collections::HashSet;
use std::path::PathBuf;

use dioxus::prelude::*;
use dioxus_sdk::storage::{use_storage, LocalStorage};

use crate::gamebanana::format_filesize;
use crate::install;
use crate::mods_ui::{SkeletonCard, Spinner};
use crate::nexus::{self, Auth, NexusFile, NexusMod};

#[component]
pub fn NexusBrowser(sd_root: PathBuf) -> Element {
    let mut apikey = use_storage::<LocalStorage, String>("nexus_apikey".into(), String::new);

    // Validate the stored key (and re-validate if it changes). The outer Option is "still loading",
    // the inner Option is "no key entered yet".
    let user = use_resource(move || {
        let key = apikey();
        async move {
            let key = key.trim().to_string();
            if key.is_empty() {
                return None;
            }
            Some(nexus::validate(&Auth::ApiKey(key)).await)
        }
    });

    rsx! {
        div { id: "nexus_browser",
            match &*user.read() {
                Some(Some(Ok(u))) => rsx! {
                    div { class: "nexus_userbar",
                        span { "Signed in as {u.name}" }
                        if u.is_premium {
                            span { class: "premium_badge", "Premium" }
                        } else {
                            span { class: "muted", "Free account" }
                        }
                        button { class: "secondary", onclick: move |_| apikey.set(String::new()), "Sign out" }
                    }
                    NexusList { sd_root: sd_root.clone(), apikey: apikey() }
                },
                Some(Some(Err(e))) => rsx! {
                    NexusSetup { apikey, error: Some(e.to_string()) }
                },
                Some(None) => rsx! {
                    NexusSetup { apikey, error: None }
                },
                None => rsx! {
                    div { class: "mod_message", "Checking your NexusMods account…" }
                },
            }
        }
    }
}

#[component]
fn NexusSetup(mut apikey: Signal<String>, error: Option<String>) -> Element {
    let mut draft = use_signal(String::new);
    rsx! {
        div { class: "nexus_setup message_zone second",
            div { "Connect your NexusMods account to browse and install Nexus mods." }
            if let Some(e) = error {
                div { class: "mod_message error", "That key didn't work: {e}" }
            }
            div { class: "note",
                "Paste your personal API key from your NexusMods account (Site Preferences → API Access)."
            }
            a {
                class: "mod_page_link",
                href: "https://www.nexusmods.com/users/myaccount?tab=api%20access",
                "Open your NexusMods API keys page"
            }
            div { class: "nexus_key_row",
                input {
                    r#type: "password",
                    placeholder: "Personal API key",
                    value: "{draft}",
                    oninput: move |e| draft.set(e.value()),
                }
                button {
                    class: "primary",
                    disabled: draft().trim().is_empty(),
                    onclick: move |_| apikey.set(draft().trim().to_string()),
                    "Connect"
                }
            }
            div { class: "note", "Sign in with NexusMods (OAuth) is coming soon." }
        }
    }
}

#[component]
fn NexusList(sd_root: PathBuf, apikey: String) -> Element {
    // How many mods to pull per page from the GraphQL catalog.
    const PAGE: u32 = 20;

    let mut query = use_signal(String::new);
    let mut show_nsfw = use_signal(|| false);
    let mut selected = use_signal(|| None::<NexusMod>);

    // Results grow as the user hits "Load more". `total` is the full match count from Nexus.
    let mut results = use_signal(Vec::<NexusMod>::new);
    let mut loaded = use_signal(|| 0u32);
    let mut total = use_signal(|| 0u32);
    let mut loading = use_signal(|| false);
    let mut error = use_signal(|| None::<String>);

    let installed_root = sd_root.clone();
    let installed = use_signal(move || {
        install::installed_nexus_ids(&installed_root)
            .into_keys()
            .collect::<HashSet<u64>>()
    });

    // Reload from the top whenever the search text changes.
    use_effect(move || {
        let q = query().trim().to_string();
        results.set(Vec::new());
        loaded.set(0);
        total.set(0);
        error.set(None);
        loading.set(true);
        spawn(async move {
            match nexus::search_mods(&q, 0, PAGE).await {
                Ok(page) => {
                    total.set(page.total);
                    loaded.set(page.mods.len() as u32);
                    results.set(page.mods);
                }
                Err(e) => error.set(Some(e.to_string())),
            }
            loading.set(false);
        });
    });

    let load_more = move |_| {
        if loading() {
            return;
        }
        let q = query().trim().to_string();
        let offset = loaded();
        loading.set(true);
        spawn(async move {
            match nexus::search_mods(&q, offset, PAGE).await {
                Ok(page) => {
                    total.set(page.total);
                    loaded.set(loaded() + page.mods.len() as u32);
                    results.write().extend(page.mods);
                }
                Err(e) => error.set(Some(e.to_string())),
            }
            loading.set(false);
        });
    };

    rsx! {
        div { class: "mod_search_bar",
            input {
                r#type: "text",
                placeholder: "Search NexusMods…",
                value: "{query}",
                oninput: move |e| query.set(e.value()),
            }
            label { class: "nsfw_toggle",
                input {
                    r#type: "checkbox",
                    checked: show_nsfw(),
                    onchange: move |e| show_nsfw.set(e.checked()),
                }
                "Show sensitive content"
            }
        }

        if let Some(e) = error() {
            div { class: "mod_message error", "Couldn't load mods: {e}" }
        }

        if !results().is_empty() {
            div { class: "mod_grid",
                for m in results() {
                    {
                        let card = m.clone();
                        let id = m.mod_id;
                        rsx! {
                            NexusCard {
                                key: "{id}",
                                m: m.clone(),
                                installed: installed().contains(&id),
                                show_nsfw: show_nsfw(),
                                on_open: move |_| selected.set(Some(card.clone())),
                            }
                        }
                    }
                }
            }
        } else if loading() {
            div { class: "mod_grid",
                for i in 0..12 {
                    SkeletonCard { key: "{i}" }
                }
            }
        }

        div { class: "mod_pager",
            if loading() && !results().is_empty() {
                button { class: "secondary", disabled: true,
                    Spinner {}
                    "Loading…"
                }
            } else if !results().is_empty() && loaded() < total() {
                button { class: "secondary", onclick: load_more, "Load more ({loaded}/{total})" }
            } else if results().is_empty() && !loading() && error().is_none() {
                span { class: "mod_message", "No mods found." }
            }
        }

        if let Some(m) = selected() {
            NexusDetailPanel {
                key: "{m.mod_id}",
                info: m.clone(),
                apikey: apikey.clone(),
                sd_root: sd_root.clone(),
                installed,
                on_close: move |_| selected.set(None),
            }
        }
    }
}

#[component]
fn NexusCard(m: NexusMod, installed: bool, show_nsfw: bool, on_open: EventHandler<()>) -> Element {
    let blur = m.contains_adult_content && !show_nsfw;
    rsx! {
        div { class: "mod_card", onclick: move |_| on_open.call(()),
            div { class: "mod_card_media",
                div { class: if blur { "mod_thumb blurred" } else { "mod_thumb" },
                    if let Some(pic) = &m.picture_url {
                        img { src: "{pic}", alt: "{m.name}" }
                    }
                }
                div { class: "mod_card_scrim" }
                div { class: "mod_card_pills",
                    if installed {
                        span { class: "pill installed", {crate::icons::check(12)} "Installed" }
                    }
                    if m.contains_adult_content {
                        span { class: "pill adult", "18+" }
                    }
                }
            }
            div { class: "mod_card_body",
                div { class: "mod_card_title", "{m.name}" }
                div { class: "mod_card_author", "by {m.author()}" }
                div { class: "mod_card_stats",
                    if !m.version.is_empty() {
                        span { class: "stat ver", "v{m.version}" }
                    }
                }
            }
        }
    }
}

#[component]
fn NexusDetailPanel(
    info: NexusMod,
    apikey: String,
    sd_root: PathBuf,
    mut installed: Signal<HashSet<u64>>,
    on_close: EventHandler<()>,
) -> Element {
    let mod_id = info.mod_id;
    // We already have the mod's details from the listing, so only the file list needs fetching (and
    // that's the v1 endpoint, which needs the key).
    let key = apikey.clone();
    let files = use_resource(move || {
        let auth = Auth::ApiKey(key.clone());
        async move { nexus::mod_files(&auth, mod_id).await }
    });

    let is_installed = installed().contains(&info.mod_id);
    let summary = install::strip_html(&info.summary);

    rsx! {
        div { class: "mod_detail_overlay", onclick: move |_| on_close.call(()),
            div {
                class: "mod_detail_panel",
                onclick: move |e| e.stop_propagation(),
                button { class: "close", onclick: move |_| on_close.call(()), "X" }

                div { class: "mod_detail_head",
                    div {
                        h2 { class: "mod_detail_title", "{info.name}" }
                        div { class: "mod_detail_meta", "by {info.author()} · v{info.version}" }
                    }
                    if is_installed {
                        button {
                            class: "danger",
                            onclick: {
                                let sd = sd_root.clone();
                                let id = info.mod_id;
                                move |_| {
                                    install::uninstall_nexus_mod(&sd, id);
                                    installed.set(install::installed_nexus_ids(&sd).into_keys().collect());
                                }
                            },
                            "Uninstall"
                        }
                    }
                }

                if let Some(pic) = &info.picture_url {
                    div { class: "mod_screens", img { src: "{pic}" } }
                }

                if !summary.is_empty() {
                    p { class: "mod_desc", "{summary}" }
                }

                div { class: "mod_files",
                    match &*files.read() {
                        None => rsx! { div { class: "mod_message", "Loading files…" } },
                        Some(Err(e)) => rsx! { div { class: "mod_message error", "Couldn't load files: {e}" } },
                        Some(Ok(list)) if list.is_empty() => rsx! { div { class: "mod_message", "No files available." } },
                        Some(Ok(list)) => rsx! {
                            for f in list.clone() {
                                NexusInstallRow {
                                    key: "{f.file_id}",
                                    info: info.clone(),
                                    file: f.clone(),
                                    apikey: apikey.clone(),
                                    sd_root: sd_root.clone(),
                                    installed,
                                }
                            }
                        },
                    }
                }

                ManualInstall { info: info.clone(), sd_root: sd_root.clone(), installed }

                a { class: "mod_page_link", href: "{nexus::mod_page_url(info.mod_id)}", "Open the mod on NexusMods" }
            }
        }
    }
}

// The free-account download: given an nxm:// link plus the user's API key, ask Nexus for the real
// download URL (the link's one-time key lets a free account through), fetch the mod's name, then
// download and install. Shared by the paste box and the OS nxm:// handler. `on_status` reports
// progress. Returns the installed mod's name on success.
pub async fn run_nxm<F: FnMut(String)>(
    link: &str,
    apikey: String,
    sd_root: PathBuf,
    mut on_status: F,
) -> Result<String, String> {
    let nxm = nexus::parse_nxm(link).map_err(|e| e.to_string())?;
    if nxm.game_domain != nexus::GAME_DOMAIN {
        return Err(format!("That link is for \"{}\", not Fire Emblem Engage.", nxm.game_domain));
    }

    let auth = Auth::ApiKey(apikey);
    on_status("Getting the download link…".to_string());
    let url = nexus::download_link(&auth, nxm.mod_id, nxm.file_id, Some(&nxm.key), Some(&nxm.expires))
        .await
        .map_err(|e| e.to_string())?;

    on_status("Looking up the mod…".to_string());
    let info = nexus::mod_info(&auth, nxm.mod_id).await.map_err(|e| e.to_string())?;

    on_status("Downloading…".to_string());
    let bytes = nexus::download(&url, |_, _| {}).await.map_err(|e| e.to_string())?;

    on_status(format!("Installing {}…", info.name));
    let meta = install::NexusMeta {
        mod_id: info.mod_id,
        name: info.name.clone(),
        author: info.author(),
        description: info.summary.clone(),
        source_url: nexus::mod_page_url(info.mod_id),
    };
    install::install_nexus_mod(&sd_root, &meta, &bytes).map_err(|e| e.to_string())?;
    Ok(info.name)
}

// Install a mod from a file the user downloaded themselves. This is the free-account path for games
// like Fire Emblem Engage that NexusMods doesn't offer "Mod Manager Download" for (so there's no
// nxm:// link and the API refuses free downloads). We already know the mod's details from the
// listing, so the file just supplies the bytes and we build a proper config.yaml around them.
#[component]
fn ManualInstall(info: NexusMod, sd_root: PathBuf, mut installed: Signal<HashSet<u64>>) -> Element {
    let mut status = use_signal(|| None::<String>);
    let input_id = format!("manual_zip_{}", info.mod_id);

    rsx! {
        div { class: "manual_install",
            div { class: "note",
                "Free account? Download the mod from its NexusMods page (Manual Download), then pick the .zip here."
            }
            label { r#for: "{input_id}", class: "manual_btn", "Install from downloaded file…" }
            input {
                id: "{input_id}",
                r#type: "file",
                accept: ".zip",
                display: "none",
                onchange: move |evt| {
                    let info = info.clone();
                    let sd = sd_root.clone();
                    async move {
                        let files = evt.files();
                        let Some(file) = files.first() else {
                            return;
                        };
                        status.set(Some("Reading file…".to_string()));
                        let bytes = match file.read_bytes().await {
                            Ok(b) => b,
                            Err(e) => {
                                status.set(Some(format!("Couldn't read that file: {e}")));
                                return;
                            }
                        };
                        status.set(Some("Installing…".to_string()));
                        let meta = install::NexusMeta {
                            mod_id: info.mod_id,
                            name: info.name.clone(),
                            author: info.author(),
                            description: info.summary.clone(),
                            source_url: nexus::mod_page_url(info.mod_id),
                        };
                        match install::install_nexus_mod(&sd, &meta, bytes.as_ref()) {
                            Ok(()) => {
                                installed.set(install::installed_nexus_ids(&sd).into_keys().collect());
                                status.set(Some(format!("Installed {}!", info.name)));
                            }
                            Err(e) => status.set(Some(format!("Error: {e}"))),
                        }
                    }
                },
            }
            if let Some(s) = status() {
                div { class: "nxm_status", "{s}" }
            }
        }
    }
}

// Local copy of the install button's state machine (GameBanana's is in mods_ui, kept separate so
// the two browsers don't depend on each other).
#[derive(Clone, PartialEq)]
enum Status {
    Idle,
    Downloading(u64, u64),
    Installing,
    Done,
    Error(String),
}

#[component]
fn NexusInstallRow(
    info: NexusMod,
    file: NexusFile,
    apikey: String,
    sd_root: PathBuf,
    installed: Signal<HashSet<u64>>,
) -> Element {
    let mut status = use_signal(|| Status::Idle);
    let label = if installed().contains(&info.mod_id) { "Reinstall" } else { "Install" };

    rsx! {
        div { class: "install_row",
            div { class: "file_info",
                "{file.name} ({format_filesize(file.size_bytes())})"
            }
            match status() {
                Status::Idle => rsx! {
                    button {
                        class: "primary",
                        onclick: move |_| {
                            let auth = Auth::ApiKey(apikey.clone());
                            let info = info.clone();
                            let file = file.clone();
                            let sd = sd_root.clone();
                            status.set(Status::Downloading(0, 0));
                            spawn(async move {
                                // Premium can fetch the link directly, free accounts get a clear error here.
                                let link = match nexus::download_link(&auth, info.mod_id, file.file_id, None, None).await {
                                    Ok(l) => l,
                                    Err(e) => { status.set(Status::Error(e.to_string())); return; }
                                };
                                match nexus::download(&link, move |d, t| status.set(Status::Downloading(d, t))).await {
                                    Ok(bytes) => {
                                        status.set(Status::Installing);
                                        let meta = install::NexusMeta {
                                            mod_id: info.mod_id,
                                            name: info.name.clone(),
                                            author: info.author(),
                                            description: info.summary.clone(),
                                            source_url: nexus::mod_page_url(info.mod_id),
                                        };
                                        match install::install_nexus_mod(&sd, &meta, &bytes) {
                                            Ok(()) => {
                                                installed.set(install::installed_nexus_ids(&sd).into_keys().collect());
                                                status.set(Status::Done);
                                            }
                                            Err(e) => status.set(Status::Error(e.to_string())),
                                        }
                                    }
                                    Err(e) => status.set(Status::Error(e.to_string())),
                                }
                            });
                        },
                        "{label}"
                    }
                },
                Status::Downloading(downloaded, total) => {
                    let pct = (downloaded * 100).checked_div(total).unwrap_or(0);
                    rsx! {
                        div { class: "progress",
                            div { class: "bar", style: "width: {pct}%" }
                        }
                        span { class: "progress_label",
                            "{format_filesize(downloaded)} / {format_filesize(total)}"
                        }
                    }
                }
                Status::Installing => rsx! {
                    span { class: "muted", "Installing…" }
                },
                Status::Done => rsx! {
                    span { class: "ok", "Installed!" }
                },
                Status::Error(e) => rsx! {
                    div { class: "errline",
                        span { class: "err", "Error: {e}" }
                        button { onclick: move |_| status.set(Status::Idle), "Retry" }
                    }
                },
            }
        }
    }
}
