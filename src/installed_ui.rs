// The "My Mods" view (desktop only): everything currently sitting in engage/mods, whether we
// installed it or the user dropped it in by hand. Each row shows where the mod came from and lets
// the user open its folder or remove it. The list is just a scan of the folder, no database.

use std::collections::HashSet;
use std::path::PathBuf;

use dioxus::prelude::*;

use crate::install::{self, InstalledMod, ModSource};

#[component]
pub fn MyMods(sd_root: PathBuf) -> Element {
    // Scan once on mount, then re-scan whenever we change something (uninstall) or the user asks.
    let scan_root = sd_root.clone();
    let mut mods = use_signal(move || install::scan_installed_mods(&scan_root));

    // "View" opens the browser's detail panel right here as an overlay — no tab jump. The
    // panel fetches its own data from the GameBanana id; this set feeds its Installed badge
    // and gets refreshed on open so it reflects reality even after outside changes.
    let mut viewing = use_signal(|| None::<u64>);
    let ids_root = sd_root.clone();
    let mut installed_ids = use_signal(move || {
        install::installed_gamebanana_ids(&ids_root).into_keys().collect::<HashSet<u64>>()
    });

    // In-progress installs from the Body coroutine, shown as live rows above the list. A finished
    // install drops out of this list, so re-scan the folder whenever the set of active ids changes
    // (memoized so download progress ticks don't trigger a rescan) to surface the new mod.
    let downloads = use_context::<crate::downloads::Downloads>();
    let active_ids = use_memo(move || {
        let mut ids: Vec<u64> = downloads().iter().map(|d| d.id).collect();
        ids.sort();
        ids
    });
    let rescan_root = sd_root.clone();
    use_effect(move || {
        let _ = active_ids();
        mods.set(install::scan_installed_mods(&rescan_root));
    });

    let refresh_root = sd_root.clone();
    let mut refresh = move |_| mods.set(install::scan_installed_mods(&refresh_root));

    // Open the whole mods folder in the OS file browser.
    let open_root = sd_root.clone();
    let open_mods_folder = move |_| {
        let mods_dir = open_root.join("engage").join("mods");
        let _ = crate::open_dir(mods_dir);
    };

    // The hamburger next to the sort select, holding the list-level tools.
    let mut tools_open = use_signal(|| false);

    // Case-insensitive filter over name + folder slug + author + description
    // (author comes from the mod's config.yaml, which the scanner already
    // parses), then the chosen sort. installed_at is filesystem metadata.
    let mut query = use_signal(String::new);
    let mut sort_by = use_signal(|| "recent".to_string());
    // The starter-pack offer, shown as an overlay from the empty state.
    let mut show_starter = use_signal(|| false);
    let filtered = use_memo(move || {
        let q = query().trim().to_lowercase();
        let mut list = mods();
        if !q.is_empty() {
            list.retain(|m| {
                m.name.to_lowercase().contains(&q)
                    || m.folder.to_lowercase().contains(&q)
                    || m.author.as_deref().unwrap_or("").to_lowercase().contains(&q)
                    || m.description.as_deref().unwrap_or("").to_lowercase().contains(&q)
            });
        }
        match sort_by().as_str() {
            "recent" => list.sort_by_key(|m| std::cmp::Reverse(m.installed_at)),
            "oldest" => list.sort_by_key(|m| m.installed_at),
            // scan_installed_mods already returns name order; re-sort anyway
            // so switching back from another sort restores it
            _ => list.sort_by(|a, b| a.name.to_lowercase().cmp(&b.name.to_lowercase())),
        }
        list
    });

    rsx! {
        div { id: "my_mods",
            // No page title: the mod count lives in the search placeholder, and the
            // match count surfaces inside the input while a query is active.
            div { class: "mod_search_bar",
                div { class: "search_wrap",
                    input {
                        r#type: "text",
                        placeholder: "Search name, author or description…",
                        // mod names are codes and slugs — keep the OS text helpers out
                        autocomplete: "off",
                        autocapitalize: "off",
                        spellcheck: "false",
                        "autocorrect": "off",
                        value: "{query}",
                        oninput: move |e| query.set(e.value()),
                    }
                    // Idle: the library size. Filtering: how much of it matches.
                    span { class: "search_count",
                        if query().trim().is_empty() {
                            "{mods().len()} mods installed"
                        } else {
                            "{filtered().len()} of {mods().len()}"
                        }
                    }
                }
                select {
                    class: "category_select",
                    value: "{sort_by}",
                    onchange: move |e| sort_by.set(e.value()),
                    option { value: "name", "Name" }
                    option { value: "recent", "Recently installed" }
                    option { value: "oldest", "Oldest first" }
                }
                // List-level tools live behind the hamburger, same popover as the rows'.
                div { class: "row_menu_anchor",
                    button {
                        class: "ghost kebab",
                        title: "List tools",
                        onclick: move |_| tools_open.set(!tools_open()),
                        {crate::icons::hamburger(16)}
                    }
                    if tools_open() {
                        div { class: "menu_backdrop", onclick: move |_| tools_open.set(false) }
                        div { class: "row_menu",
                            button {
                                class: "menu_item",
                                onclick: move |e| {
                                    tools_open.set(false);
                                    open_mods_folder(e);
                                },
                                "Open folder"
                            }
                            button {
                                class: "menu_item",
                                onclick: move |e| {
                                    tools_open.set(false);
                                    refresh(e);
                                },
                                "Refresh"
                            }
                        }
                    }
                }
            }

            if !downloads().is_empty() {
                div { class: "installed_list downloads",
                    for d in downloads() {
                        DownloadRow { key: "dl{d.id}", entry: d.clone() }
                    }
                }
            }

            if mods().is_empty() && downloads().is_empty() {
                div { class: "empty_state",
                    div { class: "empty_icon", {crate::icons::package(40)} }
                    div { class: "empty_title", "No mods installed yet" }
                    div { class: "empty_sub",
                        "Grab a starter pack of hand-picked mods, or head to Browse to find your own."
                    }
                    button {
                        class: "engage",
                        onclick: move |_| show_starter.set(true),
                        "Browse the starter pack"
                    }
                }
            } else if !mods().is_empty() && filtered().is_empty() {
                div { class: "mod_message", "No installed mods match your search." }
            } else {
                div { class: "installed_list",
                    for m in filtered() {
                        InstalledRow {
                            key: "{m.folder}",
                            entry: m.clone(),
                            sd_root: sd_root.clone(),
                            mods,
                            on_view: {
                                let root = sd_root.clone();
                                move |id| {
                                    installed_ids
                                        .set(install::installed_gamebanana_ids(&root).into_keys().collect());
                                    viewing.set(Some(id));
                                }
                            },
                            query: query().trim().to_lowercase(),
                        }
                    }
                }
            }
        }

        if let Some(id) = viewing() {
            crate::mods_ui::ModDetailPanel {
                mod_id: id,
                sd_root: sd_root.clone(),
                installed: installed_ids,
                on_close: {
                    let root = sd_root.clone();
                    move |_| {
                        viewing.set(None);
                        // The panel can reinstall or uninstall; the list must reflect it.
                        mods.set(install::scan_installed_mods(&root));
                    }
                },
            }
        }

        if show_starter() {
            div {
                class: "starter_overlay",
                onclick: move |_| show_starter.set(false),
                div {
                    class: "starter_panel",
                    onclick: move |e| e.stop_propagation(),
                    button {
                        class: "close",
                        onclick: move |_| show_starter.set(false),
                        "X"
                    }
                    crate::starter_ui::StarterPack {
                        sd_root: sd_root.clone(),
                        on_close: {
                            let root = sd_root.clone();
                            move |_| {
                                show_starter.set(false);
                                mods.set(install::scan_installed_mods(&root));
                            }
                        },
                    }
                }
            }
        }
    }
}

