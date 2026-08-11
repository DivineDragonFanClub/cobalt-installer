// The "starter pack" (desktop only): a friendly first-run offer to install a curated set of mods.
//
// The mods come from a GameBanana collection (see gamebanana::STARTER_COLLECTION), so the pack is
// curated on the website, not baked into the build. Shown in two spots: right after a fresh Cobalt
// install during onboarding, and on the My Mods page when nothing is installed yet.
//
// Cards are the same ones the Browse tab uses (thumbnail, category, stats), so clicking a card opens
// the full detail overlay (description + screenshots + per-file install). A checkbox in the corner
// picks it for the batch: tick the mods you want (all ticked by default) and hit "Install selected".

use std::collections::{HashMap, HashSet};
use std::path::PathBuf;

use dioxus::prelude::*;

use crate::gamebanana::{self, Listing};
use crate::install;

// Per-mod install state, keyed by mod id in a shared map so a card and the batch installer agree.
// Done is also mirrored in the `installed` set (which drives the card's "Installed" badge); this map
// is what remembers a Failed attempt so we can nudge the user to retry.
#[derive(Clone, PartialEq)]
enum ItemState {
    Working,
    Done,
    Failed,
}

// Download + install one starter mod. Fetches the mod's page, picks its newest file, and unzips it
// into engage/mods. Progress shows on the button (these are small), so we ignore download bytes.
async fn install_one(
    sd: PathBuf,
    id: u64,
    mut statuses: Signal<HashMap<u64, ItemState>>,
    mut installed: Signal<HashSet<u64>>,
) {
    statuses.write().insert(id, ItemState::Working);
    let detail = match gamebanana::detail(id).await {
        Ok(d) => d,
        Err(_) => {
            statuses.write().insert(id, ItemState::Failed);
            return;
        }
    };
    let Some(file) = detail.primary_file().cloned() else {
        statuses.write().insert(id, ItemState::Failed);
        return;
    };
    // One transactional install: the mod plus any FE Engage GameBanana mods it requires, all-or-
    // nothing. The starter pack shows a single spinner, so we don't need the per-step phases here.
    match crate::downloads::install_transactional(&sd, &detail, &file, |_| {}).await {
        Ok(()) => {
            statuses.write().insert(id, ItemState::Done);
            // Re-scan so any required mods installed alongside also show as installed.
            installed.set(install::installed_gamebanana_ids(&sd).into_keys().collect());
        }
        Err(_) => {
            statuses.write().insert(id, ItemState::Failed);
        }
    }
}

#[component]
pub fn StarterPack(sd_root: PathBuf, on_close: EventHandler<()>) -> Element {
    // The collection's mods, fetched once.
    let pack = use_resource(|| async {
        gamebanana::collection_items(gamebanana::STARTER_COLLECTION, 1).await
    });

    // Which of these are already in engage/mods, so we don't offer them and can badge them. Grows as
    // installs finish.
    let installed_root = sd_root.clone();
    let installed = use_signal(move || {
        install::installed_gamebanana_ids(&installed_root)
            .into_keys()
            .collect::<HashSet<u64>>()
    });
    let statuses = use_signal(HashMap::<u64, ItemState>::new);
    // The user's un-ticked mods. Everything starts ticked, so tracking the exceptions keeps this in
    // sync with a list that's still loading (no need to seed it from the fetched ids).
    let mut deselected = use_signal(HashSet::<u64>::new);
    // True while a batch install runs, so the checkboxes lock and the button shows progress.
    let mut installing = use_signal(|| false);
    // The mod whose detail overlay is open, if any.
    let mut detail_open = use_signal(|| None::<u64>);

    rsx! {
        div { class: "starter",
            div { class: "starter_head",
                h2 { "Start with a few mods" }
                p { class: "starter_sub",
                    "New to modding Engage? Here's a hand-picked pack to get you going. Click a mod to \
                     read more, tick the ones you want, and install them in one go."
                }
            }

            match &*pack.read() {
                None => rsx! {
                    div { class: "starter_list",
                        for i in 0..5 {
                            div { key: "{i}", class: "starter_row skeleton",
                                div { class: "starter_row_thumb skeleton_box" }
                                div { class: "starter_row_info",
                                    div { class: "skeleton_line w40" }
                                    div { class: "skeleton_line w70" }
                                    div { class: "skeleton_line w55" }
                                }
                            }
                        }
                    }
                },
                Some(Err(e)) => rsx! {
                    div { class: "mod_message error", "Couldn't load the starter pack: {e}" }
                    div { class: "starter_actions",
                        button { class: "ghost", onclick: move |_| on_close.call(()), "Skip for now" }
                    }
                },
                Some(Ok(list)) => {
                    // Keep the pack beginner-safe: leave content-rated mods out of it entirely.
                    let mods: Vec<Listing> =
                        list.iter().filter(|m| !m.has_content_ratings).cloned().collect();
                    // Mods still up for grabs (not already installed), and of those the ticked ones.
                    let pending: Vec<u64> =
                        mods.iter().map(|m| m.id).filter(|id| !installed().contains(id)).collect();
                    let to_install: Vec<u64> =
                        pending.iter().copied().filter(|id| !deselected().contains(id)).collect();
                    let none_pending = pending.is_empty();
                    // Show "Select all" only when the user has actually un-ticked something.
                    let some_unticked = to_install.len() < pending.len();
                    // A failed attempt that isn't installed: worth telling the user so they retry.
                    let any_failed = mods.iter().any(|m| {
                        statuses().get(&m.id) == Some(&ItemState::Failed) && !installed().contains(&m.id)
                    });
                    rsx! {
                        div { class: "starter_list",
                            for m in mods.clone() {
                                StarterRow {
                                    key: "{m.id}",
                                    listing: m.clone(),
                                    statuses,
                                    installed,
                                    deselected,
                                    installing: installing(),
                                    on_details: move |id| detail_open.set(Some(id)),
                                }
                            }
                        }
                        if any_failed {
                            div { class: "starter_failnote",
                                "Some mods couldn't be installed. They're still selected, so you can try again."
                            }
                        }
                        div { class: "starter_actions",
                            if !none_pending {
                                button {
                                    class: "engage",
                                    disabled: to_install.is_empty() || installing(),
                                    onclick: {
                                        let sd = sd_root.clone();
                                        let ids = to_install.clone();
                                        move |_| {
                                            let sd = sd.clone();
                                            let ids = ids.clone();
                                            installing.set(true);
                                            spawn(async move {
                                                for id in ids {
                                                    install_one(sd.clone(), id, statuses, installed).await;
                                                }
                                                installing.set(false);
                                            });
                                        }
                                    },
                                    if installing() {
                                        crate::mods_ui::Spinner {}
                                        "Installing…"
                                    } else {
                                        "Install selected ({to_install.len()})"
                                    }
                                }
                                if some_unticked && !installing() {
                                    button {
                                        class: "ghost",
                                        onclick: move |_| deselected.write().clear(),
                                        "Select all"
                                    }
                                }
                            }
                            button {
                                class: "ghost",
                                disabled: installing(),
                                onclick: move |_| on_close.call(()),
                                if none_pending { "Done" } else { "Skip for now" }
                            }
                        }
                    }
                }
            }
        }

        // Full detail overlay, reused from the Browse tab: description, screenshots, per-file install.
        if let Some(id) = detail_open() {
            crate::mods_ui::ModDetailPanel {
                key: "{id}",
                mod_id: id,
                sd_root: sd_root.clone(),
                installed,
                on_close: move |_| detail_open.set(None),
                // Installing in the pack is driven by the row checkboxes, so keep the modal read-only.
                install_disabled: true,
            }
        }
    }
}

