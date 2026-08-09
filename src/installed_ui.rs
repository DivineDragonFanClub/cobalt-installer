// The "My Mods" view (desktop only): everything currently sitting in engage/mods, whether we
// installed it or the user dropped it in by hand. Each row shows where the mod came from and lets
// the user open its folder or remove it. The list is just a scan of the folder, no database.

use std::path::PathBuf;

use dioxus::prelude::*;

use crate::install::{self, InstalledMod, ModSource};

#[component]
pub fn MyMods(sd_root: PathBuf, on_view: EventHandler<ModSource>) -> Element {
    // Scan once on mount, then re-scan whenever we change something (uninstall) or the user asks.
    let scan_root = sd_root.clone();
    let mut mods = use_signal(move || install::scan_installed_mods(&scan_root));

    let refresh_root = sd_root.clone();
    let refresh = move |_| mods.set(install::scan_installed_mods(&refresh_root));

    // Open the whole mods folder in the OS file browser.
    let open_root = sd_root.clone();
    let open_mods_folder = move |_| {
        let mods_dir = open_root.join("engage").join("mods");
        let _ = crate::open_dir(mods_dir);
    };

    // Case-insensitive filter over name + folder slug + author + description
    // (author comes from the mod's config.yaml, which the scanner already
    // parses), then the chosen sort. installed_at is filesystem metadata.
    let mut query = use_signal(String::new);
    let mut sort_by = use_signal(|| "name".to_string());
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
            "large" => list.sort_by_key(|m| std::cmp::Reverse(m.size_bytes)),
            "small" => list.sort_by_key(|m| m.size_bytes),
            // scan_installed_mods already returns name order; re-sort anyway
            // so switching back from another sort restores it
            _ => list.sort_by(|a, b| a.name.to_lowercase().cmp(&b.name.to_lowercase())),
        }
        list
    });

    rsx! {
        div { id: "my_mods",
            div { class: "my_mods_head",
                div { class: "my_mods_title",
                    h2 { "Installed mods" }
                    span { class: "count",
                        if query().trim().is_empty() {
                            "{mods().len()} installed"
                        } else {
                            "{filtered().len()} of {mods().len()}"
                        }
                    }
                }
                div { class: "my_mods_actions",
                    button { class: "ghost", onclick: open_mods_folder, "Open folder" }
                    button { class: "ghost", onclick: refresh, "Refresh" }
                }
            }

            // Same bar layout as the Browse view: search stretches, sort trails.
            div { class: "mod_search_bar",
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
                select {
                    class: "category_select",
                    value: "{sort_by}",
                    onchange: move |e| sort_by.set(e.value()),
                    option { value: "name", "Name" }
                    option { value: "recent", "Recently installed" }
                    option { value: "oldest", "Oldest first" }
                    option { value: "large", "Largest first" }
                    option { value: "small", "Smallest first" }
                }
            }

            if mods().is_empty() {
                div { class: "empty_state",
                    div { class: "empty_icon", {crate::icons::package(40)} }
                    div { class: "empty_title", "No mods installed yet" }
                    div { class: "empty_sub", "Head to Browse to find and install mods." }
                }
            } else if filtered().is_empty() {
                div { class: "mod_message", "No installed mods match your search." }
            } else {
                div { class: "installed_list",
                    for m in filtered() {
                        InstalledRow {
                            key: "{m.folder}",
                            entry: m.clone(),
                            sd_root: sd_root.clone(),
                            mods,
                            on_view,
                            query: query().trim().to_lowercase(),
                        }
                    }
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
        for (seg, hit) in segs {
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
        ModSource::Manual => ("Manual", "chip manual"),
    }
}


#[component]
fn InstalledRow(
    entry: InstalledMod,
    sd_root: PathBuf,
    mods: Signal<Vec<InstalledMod>>,
    on_view: EventHandler<ModSource>,
    query: String,
) -> Element {
    // Uninstall is destructive, so the button asks for a second click to confirm.
    let mut confirming = use_signal(|| false);
    let (chip_label, chip_class) = source_chip(&entry.source);

    let open_entry = {
        let path = entry.path.clone();
        move |_| {
            // A folder opens directly, a .zip mod opens its parent so we don't try to launch the zip.
            let target = if path.is_dir() { path.clone() } else { path.parent().map(PathBuf::from).unwrap_or_else(|| path.clone()) };
            let _ = crate::open_dir(target);
        }
    };

    let do_uninstall = {
        let path = entry.path.clone();
        let root = sd_root.clone();
        move |_| {
            install::remove_installed_mod(&path);
            mods.set(install::scan_installed_mods(&root));
        }
    };

    rsx! {
        div { class: "installed_row",
            div { class: "installed_info",
                div { class: "installed_name_line",
                    span { class: "installed_name", {highlight(&entry.name, &query)} }
                    span { class: chip_class, "{chip_label}" }
                    if !entry.has_config {
                        span { class: "chip warn", "No config" }
                    }
                }
                div { class: "installed_meta",
                    if let Some(author) = entry.author.clone() {
                        "by "
                        {highlight(&author, &query)}
                        " · "
                    }
                    if entry.size_bytes > 0 {
                        "{crate::gamebanana::format_filesize(entry.size_bytes)} · "
                    }
                    code { {highlight(&entry.folder, &query)} }
                }
                if let Some(desc) = entry.description.clone() {
                    p { class: "installed_desc", {highlight(&desc, &query)} }
                }
            }
            div { class: "installed_row_actions",
                // "View" opens the in-app browser detail. GameBanana always; Nexus only while its
                // browser is enabled (otherwise the button would open nothing).
                if matches!(entry.source, ModSource::GameBanana(_))
                    || (matches!(entry.source, ModSource::Nexus(_)) && crate::mods_ui::NEXUS_ENABLED)
                {
                    button {
                        class: "ghost",
                        onclick: {
                            let src = entry.source.clone();
                            move |_| on_view.call(src.clone())
                        },
                        "View"
                    }
                }
                button { class: "ghost", onclick: open_entry, "Open" }
                if confirming() {
                    button { class: "danger", onclick: do_uninstall, "Confirm remove" }
                    button { class: "ghost", onclick: move |_| confirming.set(false), "Cancel" }
                } else {
                    button { class: "danger_outline", onclick: move |_| confirming.set(true), "Uninstall" }
                }
            }
        }
    }
}
