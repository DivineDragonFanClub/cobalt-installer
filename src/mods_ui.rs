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

// NexusMods browsing/installing isn't finished, so it's hidden for now: the source toggle is gone
// and everything is forced to GameBanana. Flip this to `true` to bring the toggle (and Nexus) back.
pub(crate) const NEXUS_ENABLED: bool = false;

#[component]
pub fn ModBrowser(sd_root: PathBuf) -> Element {
    let mut source = use_signal(|| Source::GameBanana);

    rsx! {
        if NEXUS_ENABLED {
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
    let mut sort = use_signal(|| gamebanana::SORTS[0].0.to_string());
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
    let mut installed = use_signal(move || {
        install::installed_gamebanana_ids(&installed_root)
            .into_keys()
            .collect::<HashSet<u64>>()
    });

    // A background install (from the Body coroutine) finishes silently by dropping out of the shared
    // list, so re-scan what's on disk whenever the set of in-progress ids changes. That picks up a
    // just-finished mod for the card badge and flips the detail modal's button to "Reinstall".
    let downloads = use_context::<crate::downloads::Downloads>();
    let active_ids = use_memo(move || {
        let mut ids: Vec<u64> = downloads().iter().map(|d| d.id).collect();
        ids.sort();
        ids
    });
    let rescan_root = sd_root.clone();
    use_effect(move || {
        let _ = active_ids();
        installed.set(install::installed_gamebanana_ids(&rescan_root).into_keys().collect());
    });

    // Whenever the search text, category, or sort changes, start over from page 1.
    use_effect(move || {
        let q = query().trim().to_string();
        let cat = category();
        let s = sort();
        results.set(Vec::new());
        page.set(1);
        has_more.set(true);
        error.set(None);
        loading.set(true);
        spawn(async move {
            match fetch(&q, cat, &s, 1).await {
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
        let s = sort();
        let next = page() + 1;
        loading.set(true);
        spawn(async move {
            match fetch(&q, cat, &s, next).await {
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
                select {
                    class: "category_select",
                    value: "{sort}",
                    // A text search comes back by relevance, so sort only applies while browsing.
                    disabled: query().trim().len() >= 2,
                    onchange: move |e| sort.set(e.value()),
                    for (val, label) in gamebanana::SORTS.iter() {
                        option { value: "{val}", "{label}" }
                    }
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
            } else if loading() {
                // First page (fresh browse or a new search): show placeholder cards, not a bare screen.
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
                } else if !results().is_empty() && has_more() {
                    button { class: "secondary", onclick: load_more, "Load more" }
                } else if results().is_empty() && !loading() && error().is_none() {
                    span { class: "mod_message", "No mods found." }
                }
            }
        }

        if let Some(id) = selected() {
            ModDetailPanel {
                key: "{id}",
                mod_id: id,
                sd_root: sd_root.clone(),
                installed,
                on_close: move |_| selected.set(None),
            }
        }
    }
}

// Pick the right API call for the current filters: text search wins, then category, else the feed.
// The sort only applies to the browse/category listings, a text search comes back by relevance.
async fn fetch(query: &str, category: Option<u64>, sort: &str, page: u32) -> anyhow::Result<Vec<Listing>> {
    if query.len() >= 2 {
        gamebanana::search(query, page).await
    } else if let Some(cat) = category {
        gamebanana::by_category(cat, page, sort).await
    } else {
        gamebanana::browse(page, sort).await
    }
}

// A small spinning circle, for buttons that are busy (shared with the Nexus browser).
#[component]
pub fn Spinner() -> Element {
    rsx! { span { class: "spinner" } }
}

// A shimmering placeholder shaped like a mod card, shown while the first page loads so the grid
// doesn't pop in from an empty screen. Shared with the Nexus browser.
#[component]
pub fn SkeletonCard() -> Element {
    rsx! {
        div { class: "mod_card skeleton",
            div { class: "skeleton_box" }
            div { class: "mod_card_body",
                div { class: "skeleton_line w70" }
                div { class: "skeleton_line w40" }
                div { class: "skeleton_line w55" }
            }
        }
    }
}

#[component]
pub(crate) fn ModCard(listing: Listing, installed: bool, show_nsfw: bool, on_open: EventHandler<()>) -> Element {
    let blur = listing.has_content_ratings && !show_nsfw;
    rsx! {
        div { class: "mod_card", onclick: move |_| on_open.call(()),
            div { class: "mod_card_media",
                div { class: if blur { "mod_thumb blurred" } else { "mod_thumb" },
                    if let Some(thumb) = listing.thumb_url() {
                        img { src: "{thumb}", alt: "{listing.name}" }
                    }
                }
                // Fades the image into the card body so the text below sits on a soft gradient.
                div { class: "mod_card_scrim" }
                div { class: "mod_card_pills",
                    if installed {
                        span { class: "pill installed", {crate::icons::check(12)} "Installed" }
                    }
                    if listing.featured {
                        span { class: "pill featured", {crate::icons::star(11)} "Featured" }
                    }
                    if listing.has_content_ratings {
                        span { class: "pill adult", "18+" }
                    }
                }
                if let Some(cat) = listing.category_name() {
                    span { class: "mod_card_cat", "{cat}" }
                }
            }
            div { class: "mod_card_body",
                div { class: "mod_card_title", "{listing.name}" }
                div { class: "mod_card_author", "by {listing.author()}" }
                div { class: "mod_card_stats",
                    span { class: "stat", title: "Likes",
                        {crate::icons::heart(13)}
                        "{gamebanana::format_count(listing.likes)}"
                    }
                    span { class: "stat", title: "Views",
                        {crate::icons::eye(13)}
                        "{gamebanana::format_count(listing.views)}"
                    }
                    if !listing.version.is_empty() {
                        span { class: "stat ver", "v{listing.version}" }
                    }
                }
            }
        }
    }
}

#[component]
pub(crate) fn ModDetailPanel(
    mod_id: u64,
    sd_root: PathBuf,
    installed: Signal<HashSet<u64>>,
    on_close: EventHandler<()>,
    // Storybook hooks, defaulted away in production: a preloaded detail instead of the
    // network fetch, and a fetch that never resolves (to hold the skeleton state).
    #[props(default)] fixture: Option<ModDetail>,
    #[props(default)] hold_loading: bool,
    // Grey out the per-file install buttons. The starter pack opens this modal just to read the
    // description, since installing there goes through the row checkboxes instead.
    #[props(default)] install_disabled: bool,
) -> Element {
    let fixture_for_fetch = fixture.clone();
    let detail = use_resource(move || {
        let fixture = fixture_for_fetch.clone();
        async move {
            if hold_loading {
                std::future::pending::<()>().await;
            }
            match fixture {
                Some(d) => Ok(d),
                None => gamebanana::detail(mod_id).await,
            }
        }
    });
    // Which screenshot the lightbox is showing, if any. Lives here (not in the content
    // component) so the keyboard loop below can drive it.
    let mut lightbox = use_signal(|| None::<usize>);

    // Keyboard + scroll lock for the modal's lifetime. The webview holds keyboard focus, so
    // the listener lives in the page: registered on mount, undone on unmount. Escape closes
    // the lightbox if one is up, else the modal; arrow keys page the lightbox (they're only
    // forwarded while it's open, so normal panel scrolling keeps them otherwise). The
    // modal_open class freezes the root scroller behind the modal.
    use_future(move || async move {
        let mut keys = document::eval(
            // The gutter compensation is measured, not assumed: classic scrollbars occupy
            // 11px, but macOS may hand the webview zero-width overlay scrollbars instead.
            "const gutter = window.innerWidth - document.documentElement.clientWidth;\n\
             document.documentElement.style.setProperty('--scroll-gutter', gutter + 'px');\n\
             document.documentElement.classList.add('modal_open');\n\
             window.__esc_close = (e) => {\n\
                 if (!['Escape', 'ArrowLeft', 'ArrowRight'].includes(e.key)) return;\n\
                 if (e.key !== 'Escape' && !document.querySelector('.lightbox_overlay')) return;\n\
                 e.preventDefault();\n\
                 dioxus.send(e.key);\n\
             };\n\
             document.addEventListener('keydown', window.__esc_close);",
        );
        while let Ok(key) = keys.recv::<String>().await {
            match key.as_str() {
                "Escape" => {
                    if lightbox().is_some() {
                        lightbox.set(None);
                    } else {
                        on_close.call(());
                    }
                }
                "ArrowLeft" | "ArrowRight" => {
                    let count = detail
                        .read()
                        .as_ref()
                        .and_then(|r| r.as_ref().ok())
                        .map(|d| d.preview.images.len())
                        .unwrap_or(0);
                    if let Some(i) = lightbox() {
                        if count > 0 {
                            let i = i.min(count - 1);
                            let next = if key == "ArrowRight" {
                                (i + 1) % count
                            } else {
                                (i + count - 1) % count
                            };
                            lightbox.set(Some(next));
                        }
                    }
                }
                _ => {}
            }
        }
    });
    use_drop(|| {
        document::eval(
            "document.documentElement.classList.remove('modal_open');\n\
             document.removeEventListener('keydown', window.__esc_close);\n\
             delete window.__esc_close;",
        );
    });

    rsx! {
        div { class: "mod_detail_overlay", onclick: move |_| on_close.call(()),
            div {
                class: "mod_detail_panel",
                // Clicks inside the panel shouldn't close it.
                onclick: move |e| e.stop_propagation(),
                // Loaded content carries its own close in the header row; the corner X
                // only covers the header-less loading/error states.
                if !matches!(&*detail.read(), Some(Ok(_))) {
                    button { class: "close", onclick: move |_| on_close.call(()), {crate::icons::x(15)} }
                }
                div { class: "mod_detail_scroll",
                    match &*detail.read() {
                        Some(Ok(d)) => rsx! {
                            ModDetailContent { detail: d.clone(), sd_root: sd_root.clone(), installed, lightbox, on_close, install_disabled }
                        },
                        Some(Err(e)) => rsx! {
                            div { class: "mod_message error", "Couldn't load this mod: {e}" }
                        },
                        None => rsx! {
                            DetailSkeleton {}
                        },
                    }
                }
            }
        }
    }
}

// Shaped like the loaded layout so the card opens at roughly its final size instead of
// flashing a sliver that then jumps. Also a storybook story.
#[component]
pub(crate) fn DetailSkeleton() -> Element {
    rsx! {
        div { class: "detail_skeleton",
            div { class: "skeleton_line title" }
            div { class: "skeleton_line meta" }
            div { class: "skeleton_screens",
                div { class: "skeleton_box shot" }
                div { class: "skeleton_box shot" }
                div { class: "skeleton_box shot" }
            }
            div { class: "skeleton_line w90" }
            div { class: "skeleton_line w70" }
            div { class: "skeleton_line w55" }
            div { class: "skeleton_box filerow" }
        }
    }
}

#[component]
pub(crate) fn ModDetailContent(
    detail: ModDetail,
    sd_root: PathBuf,
    mut installed: Signal<HashSet<u64>>,
    mut lightbox: Signal<Option<usize>>,
    on_close: EventHandler<()>,
    #[props(default)] install_disabled: bool,
) -> Element {
    let description = gamebanana::sanitize_description(&detail.description_html);
    let subtitle = detail.subtitle.clone().unwrap_or_default();
    let is_installed = installed().contains(&detail.id);
    // Files grouped by their version tag: one release often ships several alternate zips (main +
    // addon, classes-only vs full), and none of those are "older" than each other. Tags are
    // modder-typed free text ("5.4", "classes only"), so they're matched, never parsed as semver.
    // Untagged files stay individual — nothing says they belong together. The current release is
    // the group matching the mod's own declared version (an auxiliary variant uploaded later must
    // not displace it); when no group matches, newest upload wins.
    let mut files = detail.files.clone();
    files.sort_by_key(|f| std::cmp::Reverse(f.date_added));
    let mut groups: Vec<Vec<ModFile>> = Vec::new();
    for f in files.iter().cloned() {
        match groups.iter_mut().find(|g| !f.version.is_empty() && g[0].version == f.version) {
            Some(g) => g.push(f),
            None => groups.push(vec![f]),
        }
    }
    let current_idx = groups
        .iter()
        .position(|g| !detail.version.is_empty() && g[0].version == detail.version)
        .unwrap_or(0);
    let current: Vec<ModFile> = groups.get(current_idx).cloned().unwrap_or_default();
    let other: Vec<ModFile> = groups
        .iter()
        .enumerate()
        .filter(|(i, _)| *i != current_idx)
        .flat_map(|(_, g)| g.clone())
        .collect();
    // Badge rule: a version tag labels each file of the current release; a bare "Latest" only
    // makes sense when a single file stands above other versions.
    let current_badge = |f: &ModFile| -> Option<String> {
        if !f.version.is_empty() {
            Some(f.version.clone())
        } else if current.len() == 1 && !other.is_empty() {
            Some("Latest".into())
        } else {
            None
        }
    };
    let mut meta = format!(
        "by {} · {} likes · {} views · published {}",
        detail.author(),
        detail.likes,
        detail.views,
        gamebanana::format_date(detail.date_added),
    );
    // Only mention an update when it's a different day than the release; same-day edits are noise.
    if detail.date_modified / 86_400 > detail.date_added / 86_400 {
        meta.push_str(&format!(" · updated {}", gamebanana::format_date(detail.date_modified)));
    }
    rsx! {
        div { class: "mod_detail_head",
            div {
                h2 { class: "mod_detail_title", "{detail.name}" }
                if !subtitle.is_empty() {
                    div { class: "mod_detail_subtitle", "{subtitle}" }
                }
                div { class: "mod_detail_meta", "{meta}" }
            }
            div { class: "mod_detail_actions",
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
                button { class: "close", onclick: move |_| on_close.call(()), {crate::icons::x(15)} }
            }
        }

        if !detail.preview.images.is_empty() {
            div { class: "mod_screens",
                // Every screenshot the mod has, but as the 530px pre-renders: the strip shows
                // them 160px tall, and a dozen full-size originals per open is real weight.
                // Clicking one opens the lightbox on it.
                for (i, img) in detail.preview.images.iter().enumerate() {
                    img {
                        src: "{img.thumb_url()}",
                        alt: img.caption.clone().unwrap_or_default(),
                        onclick: move |_| lightbox.set(Some(i)),
                    }
                }
            }
        }

        if let Some(idx) = lightbox() {
            {
                let images = detail.preview.images.clone();
                let n = images.len();
                let i = idx.min(n.saturating_sub(1));
                let img = images[i].clone();
                let caption = img.caption.clone().unwrap_or_default();
                rsx! {
                    div { class: "lightbox_overlay", onclick: move |_| lightbox.set(None),
                        if n > 1 {
                            button {
                                class: "lb_nav",
                                onclick: move |e| {
                                    e.stop_propagation();
                                    lightbox.set(Some((i + n - 1) % n));
                                },
                                "‹"
                            }
                        }
                        div { class: "lightbox_stage", onclick: move |e| e.stop_propagation(),
                            img { src: "{img.full_url()}", alt: "{caption}" }
                            if !caption.is_empty() || n > 1 {
                                // Hangs below the image without occupying layout space, so the
                                // image stays dead-center whether or not a caption exists.
                                div { class: "lb_meta",
                                    if !caption.is_empty() {
                                        div { class: "lb_caption", "{caption}" }
                                    }
                                    if n > 1 {
                                        div { class: "lb_count", "{i + 1} / {n}" }
                                    }
                                }
                            }
                        }
                        if n > 1 {
                            button {
                                class: "lb_nav",
                                onclick: move |e| {
                                    e.stop_propagation();
                                    lightbox.set(Some((i + 1) % n));
                                },
                                "›"
                            }
                        }
                    }
                }
            }
        }

        if !description.trim().is_empty() {
            div { class: "mod_desc mod_desc_html", dangerous_inner_html: "{description}" }
        }

        div { class: "mod_files",
            if files.is_empty() {
                div { class: "mod_message", "No downloadable files on this mod." }
            }
            for f in current.clone() {
                InstallRow {
                    key: "{f.id}",
                    detail: detail.clone(),
                    file: f.clone(),
                    sd_root: sd_root.clone(),
                    installed,
                    badge: current_badge(&f),
                    install_disabled,
                }
            }
            if !other.is_empty() {
                details { class: "older_files",
                    summary { "Other versions ({other.len()})" }
                    div { class: "older_list",
                        for f in other.clone() {
                            InstallRow {
                                key: "{f.id}",
                                detail: detail.clone(),
                                file: f.clone(),
                                sd_root: sd_root.clone(),
                                installed,
                                badge: (!f.version.is_empty()).then(|| f.version.clone()),
                                muted_badge: true,
                                install_disabled,
                            }
                        }
                    }
                }
            }
        }

        a { class: "mod_page_link gb_link", href: "{detail.profile_url}", "Open on GameBanana" }
    }
}

#[component]
fn InstallRow(
    detail: ModDetail,
    file: ModFile,
    sd_root: PathBuf,
    installed: Signal<HashSet<u64>>,
    // Text for the pill next to the filename: the file's version tag, or "Latest".
    #[props(default)] badge: Option<String>,
    // Grey pill instead of blue — for other-version rows, where the tag is context, not a callout.
    #[props(default)] muted_badge: bool,
    // Grey out the install button (the starter pack drives installs from its checkboxes instead).
    #[props(default)] install_disabled: bool,
) -> Element {
    // Installs run in the Body coroutine (so closing this modal doesn't cancel them). We just hand it
    // a request and read this mod's progress back out of the shared list.
    let installer = use_coroutine_handle::<crate::downloads::InstallRequest>();
    let downloads = use_context::<crate::downloads::Downloads>();
    let phase = downloads().into_iter().find(|d| d.id == detail.id).map(|d| d.phase);
    // Show "Reinstall" if this mod is already installed, since installing replaces it.
    let label = if installed().contains(&detail.id) { "Reinstall" } else { "Install" };

    // Fire a download+install request off to the coroutine.
    let start = {
        let detail = detail.clone();
        let file = file.clone();
        let sd_root = sd_root.clone();
        move |_| {
            installer.send(crate::downloads::InstallRequest {
                detail: detail.clone(),
                file: file.clone(),
                sd_root: sd_root.clone(),
            });
        }
    };

    rsx! {
        div { class: "install_row",
            div { class: "file_info",
                div { class: "file_name",
                    "{file.filename} ({gamebanana::format_filesize(file.filesize)})"
                    if let Some(b) = badge.clone() {
                        span { class: if muted_badge { "pill ver" } else { "pill latest" }, "{b}" }
                    }
                }
                if !file.description.is_empty() {
                    div { class: "file_desc", "{file.description}" }
                }
            }
            match phase {
                Some(crate::downloads::Phase::Downloading { what, done, total }) => {
                    let pct = (done * 100).checked_div(total).unwrap_or(0);
                    rsx! {
                        div { class: "progress",
                            div { class: "bar", style: "width: {pct}%" }
                        }
                        span { class: "progress_label",
                            if what.is_empty() {
                                "{gamebanana::format_filesize(done)} / {gamebanana::format_filesize(total)}"
                            } else {
                                "{what} · {gamebanana::format_filesize(done)} / {gamebanana::format_filesize(total)}"
                            }
                        }
                    }
                }
                Some(crate::downloads::Phase::Working { what }) => rsx! {
                    span { class: "muted",
                        Spinner {}
                        " {what}"
                    }
                },
                Some(crate::downloads::Phase::Error(e)) => rsx! {
                    div { class: "errline",
                        span { class: "err", "Error: {e}" }
                        button { onclick: start, "Retry" }
                    }
                },
                None => rsx! {
                    button {
                        class: "primary",
                        disabled: install_disabled,
                        title: if install_disabled { "Pick this mod with its checkbox in the starter pack to install it" } else { "" },
                        onclick: start,
                        "{label}"
                    }
                },
            }
        }
    }
}
