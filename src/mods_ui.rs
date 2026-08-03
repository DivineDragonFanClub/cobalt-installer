// The GameBanana mod browser UI (desktop only).
//
// A search bar and category filter over a grid of mod cards, with a "Load more" button that
// appends the next page, and a detail overlay with screenshots and per-file install/uninstall.
// Browsing/searching and the install itself live in the `gamebanana` and `install` modules, this
// file is just the Dioxus components that drive them.

use std::collections::HashSet;
use std::path::PathBuf;

use dioxus::prelude::*;

use crate::gamebanana::{self, Listing, ModDetail, ModFile};
use crate::install;

// The mods tab: pick a source (GameBanana or NexusMods) and show that browser.
#[derive(Clone, Copy, PartialEq)]
enum Source {
    GameBanana,
    Nexus,
}

#[component]
pub fn ModBrowser(sd_root: PathBuf) -> Element {
    let mut source = use_signal(|| Source::GameBanana);
    rsx! {
        div { class: "source_toggle",
            button {
                class: if source() == Source::GameBanana { "src active" } else { "src" },
                onclick: move |_| source.set(Source::GameBanana),
                "GameBanana"
            }
            button {
                class: if source() == Source::Nexus { "src active" } else { "src" },
                onclick: move |_| source.set(Source::Nexus),
                "NexusMods"
            }
        }
        match source() {
            Source::GameBanana => rsx! { GameBananaBrowser { sd_root: sd_root.clone() } },
            Source::Nexus => rsx! { crate::nexus_ui::NexusBrowser { sd_root: sd_root.clone() } },
        }
    }
}

#[component]
fn GameBananaBrowser(sd_root: PathBuf) -> Element {
    let mut query = use_signal(String::new);
    let mut category = use_signal(|| None::<u64>);
    let mut show_nsfw = use_signal(|| false);
    let mut selected = use_signal(|| None::<u64>);

    // The results we've loaded so far (grows as the user hits "Load more").
    let mut results = use_signal(Vec::<Listing>::new);
    let mut page = use_signal(|| 1u32);
    let mut loading = use_signal(|| false);
    let mut error = use_signal(|| None::<String>);
    let mut has_more = use_signal(|| true);

    // The category list for the dropdown, fetched once.
    let categories = use_resource(|| async { gamebanana::categories().await });

    // Which GameBanana mods are already installed, so we can badge them. Refreshed after changes.
    let installed_root = sd_root.clone();
    let installed = use_signal(move || {
        install::installed_gamebanana_ids(&installed_root)
            .into_keys()
            .collect::<HashSet<u64>>()
    });

    // Whenever the search text or category changes, start over from page 1.
    use_effect(move || {
        let q = query().trim().to_string();
        let cat = category();
        results.set(Vec::new());
        page.set(1);
        has_more.set(true);
        error.set(None);
        loading.set(true);
        spawn(async move {
            match fetch(&q, cat, 1).await {
                Ok(list) => {
                    has_more.set(list.len() >= 15);
                    results.set(list);
                }
                Err(e) => error.set(Some(e.to_string())),
            }
            loading.set(false);
        });
    });

    // Append the next page onto what's already shown.
    let load_more = move |_| {
        if loading() || !has_more() {
            return;
        }
        let q = query().trim().to_string();
        let cat = category();
        let next = page() + 1;
        loading.set(true);
        spawn(async move {
            match fetch(&q, cat, next).await {
                Ok(list) => {
                    has_more.set(list.len() >= 15);
                    results.write().extend(list);
                    page.set(next);
                }
                Err(e) => error.set(Some(e.to_string())),
            }
            loading.set(false);
        });
    };

    rsx! {
        div { id: "mod_browser",
            div { class: "mod_search_bar",
                input {
                    r#type: "text",
                    placeholder: "Search mods…",
                    value: "{query}",
                    oninput: move |e| query.set(e.value()),
                }
                select {
                    class: "category_select",
                    value: category().map(|c| c.to_string()).unwrap_or_default(),
                    // Category doesn't combine with a text search, GameBanana searches all categories.
                    disabled: query().trim().len() >= 2,
                    onchange: move |e| {
                        let v = e.value();
                        category.set(v.parse::<u64>().ok());
                    },
                    option { value: "", "All categories" }
                    if let Some(Ok(cats)) = &*categories.read() {
                        for c in cats.clone() {
                            option { value: "{c.id}", "{c.name}" }
                        }
                    }
                }
                label { class: "nsfw_toggle",
                    input {
                        r#type: "checkbox",
                        checked: show_nsfw(),
                        onchange: move |e| show_nsfw.set(e.checked()),
                    }
                    "Show adult content"
                }
            }

            if let Some(e) = error() {
                div { class: "mod_message error", "Couldn't load mods: {e}" }
            }

            if !results().is_empty() {
                div { class: "mod_grid",
                    for m in results() {
                        {
                            let id = m.id;
                            rsx! {
                                ModCard {
                                    key: "{id}",
                                    listing: m.clone(),
                                    installed: installed().contains(&id),
                                    show_nsfw: show_nsfw(),
                                    on_open: move |_| selected.set(Some(id)),
                                }
                            }
                        }
                    }
                }
            }

            div { class: "mod_pager",
                if loading() {
                    span { class: "mod_message", "Loading…" }
                } else if results().is_empty() && error().is_none() {
                    span { class: "mod_message", "No mods found." }
                } else if has_more() {
                    button { class: "secondary", onclick: load_more, "Load more" }
                }
            }
        }

        if let Some(id) = selected() {
            ModDetailPanel {
                mod_id: id,
                sd_root: sd_root.clone(),
                installed,
                on_close: move |_| selected.set(None),
            }
        }
    }
}

