#![windows_subsystem = "windows"]

use std::path::{Path, PathBuf};

use dioxus::desktop::use_window;
use dioxus::{logger::tracing, prelude::*};

const FAVICON: Asset = asset!("/assets/favicon.ico");
const MAIN_CSS: Asset = asset!("/assets/main.css");
const SAMMIE: Asset = asset!("/assets/SAMMIE.png");

use dirs::home_dir;

use std::process::{Child, Command};

use zip::ZipArchive;
use std::io::{Read, Write};
use dioxus_sdk::storage::*;

struct Emulator {
    name: &'static str,
    linux_data_path: &'static str,
    macos_data_path: &'static str,
    windows_data_folder: &'static str,
    sd_card_folder: &'static str,
}

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

fn get_emulator(name: &str) -> Option<&'static Emulator> {
    EMULATORS.iter().find(|e| e.name == name)
}

fn main() {
    dioxus_sdk::storage::set_dir!();
    LaunchBuilder::new()
        .with_cfg(
            dioxus_desktop::Config::new().with_data_directory(dirs::data_local_dir().unwrap().join("CobaltInstaller"))
        )
        .launch(App);
}

const RELEASE_URL: &str = "https://github.com/Raytwo/Cobalt/releases/latest/download/release.zip";

fn open_dir(path: impl AsRef<Path>) -> std::io::Result<Child> {
    let cmd = match std::env::consts::OS {
        "macos" => "open",
        "windows" => "explorer",
        "linux" => "xdg-open",
        other => todo!("Unsupported platform: {other}"),
    };
    Command::new(cmd).arg(path.as_ref()).spawn()
}

fn construct_bad_subsdk9_path(emulator: &Emulator) -> Option<PathBuf> {
    emulator.data_path().map(|base| {
        base.join("mods/contents/0100a6301214e000/skyline/exefs/subsdk9")
    })
}

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
    let window = use_window();
    window.set_always_on_top(false);
    rsx! {
        document::Link { rel: "icon", href: FAVICON }
        document::Link { rel: "stylesheet", href: MAIN_CSS }
        Hero {}

    }
}

fn open_engage_mods_folder(path: impl AsRef<Path>) {
    let mods_path = path.as_ref().join("engage").join("mods");
    open_dir(mods_path)
        .expect("Failed to open mods folder");
}

fn does_engage_mods_folder_exist(path: impl AsRef<Path>) -> bool {
    let mods_path = path.as_ref().join("engage").join("mods");
    mods_path.exists()
}


#[component]
pub fn Hero() -> Element {
    let mut status_message = use_signal(|| "Waiting for you".to_string());

    let mut installation_type = use_storage::<LocalStorage, String>("installation_type".into(), || { "Ryujinx".to_string()});

    let user_selected_sdcard_path = use_storage::<LocalStorage, String>("sd_card_path".into(), || { "".to_string()});

    let mut num_clicks = use_signal(|| 0);

    use_effect(move || {
        if num_clicks() == 5 {
            status_message.set("50 bond fragments obtained.".to_string());
        }
    });

    let is_install_ready = {
        if installation_type() == "SD Card" {
            !user_selected_sdcard_path().is_empty()
        } else if let Some(emulator) = get_emulator(&installation_type()) {
            emulator.is_installed()
        } else {
            false
        }
    };
    
    tracing::info!("Installation type: {:?}", installation_type);
    tracing::info!("User selected path: {:?}", user_selected_sdcard_path);

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
                        {status_message.clone()}
                    }
                }
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
