#![windows_subsystem = "windows"]

// Path handling is desktop only. On Android we never touch host file paths, the
// writing goes through the folder the user grants (see the `saf` module).
#[cfg(feature = "desktop")]
use std::path::{Path, PathBuf};

#[cfg(feature = "desktop")]
use dioxus::desktop::use_window;
// Catching the nxm:// links the OS hands us for NexusMods "Mod Manager Download".
#[cfg(feature = "desktop")]
use dioxus::desktop::{tao::event::Event, use_wry_event_handler};
use dioxus::{logger::tracing, prelude::*};

const FAVICON: Asset = asset!("/assets/favicon.ico");
const MAIN_CSS: Asset = asset!("/assets/main.css");
const SAMMIE: Asset = asset!("/assets/SAMMIE.png");

#[cfg(feature = "desktop")]
use dirs::home_dir;

#[cfg(feature = "desktop")]
use std::process::{Child, Command};

#[cfg(feature = "desktop")]
use std::io::{Read, Write};
#[cfg(feature = "desktop")]
use zip::ZipArchive;

#[cfg(feature = "desktop")]
use dioxus_sdk::storage::*;

// The GameBanana mod browser (browse/search, install, config.yaml generation) is desktop only for
// now. It's split into modules to keep main.rs readable, the Android build never pulls it in.
#[cfg(feature = "desktop")]
mod gamebanana;
#[cfg(feature = "desktop")]
mod icons;
#[cfg(feature = "desktop")]
mod install;
#[cfg(feature = "desktop")]
mod installed_ui;
#[cfg(feature = "desktop")]
mod mods_ui;
#[cfg(feature = "desktop")]
mod nexus;
#[cfg(feature = "desktop")]
mod nexus_ui;

// Everything about locating an emulator on the host filesystem is desktop only.
// On Android we don't hunt for install folders, the user hands us Eden's folder
// through the system picker instead (see the `saf` module below).
#[cfg(feature = "desktop")]
struct Emulator {
    name: &'static str,
    linux_data_path: &'static str,
    macos_data_path: &'static str,
    windows_data_folder: &'static str,
    sd_card_folder: &'static str,
}

#[cfg(feature = "desktop")]
impl Emulator {
    fn data_path(&self) -> Option<PathBuf> {
        match std::env::consts::OS {
            "macos" => home_dir().map(|h| h.join(self.macos_data_path)),
            "windows" => std::env::var_os("APPDATA").map(|a| PathBuf::from(a).join(self.windows_data_folder)),
            "linux" => home_dir().map(|h| h.join(self.linux_data_path)),
            other => todo!("Unsupported platform: {other}"),
        }
    }

    fn sd_card_path(&self) -> Option<PathBuf> {
        self.data_path().map(|p| p.join(self.sd_card_folder))
    }

    fn is_installed(&self) -> bool {
        self.data_path().map(|p| p.exists()).unwrap_or(false)
    }
}

#[cfg(feature = "desktop")]
static EMULATORS: &[Emulator] = &[
    Emulator {
        name: "Ryujinx",
        linux_data_path: ".config/Ryujinx",
        macos_data_path: "Library/Application Support/Ryujinx",
        windows_data_folder: "Ryujinx",
        sd_card_folder: "sdcard",
    },
    Emulator {
        name: "Citron",
        linux_data_path: ".local/share/citron", // I got this from the docs https://citron-emu.org/docs/installation
        macos_data_path: ".local/share/citron",
        windows_data_folder: "citron",
        sd_card_folder: "sdmc",
    },
    Emulator {
        name: "Eden",
        linux_data_path: ".local/share/eden", // Assuming based on how Eden has the same structure as Citron, it's not mentioned in the docs.
        macos_data_path: ".local/share/eden",
        windows_data_folder: "eden",
        sd_card_folder: "sdmc",
    },
];

#[cfg(feature = "desktop")]
fn get_emulator(name: &str) -> Option<&'static Emulator> {
    EMULATORS.iter().find(|e| e.name == name)
}