// A live row for an install still in flight (or one that failed), shown above the installed list.
#[component]
fn DownloadRow(entry: crate::downloads::ActiveDownload) -> Element {
    use crate::downloads::Phase;
    rsx! {
        div { class: "installed_row download_row",
            div { class: "download_thumb",
                if let Some(thumb) = entry.thumb.clone() {
                    img { src: "{thumb}", alt: "{entry.name}" }
                }
            }
            div { class: "installed_info",
                div { class: "installed_name_line",
                    span { class: "installed_name", "{entry.name}" }
                }
                match &entry.phase {
                    Phase::Downloading { what, done, total } => {
                        let pct = (*done * 100).checked_div(*total).unwrap_or(0);
                        let sizes = format!(
                            "{} / {}",
                            crate::gamebanana::format_filesize(*done),
                            crate::gamebanana::format_filesize(*total),
                        );
                        rsx! {
                            div { class: "progress",
                                div { class: "bar", style: "width: {pct}%" }
                            }
                            div { class: "installed_meta",
                                if what.is_empty() {
                                    "Downloading… {sizes}"
                                } else {
                                    "Downloading {what}… {sizes}"
                                }
                            }
                        }
                    }
                    Phase::Working { what } => rsx! {
                        div { class: "installed_meta",
                            crate::mods_ui::Spinner {}
                            " {what}"
                        }
                    },
                    Phase::Error(e) => rsx! {
                        div { class: "installed_meta dl_err", "Failed: {e}" }
                    },
                }
            }
        }
    }
}

