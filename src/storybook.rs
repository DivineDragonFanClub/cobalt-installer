// Dev-only component gallery ("storybook"): renders UI pieces in fixed states from canned
// fixture data, so every visual state is reachable instantly — no network, no timing races,
// no installs. Only compiled into debug desktop builds; the nav entry lives in main.rs.
//
// The fixture description deliberately exercises the sanitizer (color span, links, a script
// tag that must be defused), and the fixture files exercise version grouping (two 1.2.0
// alternates on top, an older version behind the fold).

use std::collections::HashSet;

use dioxus::prelude::*;

use crate::gamebanana::ModDetail;
use crate::icons;
use crate::mods_ui::ModDetailPanel;

const STORIES: [&str; 5] =
    ["Detail: loaded", "Detail: skeleton", "Buttons", "Pills", "Update banner"];

// A real ProfilePage response (mod 702023, captured 2026-08-11), deserialized through the
// exact same serde path production uses — the fixture can't drift from the API's data model.
fn fixture_detail() -> ModDetail {
    serde_json::from_str(include_str!("storybook_fixture.json"))
        .expect("storybook_fixture.json parses as ModDetail")
}

#[component]
pub fn Storybook() -> Element {
    let mut story = use_signal(|| STORIES[0].to_string());

    rsx! {
        div { id: "storybook",
            div { class: "sb_chips",
                for s in STORIES {
                    button {
                        class: if story() == s { "ghost sb_active" } else { "ghost" },
                        onclick: move |_| story.set(s.to_string()),
                        "{s}"
                    }
                }
            }
            // Keyed by story so switching chips always tears down the previous story's
            // tree — component state must never bleed across stories.
            div { class: "sb_stage", key: "{story}",
                match story().as_str() {
                    "Detail: loaded" => rsx! { ModalStory { hold_loading: false } },
                    "Detail: skeleton" => rsx! { ModalStory { hold_loading: true } },
                    "Buttons" => rsx! { ButtonsStory {} },
                    "Pills" => rsx! { PillsStory {} },
                    _ => rsx! { BannerStory {} },
                }
            }
        }
    }
}

// Launches the real ModDetailPanel — overlay, Escape, scroll lock and all — on the
// fixture, or held in its loading state forever for the skeleton.
#[component]
fn ModalStory(hold_loading: bool) -> Element {
    let mut open = use_signal(|| false);
    let installed = use_signal(HashSet::<u64>::new);

    rsx! {
        div { class: "sb_rows",
            div { class: "sb_row",
                button {
                    class: "primary",
                    onclick: move |_| open.set(true),
                    if hold_loading { "Open modal (stuck loading)" } else { "Open modal" }
                }
            }
        }
        if open() {
            ModDetailPanel {
                mod_id: 702_023,
                // A scratch dir: a stray Install click extracts somewhere harmless.
                store: crate::storage::ModStore::new(&std::env::temp_dir().join("cobalt_storybook")),
                installed,
                on_close: move |_| open.set(false),
                fixture: (!hold_loading).then(fixture_detail),
                hold_loading,
            }
        }
    }
}

#[component]
fn ButtonsStory() -> Element {
    let sprites = [
        ("Base M", crate::SPRITE_LUEUR),
        ("Base F", crate::SPRITE_LUEUR_F),
        ("Promote M", crate::SPRITE_LUEUR_PROMO),
        ("Promote F", crate::SPRITE_LUEUR_F_PROMO),
        ("Fell M", crate::SPRITE_FELL),
        ("Fell F", crate::SPRITE_FELL_F),
    ];
    rsx! {
        div { class: "sb_rows",
            // The hero button with every sprite the install views cycle through
            // (both vars pinned to one sprite so the theme doesn't swap it away).
            div { class: "sb_row",
                for (label, sprite) in sprites {
                    div { class: "sb_sprite_demo",
                        button {
                            class: "engage",
                            style: "--hero-sprite-light: url('{sprite}'); --hero-sprite-dark: url('{sprite}')",
                            "Install Cobalt"
                        }
                        span { class: "sb_cap", "{label}" }
                    }
                }
            }
            div { class: "sb_row",
                button { class: "primary", "Primary" }
                button { class: "secondary", "Secondary" }
            }
            div { class: "sb_row",
                button { class: "danger", "Danger" }
                button { class: "ghost", "Ghost" }
                button { class: "danger_outline", "Danger outline" }
                button { class: "close", {icons::x(15)} }
            }
            div { class: "sb_row",
                button { disabled: true, class: "primary", "Primary disabled" }
                button { disabled: true, class: "secondary", "Secondary disabled" }
            }
        }
    }
}

#[component]
fn PillsStory() -> Element {
    rsx! {
        div { class: "sb_rows",
            div { class: "sb_row",
                span { class: "pill installed", {icons::check(11)} "Installed" }
                span { class: "pill featured", {icons::star(11)} "Featured" }
                span { class: "pill adult", "18+" }
                span { class: "pill latest", "1.2.0" }
                span { class: "pill latest", "Latest" }
                span { class: "pill ver", "classes only" }
                span { class: "pill ver", "5.3" }
            }
        }
    }
}

#[component]
fn BannerStory() -> Element {
    rsx! {
        div { class: "sb_rows",
            div { class: "nxm_banner update_banner",
                span { "Cobalt Manager 9.9.9 is available." }
                button { class: "primary", "Update & restart" }
                button { class: "close", {icons::x(15)} }
            }
        }
    }
}