// The SD-card root for the current install choice, either an emulator's folder or a raw SD card.
// Both the installer and the mod browser resolve their target through this so they always agree.
#[cfg(feature = "desktop")]
fn resolve_sd_root(installation_type: &str, sdcard: &str) -> Option<std::path::PathBuf> {
    if installation_type == "SD Card" {
        if sdcard.is_empty() {
            None
        } else {
            Some(PathBuf::from(sdcard))
        }
    } else {
        get_emulator(installation_type).and_then(|e| e.sd_card_path())
    }
}

// Is Cobalt actually installed at this SD root? release.zip lays down engage/cobalt (Cobalt's own
// runtime folder), so its presence is a reliable marker. We don't check engage/mods because the
// installer creates that folder itself, so it'd be a false positive.
#[cfg(feature = "desktop")]
fn is_cobalt_installed(sd_root: &Path) -> bool {
    sd_root.join("engage").join("cobalt").is_dir()
}

fn main() {
    // Desktop and Android launch differently. Desktop wires up a data directory
    // and the local-storage backend, Android just hands the app to the mobile
    // renderer (no `dirs` paths, they come back None there).
    #[cfg(feature = "desktop")]
    {
        use dioxus::desktop::tao::{dpi::LogicalSize, window::WindowBuilder};

        dioxus_sdk::storage::set_dir!();
        // Open at a 16:9 size, and keep a 16:9 floor so the layout never gets squished.
        let window = WindowBuilder::new()
            .with_title("Cobalt Installer")
            .with_inner_size(LogicalSize::new(1280.0, 720.0))
            .with_min_inner_size(LogicalSize::new(960.0, 540.0));
        LaunchBuilder::new()
            .with_cfg(
                dioxus_desktop::Config::new()
                    .with_window(window)
                    .with_data_directory(dirs::data_local_dir().unwrap().join("CobaltInstaller")),
            )
            .launch(App);
    }

    #[cfg(target_os = "android")]
    dioxus::launch(App);
}

const RELEASE_URL: &str = "https://github.com/Raytwo/Cobalt/releases/latest/download/release.zip";

// On Android the target lives under Android/data, which is off limits to plain
// file access. All the writing happens on the Kotlin side (see android/MainActivity.kt),
// this module just calls those methods over JNI. The four method names and signatures
// here must match MainActivity.kt exactly.
#[cfg(target_os = "android")]
mod saf {
    use jni::objects::{JObject, JString, JValue};
    use jni::JavaVM;

    // Grab the JVM and our Activity from the Android runtime, attach this thread,
    // and run a small piece of JNI work against them.
    fn with_activity<R>(
        f: impl FnOnce(&mut jni::JNIEnv, &JObject) -> jni::errors::Result<R>,
    ) -> anyhow::Result<R> {
        let ctx = ndk_context::android_context();
        let vm = unsafe { JavaVM::from_raw(ctx.vm().cast()) }?;
        let activity = unsafe { JObject::from_raw(ctx.context().cast()) };
        let mut env = vm.attach_current_thread()?;
        let out = f(&mut env, &activity)?;
        Ok(out)
    }

    // Open the system folder picker so the user can grant Eden's folder.
    // Fire and forget, the result lands in SharedPreferences (poll persisted_tree_uri).
    pub fn request_tree_access() -> anyhow::Result<()> {
        with_activity(|env, activity| {
            env.call_method(activity, "requestTreeAccess", "()V", &[])?;
            Ok(())
        })
    }

    // Returns the previously granted folder URI, or None if the user hasn't picked yet.
    pub fn persisted_tree_uri() -> Option<String> {
        with_activity(|env, activity| {
            let value = env
                .call_method(activity, "getPersistedTreeUri", "()Ljava/lang/String;", &[])?
                .l()?;
            if value.is_null() {
                Ok(None)
            } else {
                let s: String = env.get_string(&JString::from(value))?.into();
                Ok(Some(s))
            }
        })
        .ok()
        .flatten()
    }