// Wrap case-insensitive occurrences of `q` (already trimmed + lowercased) in
// <mark>. Bails out to plain text when lowercasing changed byte offsets or a
// hit lands off a char boundary, so odd scripts can never panic the slicing.
fn highlight(text: &str, q: &str) -> Element {
    let lower = text.to_lowercase();
    if q.is_empty() || lower.len() != text.len() {
        return rsx! { "{text}" };
    }
    let mut segs: Vec<(String, bool)> = Vec::new();
    let mut pos = 0usize;
    while let Some(off) = lower[pos..].find(q) {
        let start = pos + off;
        let end = start + q.len();
        if !text.is_char_boundary(start) || !text.is_char_boundary(end) {
            return rsx! { "{text}" };
        }
        if start > pos {
            segs.push((text[pos..start].to_string(), false));
        }
        segs.push((text[start..end].to_string(), true));
        pos = end;
    }
    if segs.is_empty() {
        return rsx! { "{text}" };
    }
    if pos < text.len() {
        segs.push((text[pos..].to_string(), false));
    }
    rsx! {
        for (seg , hit) in segs {
            if hit {
                mark { "{seg}" }
            } else {
                "{seg}"
            }
        }
    }
}

// Human label + css class for the source chip.
fn source_chip(source: &ModSource) -> (&'static str, &'static str) {
    match source {
        ModSource::GameBanana(_) => ("GameBanana", "chip gb"),
        ModSource::Nexus(_) => ("NexusMods", "chip nexus"),
        ModSource::Manual => ("User installed", "chip manual"),
    }
}

// Undo urlencoding_encode for one URL path segment (http's Uri::path() keeps percent-encoding, so
// the handler decodes it itself). Strict: bad hex or invalid UTF-8 yields None → a 404, not a guess.
fn percent_decode(s: &str) -> Option<String> {
    let b = s.as_bytes();
    let mut out = Vec::with_capacity(b.len());
    let mut i = 0;
    while i < b.len() {
        if b[i] == b'%' {
            let hex = |c: &u8| (*c as char).to_digit(16).map(|d| d as u8);
            out.push(hex(b.get(i + 1)?)? * 16 + hex(b.get(i + 2)?)?);
            i += 3;
        } else {
            out.push(b[i]);
            i += 1;
        }
    }
    String::from_utf8(out).ok()
}