#[component]
fn StarterRow(
    listing: Listing,
    statuses: Signal<HashMap<u64, ItemState>>,
    installed: Signal<HashSet<u64>>,
    mut deselected: Signal<HashSet<u64>>,
    installing: bool,
    on_details: EventHandler<u64>,
) -> Element {
    let id = listing.id;
    let state = statuses().get(&id).cloned();
    let done = installed().contains(&id);
    let working = state == Some(ItemState::Working);
    let failed = !done && state == Some(ItemState::Failed);
    let ticked = !done && !deselected().contains(&id);

    // The list record has no description, so pull the mod's page lazily just for its tagline. The row
    // renders right away with everything else; while the fetch is in flight we show a shimmer line,
    // then swap in the tagline (or nothing, if the mod has none).
    let detail = use_resource(move || async move { gamebanana::detail(id).await });
    let (loading_desc, subtitle) = match &*detail.read() {
        None => (true, String::new()),
        Some(Ok(d)) => (false, d.subtitle.clone().unwrap_or_default()),
        Some(Err(_)) => (false, String::new()),
    };

    let row_class = if done {
        "starter_row done"
    } else if ticked {
        "starter_row ticked"
    } else {
        "starter_row"
    };

    rsx! {
        // Clicking the row opens the full detail overlay (description + screenshots + install).
        div { class: "{row_class}", onclick: move |_| on_details.call(id),
            // The checkbox picks the mod for the batch. stop_propagation so it doesn't open the overlay.
            label { class: "starter_pick", onclick: move |e| e.stop_propagation(),
                input {
                    r#type: "checkbox",
                    checked: done || ticked,
                    disabled: done || installing,
                    onchange: move |e| {
                        if e.checked() {
                            deselected.write().remove(&id);
                        } else {
                            deselected.write().insert(id);
                        }
                    },
                }
            }
            div { class: "starter_row_thumb",
                if let Some(thumb) = listing.thumb_url() {
                    img { src: "{thumb}", alt: "{listing.name}" }
                }
            }
            div { class: "starter_row_info",
                div { class: "starter_row_head",
                    span { class: "starter_row_name", "{listing.name}" }
                    if let Some(cat) = listing.category_name() {
                        span { class: "chip cat", "{cat}" }
                    }
                }
                if loading_desc {
                    div { class: "skeleton_line starter_desc_skel" }
                } else if !subtitle.trim().is_empty() {
                    div { class: "starter_row_desc", "{subtitle}" }
                }
                div { class: "starter_row_stats",
                    span { class: "stat", "by {listing.author()}" }
                    span { class: "stat",
                        {crate::icons::heart(12)}
                        "{gamebanana::format_count(listing.likes)}"
                    }
                    span { class: "stat",
                        {crate::icons::eye(12)}
                        "{gamebanana::format_count(listing.views)}"
                    }
                }
            }
            div { class: "starter_row_end",
                if done {
                    span { class: "ok", {crate::icons::check(14)} "Installed" }
                } else if working {
                    span { class: "muted",
                        crate::mods_ui::Spinner {}
                        "Installing…"
                    }
                } else if failed {
                    span { class: "err", "Failed" }
                }
            }
        }
    }
}