    // Hand the downloaded zip bytes to Kotlin, which unzips into sdmc/engage/mods.
    // Returns true on success.
    pub fn install_zip(bytes: &[u8]) -> anyhow::Result<bool> {
        with_activity(|env, activity| {
            let array = env.byte_array_from_slice(bytes)?;
            let ok = env
                .call_method(activity, "installZip", "([B)Z", &[JValue::Object(&array)])?
                .z()?;
            Ok(ok)
        })
    }

    // Result of the most recent folder pick: 0 = none yet, 1 = granted, 2 = wrong folder.
    pub fn pick_outcome() -> i32 {
        with_activity(|env, activity| {
            let outcome = env
                .call_method(activity, "pickOutcome", "()I", &[])?
                .i()?;
            Ok(outcome)
        })
        .unwrap_or(0)
    }

    // Delete a stray subsdk9 from a previous bad install, if there is one.
    pub fn delete_bad_subsdk9() -> anyhow::Result<bool> {
        with_activity(|env, activity| {
            let deleted = env
                .call_method(activity, "deleteBadSubsdk9", "()Z", &[])?
                .z()?;
            Ok(deleted)
        })
    }
}

#[cfg(feature = "desktop")]
pub(crate) fn open_dir(path: impl AsRef<Path>) -> std::io::Result<Child> {
    let cmd = match std::env::consts::OS {
        "macos" => "open",
        "windows" => "explorer",
        "linux" => "xdg-open",
        other => todo!("Unsupported platform: {other}"),
    };
    Command::new(cmd).arg(path.as_ref()).spawn()
}


#[cfg(feature = "desktop")]
fn construct_bad_subsdk9_path(emulator: &Emulator) -> Option<PathBuf> {
    emulator.data_path().map(|base| {
        base.join("mods/contents/0100a6301214e000/skyline/exefs/subsdk9")
    })
}

#[cfg(feature = "desktop")]
async fn delete_bad_subsdk9(emulator: &Emulator) {
    if let Some(path) = construct_bad_subsdk9_path(emulator) {
        if path.exists() {
            tracing::info!("Deleting bad subsdk9");
            std::fs::remove_file(path).unwrap();
        } else {
            tracing::info!("No bad subsdk9 found");
        }
    } else {
        tracing::error!("Could not find {} folder", emulator.name);
    }
}

async fn download_release() -> reqwest::Response {
    reqwest::get(RELEASE_URL)
        .await
        .unwrap()
}

#[cfg(feature = "desktop")]
async fn extract_release(zip_archive_bytes: &[u8], dest: PathBuf) {
    let reader = std::io::Cursor::new(zip_archive_bytes);
    let mut archive = ZipArchive::new(reader).unwrap();

    let files: Vec<String> = archive.file_names().map(String::from).collect();
    for name in files {
        let mut file = archive.by_name(&name).unwrap();
        let outpath = dest.join(file.name());

        if file.is_dir() {
            tracing::info!("File {} extracted to \"{}\"", name, outpath.display());
            std::fs::create_dir_all(&outpath).unwrap();
        } else {
            println!(
                "File {} extracted to \"{}\" ({} bytes)",
                name,
                outpath.display(),
                file.size()
            );
            if let Some(p) = outpath.parent() {
                if !p.exists() {
                    std::fs::create_dir_all(&p).unwrap();
                }
            }
            let mut outfile = std::fs::File::create(&outpath).unwrap();
            let mut buffer = Vec::new();
            file.read_to_end(&mut buffer).unwrap();
            outfile.write_all(&buffer).unwrap();
        }
    }
}

#[cfg(feature = "desktop")]
async fn create_mods_directory(sdcard_path: PathBuf) {
    let mods_path = sdcard_path.join("engage/mods");
    if !mods_path.exists() {
        std::fs::create_dir_all(mods_path).unwrap();
    } else {
        tracing::info!("Mods directory already exists");
    }
}

