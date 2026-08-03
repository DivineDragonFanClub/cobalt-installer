// The "My Mods" view (desktop only): everything currently sitting in engage/mods, whether we
// installed it or the user dropped it in by hand. Each row shows where the mod came from and lets
// the user open its folder or remove it. The list is just a scan of the folder, no database.

use std::path::PathBuf;

use dioxus::prelude::*;

use crate::install::{self, InstalledMod, ModSource};

#[component]
pub fn MyMods(sd_root: PathBuf) -> Element {
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

    rsx! {
        div { id: "my_mods",
            div { class: "my_mods_head",
                div { class: "my_mods_title",
                    h2 { "Installed mods" }
                    span { class: "count", "{mods().len()} installed" }
                }
                div { class: "my_mods_actions",
                    button { class: "ghost", onclick: open_mods_folder, "Open folder" }
                    button { class: "ghost", onclick: refresh, "Refresh" }
                }
            }

            if mods().is_empty() {
                div { class: "empty_state",
                    div { class: "empty_icon", {crate::icons::package(40)} }
                    div { class: "empty_title", "No mods installed yet" }
                    div { class: "empty_sub", "Head to Browse to find and install mods." }
                }
            } else {
                div { class: "installed_list",
                    for m in mods() {
                        InstalledRow { key: "{m.folder}", entry: m.clone(), sd_root: sd_root.clone(), mods }
                    }
                }
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
fn InstalledRow(entry: InstalledMod, sd_root: PathBuf, mods: Signal<Vec<InstalledMod>>) -> Element {
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
                    span { class: "installed_name", "{entry.name}" }
                    span { class: chip_class, "{chip_label}" }
                    if !entry.has_config {
                        span { class: "chip warn", "No config" }
                    }
                }
                div { class: "installed_meta",
                    if let Some(author) = entry.author.clone() {
                        "by {author} · "
                    }
                    code { "{entry.folder}" }
                }
            }
            div { class: "installed_row_actions",
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