// Pick the right API call for the current filters: text search wins, then category, else the feed.
async fn fetch(query: &str, category: Option<u64>, page: u32) -> anyhow::Result<Vec<Listing>> {
    if query.len() >= 2 {
        gamebanana::search(query, page).await
    } else if let Some(cat) = category {
        gamebanana::by_category(cat, page).await
    } else {
        gamebanana::browse(page).await
    }
}

#[component]
fn ModCard(listing: Listing, installed: bool, show_nsfw: bool, on_open: EventHandler<()>) -> Element {
    let blur = listing.has_content_ratings && !show_nsfw;
    rsx! {
        div { class: "mod_card", onclick: move |_| on_open.call(()),
            div { class: if blur { "mod_thumb blurred" } else { "mod_thumb" },
                if let Some(thumb) = listing.thumb_url() {
                    img { src: "{thumb}", alt: "{listing.name}" }
                }
            }
            div { class: "mod_card_body",
                div { class: "mod_card_title", "{listing.name}" }
                div { class: "mod_card_author", "by {listing.author()}" }
                if installed {
                    span { class: "installed_badge", "Installed" }
                }
            }
        }
    }
}

#[component]
fn ModDetailPanel(
    mod_id: u64,
    sd_root: PathBuf,
    installed: Signal<HashSet<u64>>,
    on_close: EventHandler<()>,
) -> Element {
    let detail = use_resource(move || async move { gamebanana::detail(mod_id).await });

    rsx! {
        div { class: "mod_detail_overlay", onclick: move |_| on_close.call(()),
            div {
                class: "mod_detail_panel",
                // Clicks inside the panel shouldn't close it.
                onclick: move |e| e.stop_propagation(),
                button { class: "close", onclick: move |_| on_close.call(()), "X" }
                match &*detail.read() {
                    Some(Ok(d)) => rsx! {
                        ModDetailContent { detail: d.clone(), sd_root: sd_root.clone(), installed }
                    },
                    Some(Err(e)) => rsx! {
                        div { class: "mod_message error", "Couldn't load this mod: {e}" }
                    },
                    None => rsx! {
                        div { class: "mod_message", "Loading…" }
                    },
                }
            }
        }
    }
}

#[component]
fn ModDetailContent(detail: ModDetail, sd_root: PathBuf, mut installed: Signal<HashSet<u64>>) -> Element {
    let description = install::strip_html(&detail.description_html);
    let is_installed = installed().contains(&detail.id);
    rsx! {
        div { class: "mod_detail_head",
            div {
                h2 { class: "mod_detail_title", "{detail.name}" }
                div { class: "mod_detail_meta",
                    "by {detail.author()} · {detail.likes} likes · {detail.views} views"
                }
            }
            if is_installed {
                button {
                    class: "danger",
                    onclick: {
                        let sd = sd_root.clone();
                        let id = detail.id;
                        move |_| {
                            install::uninstall_gamebanana_mod(&sd, id);
                            installed.set(install::installed_gamebanana_ids(&sd).into_keys().collect());
                        }
                    },
                    "Uninstall"
                }
            }
        }

        if !detail.preview.images.is_empty() {
            div { class: "mod_screens",
                for img in detail.preview.images.iter().take(6) {
                    img { src: "{img.full_url()}" }
                }
            }
        }

        if !description.is_empty() {
            p { class: "mod_desc", "{description}" }
        }

        div { class: "mod_files",
            if detail.files.is_empty() {
                div { class: "mod_message", "No downloadable files on this mod." }
            }
            for f in detail.files.clone() {
                InstallRow {
                    key: "{f.id}",
                    detail: detail.clone(),
                    file: f.clone(),
                    sd_root: sd_root.clone(),
                    installed,
                }
            }
        }

        a { class: "mod_page_link", href: "{detail.profile_url}", "Open on GameBanana" }
    }
}

// The per-file install button and its progress/result states.
#[derive(Clone, PartialEq)]
enum Status {
    Idle,
    Downloading(u64, u64),
    Installing,
    Done,
    Error(String),
}

#[component]
fn InstallRow(detail: ModDetail, file: ModFile, sd_root: PathBuf, installed: Signal<HashSet<u64>>) -> Element {
    let mut status = use_signal(|| Status::Idle);
    // Show "Reinstall" if this mod is already installed, since installing replaces it.
    let label = if installed().contains(&detail.id) { "Reinstall" } else { "Install" };

    rsx! {
        div { class: "install_row",
            div { class: "file_info",
                "{file.filename} ({gamebanana::format_filesize(file.filesize)})"
            }
            match status() {
                Status::Idle => rsx! {
                    button {
                        class: "primary",
                        onclick: move |_| {
                            let url = file.download_url.clone();
                            let detail = detail.clone();
                            let sd = sd_root.clone();
                            status.set(Status::Downloading(0, 0));
                            spawn(async move {
                                match gamebanana::download(&url, move |d, t| status.set(Status::Downloading(d, t))).await {
                                    Ok(bytes) => {
                                        status.set(Status::Installing);
                                        match install::install_gamebanana_mod(&sd, &detail, &bytes) {
                                            Ok(()) => {
                                                installed.set(
                                                    install::installed_gamebanana_ids(&sd).into_keys().collect(),
                                                );
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
                            "{gamebanana::format_filesize(downloaded)} / {gamebanana::format_filesize(total)}"
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