#[component]
fn App() -> Element {
    #[cfg(feature = "desktop")]
    {
        let window = use_window();
        window.set_always_on_top(false);
    }
    rsx! {
        document::Link { rel: "icon", href: FAVICON }
        document::Link { rel: "stylesheet", href: MAIN_CSS }
        Hero {}

    }
}

#[cfg(feature = "desktop")]
fn open_engage_mods_folder(path: impl AsRef<Path>) {
    let mods_path = path.as_ref().join("engage").join("mods");
    open_dir(mods_path)
        .expect("Failed to open mods folder");
}

#[cfg(feature = "desktop")]
fn does_engage_mods_folder_exist(path: impl AsRef<Path>) -> bool {
    let mods_path = path.as_ref().join("engage").join("mods");
    mods_path.exists()
}


#[component]
pub fn Hero() -> Element {
    // Shared shell: the welcome header, the status line, and the easter egg. The
    // platform specific controls live in `Controls`, which has a desktop and an
    // Android version below.
    let mut status_message = use_signal(|| "Waiting for you".to_string());
    let mut num_clicks = use_signal(|| 0);

    use_effect(move || {
        if num_clicks() == 5 {
            status_message.set("50 bond fragments obtained.".to_string());
        }
    });

    rsx! {
        div { id: "hero",
            header { id: "app_header",
                img {
                    id: "sammie",
                    src: SAMMIE,
                    alt: "Sammie stares at you, judgingly",
                    onclick: move |_| {
                        num_clicks.set(num_clicks() + 1);
                    },
                }
                div { class: "app_header_text",
                    h1 { "Cobalt Installer" }
                    p { class: "app_tagline", "Mods for Fire Emblem Engage" }
                }
                a { class: "header_help", href: "https://discord.gg/BH6XhKsKdS", "Need help?" }
            }
            div { id: "main-container",
                Body { status_message }
                footer { id: "credits",
                    span { "Sommie icon by badatgames26" }
                    span { class: "sep", "·" }
                    span { "v{env!(\"CARGO_PKG_VERSION\")}" }
                    span { class: "sep", "·" }
                    a { href: "https://discord.gg/BH6XhKsKdS", "Get help" }
                }
            }
        }
    }
}

// Desktop body: two tabs, the Cobalt installer and the mod browser. The installer stays reachable
// so the user can update Cobalt anytime, and the Mods tab stays locked until Cobalt is installed.
#[cfg(feature = "desktop")]
#[derive(Clone, Copy, PartialEq)]
enum Tab {
    Install,
    Browse,
    MyMods,
}