// Serves installed mods' saved preview images to the webview at /mod_thumb/<folder>/<file>.
// Registered from Body — it's always mounted, while registering here in MyMods would tear the
// protocol down on every tab switch. The handler runs synchronously on the main thread inside the
// webview's scheme callback, in the registering component's scope, so reading the storage signals
// is the same pattern as the nxm wry handler in main.rs. The install target is resolved per
// request, so a mid-session sd_root switch just works. Anything unexpected answers 404 — never
// panic in here, and never drop the responder (that would strand the request in the webview).
pub(crate) fn use_thumb_protocol(installation_type: Signal<String>, sdcard_path: Signal<String>) {
    use dioxus::desktop::wry::http::Response;
    dioxus::desktop::use_asset_handler("mod_thumb", move |request, responder| {
        let served = (|| {
            let path = request.uri().path().to_string();
            let mut segs = path.trim_start_matches('/').splitn(3, '/');
            let (_name, folder, file) = (segs.next()?, segs.next()?, segs.next()?);
            let folder = percent_decode(folder)?;
            let file = percent_decode(file)?;
            let sd = crate::resolve_sd_root(&installation_type(), &sdcard_path())?;
            let full = install::thumb_file_path(&sd, &folder, &file)?;
            let mime = match full.extension().and_then(|e| e.to_str()) {
                Some("png") => "image/png",
                Some("webp") => "image/webp",
                Some("gif") => "image/gif",
                _ => "image/jpeg",
            };
            let bytes = std::fs::read(&full).ok()?;
            Response::builder().header("Content-Type", mime).body(bytes).ok()
        })();
        match served {
            Some(resp) => responder.respond(resp),
            None => responder.respond(Response::builder().status(404).body(Vec::new()).unwrap()),
        }
    });
}


