#![windows_subsystem = "windows"]

// Path handling is desktop only. On Android we never touch host file paths, the
// writing goes through the folder the user grants (see the `saf` module).
#[cfg(feature = "desktop")]
use std::path::{Path, PathBuf};

#[cfg(feature = "desktop")]
use dioxus::desktop::use_window;
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

fn main() {
    // Desktop and Android launch differently. Desktop wires up a data directory
    // and the local-storage backend, Android just hands the app to the mobile
    // renderer (no `dirs` paths, they come back None there).
    #[cfg(feature = "desktop")]
    {
        dioxus_sdk::storage::set_dir!();
        LaunchBuilder::new()
            .with_cfg(
                dioxus_desktop::Config::new().with_data_directory(dirs::data_local_dir().unwrap().join("CobaltInstaller"))
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
fn open_dir(path: impl AsRef<Path>) -> std::io::Result<Child> {
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
            div {
                div { id: "welcome",
                    h1 { "Welcome to the Cobalt Installer" }
                    img {
                        id: "sammie",
                        src: SAMMIE,
                        alt: "Sammie stares at you, judgingly",
                        onclick: move |_| {
                            num_clicks.set(num_clicks() + 1);
                        },
                    }
                }
            }
            div { id: "main-container",
                Controls { status_message }
                div { id: "credits",
                    p {
                        "Having issues? "
                        a { href: "https://discord.gg/BH6XhKsKdS", "Get help!" }
                    }
                    p { "Sommie icon by badatgames26" }
                    p { "Version {env!(\"CARGO_PKG_VERSION\")}" }
                }
            }
        }
    }
}

// Desktop controls: pick an emulator (or a raw SD card folder), then download and
// unzip Cobalt straight onto the host filesystem.
#[cfg(feature = "desktop")]
#[component]
fn Controls(mut status_message: Signal<String>) -> Element {
    let mut installation_type = use_storage::<LocalStorage, String>("installation_type".into(), || { "Ryujinx".to_string()});

    let user_selected_sdcard_path = use_storage::<LocalStorage, String>("sd_card_path".into(), || { "".to_string()});

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
        div {
            id: "installation_type_container",
            class: "message_zone first",
            label { r#for: "installation_type_select", "How would you like to install Cobalt?" }
            select {
                id: "installation_type_select",
                value: installation_type,
                onchange: move |e| {
                    installation_type.set(e.value());
                },
                for emu in EMULATORS {
                    option { label: "Install for {emu.name}", value: "{emu.name}" }
                }
                option { label: "Install onto SD card", value: "SD Card" }
            }
        }
        if installation_type() == "SD Card" {
            SdCardSelector { selected_sdcard_path: user_selected_sdcard_path }
        }
        if get_emulator(&installation_type()).is_some() {
            EmulatorMessageZone { emulator_name: installation_type() }
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
                button {
                    id: "open_mods_folder_button",
                    class: "secondary",
                    disabled: !does_engage_mods_folder_exist(cobalt_mod_path()),
                    onclick: move |_| {
                        open_engage_mods_folder(cobalt_mod_path());
                    },
                    "Open Cobalt Mods Folder"
                }
            }
            code { class: "status",
                "Status: "
                {status_message}
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
        div { class: "message_zone second",
            div {
                if emulator.is_installed() {
                    {emulator.name}
                    " autodetected at "
                    code { {emulator.data_path().unwrap().display().to_string()} }
                } else {
                    div { "We couldn't find your {emulator.name} installation." }
                    div { "Please use the SD Card installation type instead." }
                }
            }
        }
    }
}

#[cfg(feature = "desktop")]
#[component]
pub fn SdCardSelector(mut selected_sdcard_path: Signal<String>) -> Element {
    rsx! {
        div { id: "sd_select_container", class: "message_zone second",
            div { "Select your SD Card folder, and we'll install Cobalt there." }
            div { id: "sd_select_button_container",
                label { id: "sd_select_label", r#for: "sd_select", "Select SD Card folder" }
                input {
                    id: "sd_select",
                    r#type: "file",
                    // Select a folder by setting the directory attribute
                    directory: true,
                    onchange: move |evt| {
                        let files = evt.files();
                        if let Some(file) = files.iter().next() {
                            let dir = file.name().to_string();
                            tracing::info!("You chose folder: {}", dir);
                            selected_sdcard_path.set(dir);
                        }
                    },
                    display: "none",
                }
                div {
                    code {
                        if selected_sdcard_path().len() == 0 {
                            {"No folder selected"}
                        } else {
                            {selected_sdcard_path}
                        }
                    }
                }
                if selected_sdcard_path().len() > 0 {
                    button {
                        class: "close",
                        onclick: move |_| {
                            selected_sdcard_path.set("".to_string());
                        },
                        "X"
                    }
                }
            }
        }
    }
}