#[cfg(feature = "desktop")]
#[component]
fn Body(status_message: Signal<String>) -> Element {
    // The install target lives here (shared by both tabs) so the Cobalt tab's choice drives which
    // folder the mod browser installs into.
    let installation_type = use_storage::<LocalStorage, String>("installation_type".into(), || "Ryujinx".to_string());
    let user_selected_sdcard_path = use_storage::<LocalStorage, String>("sd_card_path".into(), || "".to_string());
    let nexus_apikey = use_storage::<LocalStorage, String>("nexus_apikey".into(), String::new);
    // First run walks the user through picking their device and confirming Cobalt is there. Once
    // done we remember it and go straight to the main app on later launches.
    let onboarded = use_storage::<LocalStorage, bool>("onboarded".into(), || false);
    let mut active_tab = use_signal(|| Tab::Install);
    // A banner for nxm:// downloads triggered from outside the app (the website's Mod Manager button).
    let mut nxm_status = use_signal(|| None::<String>);
    // Set when the user clicks "View" on a mod in My Mods: the Browse tab picks it up, switches to
    // the right source, and opens that mod's detail overlay in-app.
    let mut view_request = use_signal(|| None::<install::ModSource>);

    let sd_root = resolve_sd_root(&installation_type(), &user_selected_sdcard_path());
    let cobalt_ready = sd_root.as_ref().map(|p| is_cobalt_installed(p)).unwrap_or(false);

    // Don't strand the user on a locked tab if they switch to a target without Cobalt.
    use_effect(move || {
        if active_tab() != Tab::Install && !cobalt_ready {
            active_tab.set(Tab::Install);
        }
    });

    // The OS delivers an nxm:// link (from NexusMods' "Mod Manager Download") to the running app as
    // an Opened event. Parse it and install with the stored key, showing progress in the banner.
    use_wry_event_handler(move |event, _| {
        let Event::Opened { urls } = event else {
            return;
        };
        for url in urls {
            if url.scheme() != "nxm" {
                continue;
            }
            let link = url.as_str().to_string();
            let apikey = nexus_apikey().trim().to_string();
            let sd = resolve_sd_root(&installation_type(), &user_selected_sdcard_path());
            if apikey.is_empty() {
                nxm_status.set(Some("Connect your NexusMods account first (Mods tab → NexusMods).".to_string()));
                continue;
            }
            let Some(sd) = sd else {
                nxm_status.set(Some("Choose an install target on the Cobalt tab first.".to_string()));
                continue;
            };
            nxm_status.set(Some("Starting NexusMods download…".to_string()));
            let mut st = nxm_status;
            spawn(async move {
                match nexus_ui::run_nxm(&link, apikey, sd, move |s| st.set(Some(s))).await {
                    Ok(name) => st.set(Some(format!("Installed {name}!"))),
                    Err(e) => st.set(Some(format!("Error: {e}"))),
                }
            });
        }
    });

    rsx! {
        if let Some(msg) = nxm_status() {
            div { class: "nxm_banner",
                span { "{msg}" }
                button { class: "close", onclick: move |_| nxm_status.set(None), "X" }
            }
        }
        if !onboarded() {
            Onboarding { status_message, installation_type, user_selected_sdcard_path, onboarded }
        } else {
        div { class: "app_shell",
            nav { class: "sidebar",
                button {
                    class: if active_tab() == Tab::Install { "nav_item active" } else { "nav_item" },
                    onclick: move |_| active_tab.set(Tab::Install),
                    span { class: "nav_icon", {icons::download(18)} }
                    span { class: "nav_label", "Install Cobalt" }
                }
                button {
                    class: if active_tab() == Tab::Browse { "nav_item active" } else { "nav_item" },
                    disabled: !cobalt_ready,
                    title: if cobalt_ready { "" } else { "Install Cobalt first to browse mods" },
                    onclick: move |_| active_tab.set(Tab::Browse),
                    span { class: "nav_icon", {icons::search(18)} }
                    span { class: "nav_label", "Browse Mods" }
                    if !cobalt_ready {
                        span { class: "nav_lock", {icons::lock(13)} }
                    }
                }
                button {
                    class: if active_tab() == Tab::MyMods { "nav_item active" } else { "nav_item" },
                    disabled: !cobalt_ready,
                    title: if cobalt_ready { "" } else { "Install Cobalt first to manage mods" },
                    onclick: move |_| active_tab.set(Tab::MyMods),
                    span { class: "nav_icon", {icons::package(18)} }
                    span { class: "nav_label", "My Mods" }
                    if !cobalt_ready {
                        span { class: "nav_lock", {icons::lock(13)} }
                    }
                }
            }
            div { class: "tab_content",
                match active_tab() {
                    Tab::Install => rsx! {
                        Controls { status_message, installation_type, user_selected_sdcard_path }
                    },
                    Tab::Browse => rsx! {
                        if let Some(root) = sd_root.clone() {
                            mods_ui::ModBrowser { sd_root: root, view_request }
                        } else {
                            div { class: "mod_message", "Pick an install target on the Install tab first." }
                        }
                    },
                    Tab::MyMods => rsx! {
                        if let Some(root) = sd_root.clone() {
                            installed_ui::MyMods {
                                sd_root: root,
                                on_view: move |src| {
                                    view_request.set(Some(src));
                                    active_tab.set(Tab::Browse);
                                },
                            }
                        } else {
                            div { class: "mod_message", "Pick an install target on the Install tab first." }
                        }
                    },
                }
            }
        }
        }
    }
}

