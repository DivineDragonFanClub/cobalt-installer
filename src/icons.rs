// Inline SVG icons (desktop only) so the UI uses real icons instead of emoji. Feather/lucide style:
// a 24x24 grid, stroke follows the current text color so an icon matches whatever text it sits next
// to. Each function takes a pixel size. Filled icons (heart, star, check-circle) set fill instead.

use dioxus::prelude::*;

// Shared wrapper for the stroke-based icons: builds the <svg> and drops the icon's shapes inside.
fn stroke(size: u32, children: Element) -> Element {
    rsx! {
        svg {
            class: "icon",
            width: "{size}",
            height: "{size}",
            view_box: "0 0 24 24",
            fill: "none",
            stroke: "currentColor",
            stroke_width: "2",
            stroke_linecap: "round",
            stroke_linejoin: "round",
            {children}
        }
    }
}

// A parcel box. Used for the empty state. (The sidebar now uses game-texture
// icons masked in CSS, see ICON_* in main.rs.)
pub fn package(size: u32) -> Element {
    stroke(size, rsx! {
        path { d: "m7.5 4.27 9 5.15" }
        path { d: "M21 8a2 2 0 0 0-1-1.73l-7-4a2 2 0 0 0-2 0l-7 4A2 2 0 0 0 3 8v8a2 2 0 0 0 1 1.73l7 4a2 2 0 0 0 2 0l7-4A2 2 0 0 0 21 16Z" }
        path { d: "m3.3 7 8.7 5 8.7-5" }
        path { d: "M12 22V12" }
    })
}

// Padlock. Shown on locked nav items before Cobalt is installed.
pub fn lock(size: u32) -> Element {
    stroke(size, rsx! {
        rect { width: "18", height: "11", x: "3", y: "11", rx: "2", ry: "2" }
        path { d: "M7 11V7a5 5 0 0 1 10 0v4" }
    })
}

// An eye, for view counts.
pub fn eye(size: u32) -> Element {
    stroke(size, rsx! {
        path { d: "M2 12s3-7 10-7 10 7 10 7-3 7-10 7-10-7-10-7Z" }
        circle { cx: "12", cy: "12", r: "3" }
    })
}

// Filled heart, for like counts.
pub fn heart(size: u32) -> Element {
    rsx! {
        svg {
            class: "icon",
            width: "{size}",
            height: "{size}",
            view_box: "0 0 24 24",
            fill: "currentColor",
            stroke: "none",
            path { d: "M19 14c1.49-1.46 3-3.21 3-5.5A5.5 5.5 0 0 0 16.5 3c-1.76 0-3 .5-4.5 2-1.5-1.5-2.74-2-4.5-2A5.5 5.5 0 0 0 2 8.5c0 2.3 1.5 4.05 3 5.5l7 7Z" }
        }
    }
}

// Filled star, for the "featured" pill.
pub fn star(size: u32) -> Element {
    rsx! {
        svg {
            class: "icon",
            width: "{size}",
            height: "{size}",
            view_box: "0 0 24 24",
            fill: "currentColor",
            stroke: "none",
            polygon { points: "12 2 15.09 8.26 22 9.27 17 14.14 18.18 21.02 12 17.77 5.82 21.02 7 14.14 2 9.27 8.91 8.26 12 2" }
        }
    }
}

// A check mark, for the "installed" pill.
pub fn check(size: u32) -> Element {
    stroke(size, rsx! {
        polyline { points: "20 6 9 17 4 12" }
    })
}

// A crisp ×, for close/dismiss buttons (the letter X reads as text, not a control).
pub fn x(size: u32) -> Element {
    stroke(size, rsx! {
        path { d: "M18 6 6 18" }
        path { d: "m6 6 12 12" }
    })
}

// Three lines, for the list-tools menu next to the sort select.
pub fn hamburger(size: u32) -> Element {
    stroke(size, rsx! {
        path { d: "M4 6h16" }
        path { d: "M4 12h16" }
        path { d: "M4 18h16" }
    })
}

// Three dots, for the row overflow menu (a text ⋯ never sits optically centered).
pub fn dots(size: u32) -> Element {
    rsx! {
        svg {
            class: "icon",
            width: "{size}",
            height: "{size}",
            view_box: "0 0 24 24",
            fill: "currentColor",
            stroke: "none",
            circle { cx: "5", cy: "12", r: "2" }
            circle { cx: "12", cy: "12", r: "2" }
            circle { cx: "19", cy: "12", r: "2" }
        }
    }
}

// (The theme toggle now uses the game's Day/Night/Evening icons, injected as
// background images from main.rs.)