#[component]
fn InstalledRow(
    entry: InstalledMod,
    sd_root: PathBuf,
    mods: Signal<Vec<InstalledMod>>,
    // Called with the GameBanana mod id when the user wants the detail panel.
    on_view: EventHandler<u64>,
    query: String,
) -> Element {
    // Uninstall is destructive, so the button asks for a second click to confirm.
    let mut confirming = use_signal(|| false);
    // The ⋯ menu holding the row's secondary actions.
    let mut menu_open = use_signal(|| false);
    // The title bubble: hover shows it after a beat (tooltip-style, not instant),
    // leaving hides and cancels a pending show, and on manual rows a click shows it
    // immediately and pins it for 1.8s.
    let mut show_origin = use_signal(|| false);
    let mut show_delay = dioxus_sdk::time::use_debounce(
        std::time::Duration::from_millis(450),
        move |_| show_origin.set(true),
    );
    let mut hide_origin = dioxus_sdk::time::use_debounce(
        std::time::Duration::from_millis(1800),
        move |_| show_origin.set(false),
    );
    let (chip_label, chip_class) = source_chip(&entry.source);

    let reveal_label = crate::reveal_label();
    let reveal = {
        let path = entry.path.clone();
        move |_| {
            menu_open.set(false);
            let _ = crate::reveal_in_file_browser(&path);
        }
    };

    // Row art: the preview saved at install time, served through the mod_thumb protocol below. The
    // folder name is percent-encoded (spaces, #, unicode would break the URL), and v= busts the
    // webview's image cache when a reinstall replaces the folder.
    let thumb_src = entry.thumb.as_ref().map(|file| {
        let v = entry
            .installed_at
            .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
            .map(|d| d.as_secs())
            .unwrap_or(0);
        format!("/mod_thumb/{}/{file}?v={v}", crate::gamebanana::urlencoding_encode(&entry.folder))
    });

    rsx! {
        div { class: "installed_row",
            div { class: "installed_thumb",
                if let Some(src) = thumb_src {
                    // Cover-cropped to the tile, same treatment as the browse cards and download
                    // rows. Odd-aspect art just crops — "it is what it is" (a contain-over-blur
                    // letterbox was tried and rejected).
                    img { src: "{src}", alt: "", loading: "lazy" }
                } else {
                    // Interim placeholder (final art TBD): the package glyph on a neutral tile.
                    span { class: "thumb_placeholder", {crate::icons::package(18)} }
                }
            }
            div { class: "installed_info",
                div { class: "installed_name_line",
                    // A GameBanana mod's whole title links to its page, with the banana mark
                    // riding tight after the text; other sources keep the plain name + chip.
                    if let ModSource::GameBanana(gb_id) = entry.source {
                        // Clicking the title opens the in-app detail panel (the old View);
                        // the ⋯ menu holds the link out to the GameBanana site.
                        span {
                            class: "installed_name_group gb",
                            onmouseenter: move |_| show_delay.action(()),
                            onmouseleave: move |_| {
                                show_delay.cancel();
                                show_origin.set(false);
                            },
                            onclick: move |_| on_view.call(gb_id),
                            span { class: "installed_name", {highlight(&entry.name, &query)} }
                            span { class: "src_mark gb" }
                            if show_origin() {
                                span { class: "origin_tip", "View mod details" }
                            }
                        }
                    } else if matches!(entry.source, ModSource::Manual) {
                        // The Fortune Telling card: origin unknown, the user put it here. Same
                        // shape and bubble as the GameBanana title link; hover shows it, and a
                        // click pins it for a beat (useful mid-scroll and on touchpads).
                        span {
                            class: "installed_name_group",
                            onmouseenter: move |_| show_delay.action(()),
                            onmouseleave: move |_| {
                                show_delay.cancel();
                                show_origin.set(false);
                            },
                            onclick: move |_| {
                                show_delay.cancel();
                                show_origin.set(true);
                                hide_origin.action(());
                            },
                            span { class: "installed_name", {highlight(&entry.name, &query)} }
                            span { class: "src_mark fortune" }
                            if show_origin() {
                                span { class: "origin_tip", "You installed this mod manually." }
                            }
                        }
                    } else {
                        span { class: "installed_name", {highlight(&entry.name, &query)} }
                        span { class: chip_class, "{chip_label}" }
                    }
                }
                // A missing config takes the byline's place as plain text, not a chip. A mod
                // with a config but no author gets no meta line at all.
                if !entry.has_config {
                    div { class: "installed_meta", "Mod has no config file" }
                } else if let Some(author) = entry.author.clone() {
                    div { class: "installed_meta",
                        "by "
                        {highlight(&author, &query)}
                    }
                }
                if let Some(desc) = entry.description.clone() {
                    p { class: "installed_desc", {highlight(&desc, &query)} }
                }
            }
            div { class: "installed_row_actions",
                div { class: "row_menu_anchor",
                    button {
                        class: "ghost kebab",
                        title: "More actions",
                        onclick: move |_| menu_open.set(!menu_open()),
                        {crate::icons::dots(16)}
                    }
                    if menu_open() {
                        // Invisible click-catcher: any click outside the menu closes it.
                        div {
                            class: "menu_backdrop",
                            onclick: move |_| {
                                menu_open.set(false);
                                confirming.set(false);
                            },
                        }
                        div { class: "row_menu",
                            // Out to the mod's page on the GameBanana site.
                            if let ModSource::GameBanana(id) = entry.source {
                                a {
                                    class: "menu_item",
                                    href: "https://gamebanana.com/mods/{id}",
                                    onclick: move |_| menu_open.set(false),
                                    "Open on GameBanana"
                                }
                            }
                            button { class: "menu_item", onclick: reveal, "{reveal_label}" }
                            // Delete confirms in place: the same row re-arms as "Confirm delete",
                            // so nothing jumps under the cursor. Clicking outside disarms.
                            if confirming() {
                                button {
                                    class: "menu_item danger_item armed",
                                    onclick: {
                                        let path = entry.path.clone();
                                        let root = sd_root.clone();
                                        move |_| {
                                            menu_open.set(false);
                                            confirming.set(false);
                                            install::remove_installed_mod(&path);
                                            mods.set(install::scan_installed_mods(&root));
                                        }
                                    },
                                    "Confirm delete"
                                }
                            } else {
                                button {
                                    class: "menu_item danger_item",
                                    onclick: move |_| confirming.set(true),
                                    "Delete"
                                }
                            }
                        }
                    }
                }
            }
        }
    }
}