// First-run onboarding (desktop): pick the device you play on, then we check whether Cobalt is
// already installed there. If it is, Continue opens the main app. If not, install it right here
// first. The device choice is the same one the rest of the app uses, so picking it here also sets
// the target the mod browser installs into.
#[cfg(feature = "desktop")]
#[component]
fn Onboarding(
    mut status_message: Signal<String>,
    mut installation_type: Signal<String>,
    user_selected_sdcard_path: Signal<String>,
    mut onboarded: Signal<bool>,
) -> Element {
    let sd_root = resolve_sd_root(&installation_type(), &user_selected_sdcard_path());
    let cobalt_ready = sd_root.as_ref().map(|p| is_cobalt_installed(p)).unwrap_or(false);

    // Is the chosen device a usable target yet? (emulator actually found on disk, or an SD folder picked)
    let target_ready = if installation_type() == "SD Card" {
        !user_selected_sdcard_path().is_empty()
    } else if let Some(emulator) = get_emulator(&installation_type()) {
        emulator.is_installed()
    } else {
        false
    };

    let install_cobalt = move |_| async move {
        let Some(dest) = resolve_sd_root(&installation_type(), &user_selected_sdcard_path()) else {
            return;
        };
        if let Some(emulator) = get_emulator(&installation_type()) {
            delete_bad_subsdk9(emulator).await;
        }
        status_message.set("Downloading release".to_string());
        let response = download_release().await;
        let zip_archive_bytes = response.bytes().await.unwrap();
        extract_release(&zip_archive_bytes, dest.clone()).await;
        create_mods_directory(dest).await;
        // Setting status re-renders, and the Cobalt check above re-runs, unlocking Continue.
        status_message.set("Installation complete".to_string());
    };

    rsx! {
        div { id: "onboarding",
            div { class: "onboard_card",
                h2 { "Set up your device" }
                p { class: "onboard_sub",
                    "Where do you play Fire Emblem Engage? We'll check whether Cobalt is already installed there."
                }

                section { class: "panel",
                    div { class: "field",
                        label { r#for: "onboard_device_select", class: "field_label", "Which device do you use?" }
                        select {
                            id: "onboard_device_select",
                            class: "field_input",
                            value: installation_type,
                            onchange: move |e| installation_type.set(e.value()),
                            for emu in EMULATORS {
                                option { label: "{emu.name}", value: "{emu.name}" }
                            }
                            option { label: "SD card", value: "SD Card" }
                        }
                        if installation_type() == "SD Card" {
                            SdCardSelector { selected_sdcard_path: user_selected_sdcard_path }
                        } else if get_emulator(&installation_type()).is_some() {
                            EmulatorMessageZone { emulator_name: installation_type() }
                        }
                    }

                    div { class: "onboard_result",
                    if cobalt_ready {
                        div { class: "onboard_status ok",
                            {icons::check(18)}
                            span { "Cobalt is installed on {installation_type()}. You're all set!" }
                        }
                        button {
                            class: "primary",
                            onclick: move |_| onboarded.set(true),
                            "Continue to mods"
                        }
                    } else if target_ready {
                        div { class: "onboard_status",
                            "Cobalt isn't installed on {installation_type()} yet. Install it here to continue."
                        }
                        div { class: "action_zone_buttons",
                            button { class: "primary", onclick: install_cobalt, "Install Cobalt" }
                        }
                        code { class: "status",
                            "Status: "
                            {status_message}
                        }
                    } else {
                        div { class: "onboard_status",
                            if installation_type() == "SD Card" {
                                "Select your SD card folder above to continue."
                            } else {
                                "We couldn't find this emulator. Pick your SD card folder instead, or choose another device."
                            }
                        }
                    }
                    }
                }
            }
        }
    }
}

// Android body: just the Eden installer, no mod browser yet.
#[cfg(target_os = "android")]
#[component]
fn Body(status_message: Signal<String>) -> Element {
    rsx! {
        Controls { status_message }
    }
}

// Desktop controls: pick an emulator (or a raw SD card folder), then download and
// unzip Cobalt straight onto the host filesystem.
#[cfg(feature = "desktop")]
#[component]
fn Controls(
    mut status_message: Signal<String>,
    mut installation_type: Signal<String>,
    user_selected_sdcard_path: Signal<String>,
) -> Element {
    let is_install_ready = {
        if installation_type() == "SD Card" {
            !user_selected_sdcard_path().is_empty()
        } else if let Some(emulator) = get_emulator(&installation_type()) {
            emulator.is_installed()
        } else {
            false
        }
    };

    let mut cobalt_mod_path = use_signal(|| PathBuf::new());

    use_effect(move || {
        let sdcard_path = if installation_type() == "SD Card" {
            PathBuf::from(user_selected_sdcard_path())
        } else if let Some(emulator) = get_emulator(&installation_type()) {
            emulator.sd_card_path().expect("Could not find emulator folder")
        } else {
            return;
        };

        cobalt_mod_path.set(sdcard_path);
    });

    let install_cobalt = move |_| async move {
        tracing::info!("Extracting release to {:?}", cobalt_mod_path);

        if let Some(emulator) = get_emulator(&installation_type()) {
            delete_bad_subsdk9(emulator).await;
        }
        tracing::info!("Downloading release");
        status_message.set("Downloading release".to_string());
        let response = download_release().await;
        let zip_archive_bytes = response.bytes().await.unwrap();

        tracing::info!("Extracting release to {:?}", cobalt_mod_path);
        extract_release(&zip_archive_bytes, cobalt_mod_path()).await;
        create_mods_directory(cobalt_mod_path()).await;
        tracing::info!("Installation complete");
        status_message.set("Installation complete".to_string());
    };

    rsx! {
        section { class: "panel",
            div { class: "panel_head",
                h2 { class: "panel_title", "Install Cobalt" }
                p { class: "panel_hint",
                    "Downloads the latest Cobalt and sets it up for your device. Run it again anytime to update."
                }
            }

            div { class: "field",
                label { r#for: "installation_type_select", class: "field_label", "Device" }
                select {
                    id: "installation_type_select",
                    class: "field_input",
                    value: installation_type,
                    onchange: move |e| installation_type.set(e.value()),
                    for emu in EMULATORS {
                        option { label: "{emu.name}", value: "{emu.name}" }
                    }
                    option { label: "SD card", value: "SD Card" }
                }
                if installation_type() == "SD Card" {
                    SdCardSelector { selected_sdcard_path: user_selected_sdcard_path }
                } else if get_emulator(&installation_type()).is_some() {
                    EmulatorMessageZone { emulator_name: installation_type() }
                }
            }

            div { class: "actions_row",
                button {
                    id: "install_button",
                    class: "primary",
                    onclick: install_cobalt,
                    disabled: !is_install_ready,
                    "Install Cobalt"
                }
                button {
                    id: "open_mods_folder_button",
                    class: "secondary",
                    disabled: !does_engage_mods_folder_exist(cobalt_mod_path()),
                    onclick: move |_| {
                        open_engage_mods_folder(cobalt_mod_path());
                    },
                    "Open mods folder"
                }
            }
            if status_message() != "Waiting for you" {
                p { class: "status_line", {status_message} }
            }
        }
    }
}

// Android controls: Eden only. The user grants Eden's folder through the system
// picker (once, it sticks), then we download Cobalt and hand the bytes to Kotlin
// to write through the Storage Access Framework.
#[cfg(target_os = "android")]
#[component]
fn Controls(mut status_message: Signal<String>) -> Element {
    // Seed from any grant the user gave on a previous run.
    let mut tree_uri = use_signal(|| saf::persisted_tree_uri());

    let is_install_ready = tree_uri().is_some();

    let grant_access = move |_| {
        if let Err(e) = saf::request_tree_access() {
            status_message.set(format!("Couldn't open the folder picker: {e}"));
            return;
        }
        // The picker runs in the system UI on its own, so we can't await it. Poll for the
        // outcome of THIS pick (not the persisted grant, which could be a stale one from a
        // previous pick) so the most recent choice always wins.
        spawn(async move {
            for _ in 0..600 {
                futures_timer::Delay::new(std::time::Duration::from_millis(300)).await;
                match saf::pick_outcome() {
                    1 => {
                        tree_uri.set(saf::persisted_tree_uri());
                        break;
                    }
                    2 => {
                        tree_uri.set(None);
                        status_message.set("That's not Eden's folder. Tap the button again and pick Eden's folder.".to_string());
                        break;
                    }
                    _ => {}
                }
            }
        });
    };

    let install_cobalt = move |_| async move {
        status_message.set("Downloading release".to_string());
        let response = download_release().await;
        let zip_archive_bytes = match response.bytes().await {
            Ok(bytes) => bytes,
            Err(e) => {
                status_message.set(format!("Download failed: {e}"));
                return;
            }
        };

        // Clean up a stray subsdk9 from an old bad install before writing the new one.
        let _ = saf::delete_bad_subsdk9();

        status_message.set("Installing into Eden".to_string());
        match saf::install_zip(&zip_archive_bytes) {
            Ok(true) => status_message.set("Installation complete".to_string()),
            Ok(false) => status_message.set("Install failed: couldn't write into Eden's folder".to_string()),
            Err(e) => status_message.set(format!("Install failed: {e}")),
        }
    };

    rsx! {
        div {
            id: "installation_type_container",
            class: "message_zone first",
            div { "This installs Cobalt into the Eden emulator." }
        }
        div { class: "message_zone second",
            if tree_uri().is_some() {
                div { "Eden folder access granted." }
            } else {
                div { "First, grant access to Eden's folder." }
                div { "In the file picker, open the menu (top-left), choose Eden, then tap \"Use this folder\"." }
            }
            button {
                class: "secondary",
                onclick: grant_access,
                "Grant Eden folder access"
            }
        }
        div {
            id: "action_zone",
            class: {if is_install_ready { "message_zone third" } else { "message_zone disabled" }},
            div { class: "action_zone_buttons",
                button {
                    id: "install_button",
                    class: "primary",
                    onclick: install_cobalt,
                    disabled: !is_install_ready,
                    "Install Cobalt"
                }
            }
            code { class: "status",
                "Status: "
                {status_message}
            }
        }
    }
}

#[cfg(feature = "desktop")]
#[component]
pub fn EmulatorMessageZone(emulator_name: String) -> Element {
    let Some(emulator) = get_emulator(&emulator_name) else {
        return rsx! {};
    };

    rsx! {
        if emulator.is_installed() {
            p { class: "field_hint",
                "Detected at "
                code { {emulator.data_path().unwrap().display().to_string()} }
            }
        } else {
            p { class: "field_hint warn",
                "We couldn't find {emulator.name} on this computer. Choose SD card instead, or pick a different device."
            }
        }
    }
}

#[cfg(feature = "desktop")]
#[component]
pub fn SdCardSelector(mut selected_sdcard_path: Signal<String>) -> Element {
    rsx! {
        div { id: "sd_select_button_container", class: "file_pick",
            label { id: "sd_select_label", class: "pick_btn", r#for: "sd_select", "Choose folder…" }
            input {
                id: "sd_select",
                r#type: "file",
                // Select a folder by setting the directory attribute
                directory: true,
                display: "none",
                onchange: move |evt| {
                    let files = evt.files();
                    if let Some(file) = files.first() {
                        let dir = file.name().to_string();
                        tracing::info!("You chose folder: {}", dir);
                        selected_sdcard_path.set(dir);
                    }
                },
            }
            if selected_sdcard_path().is_empty() {
                span { class: "pick_path muted", "No folder selected" }
            } else {
                code { class: "pick_path", {selected_sdcard_path} }
                button {
                    class: "close",
                    onclick: move |_| selected_sdcard_path.set(String::new()),
                    "X"
                }
            }
        }
    }
}
