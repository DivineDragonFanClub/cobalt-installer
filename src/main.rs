#![windows_subsystem = "windows"]

use std::path::{Path, PathBuf};

use dioxus::desktop::use_window;
use dioxus::{logger::tracing, prelude::*};

const FAVICON: Asset = asset!("/assets/favicon.ico");
const MAIN_CSS: Asset = asset!("/assets/main.css");

use dirs::home_dir;

use std::process::{Child, Command};

use zip::ZipArchive;
use std::io::{Read, Write};
use dioxus_sdk::storage::*;

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
    if cfg!(target_os = "macos") {
        Command::new("open").arg(path.as_ref()).spawn()
    } else if cfg!(target_os = "windows") {
        Command::new("explorer").arg(path.as_ref()).spawn()
    } else {
        Command::new("xdg-open").arg(path.as_ref()).spawn()
    }
}

/// Returns the Ryujinx data folder in a platform-agnostic way:
/// - macOS: ~/Library/Application Support/Ryujinx
/// - Windows: %APPDATA%/Ryujinx ? unconfirmed
/// - Linux/Other: ~/.config/Ryujinx ? unconfirmed
fn ryujinx_data_path() -> Option<PathBuf> {
    if cfg!(target_os = "macos") {
        home_dir().map(|h| h.join("Library").join("Application Support").join("Ryujinx"))
    } else if cfg!(target_os = "windows") {
        std::env::var_os("APPDATA").map(|appdata| PathBuf::from(appdata).join("Ryujinx"))
    } else {
        home_dir().map(|h| h.join(".config").join("Ryujinx"))
    }
}

fn is_ryujinx_installed() -> bool {
    ryujinx_data_path().map(|path| path.exists()).unwrap_or(false)
}

/// Constructs the path to the `subsdk9` directory inside the mods/contents/... directory.
fn construct_bad_subsdk9_path() -> Option<PathBuf> {
    ryujinx_data_path().map(|base| {
        base.join("mods/contents/0100a6301214e000/skyline/exefs/subsdk9")
    })
}

async fn delete_bad_subsdk9() {
    if let Some(path) = construct_bad_subsdk9_path() {
        if path.exists() {
            tracing::info!("Deleting bad subsdk9");
            std::fs::remove_file(path).unwrap();
        } else {
            tracing::info!("No bad subsdk9 found");
        }
    } else {
        tracing::error!("Could not find Ryujinx folder");
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

fn get_ryujinx_sd_card_folder() -> Option<PathBuf> {
    ryujinx_data_path().map(|base| base.join("sdcard"))
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
        // if SD card, need a filled in SD card path
        // else, it's ready
        if installation_type() == "SD Card" {
            user_selected_sdcard_path().len() > 0
        } else {
            is_ryujinx_installed()
        }
    };
    
    tracing::info!("Installation type: {:?}", installation_type);
    tracing::info!("User selected path: {:?}", user_selected_sdcard_path);

    let mut cobalt_mod_path = use_signal(|| PathBuf::new());

    use_effect(move || {
        let sdcard_path = if installation_type() == String::from("SD Card") {
            PathBuf::from(user_selected_sdcard_path())
        } else if installation_type() == String::from("Ryujinx") {
            get_ryujinx_sd_card_folder().expect("Could not find Ryujinx folder")
        } else {
            panic!("Pick an installation method.");
        };

        cobalt_mod_path.set(sdcard_path);
    });

    // let sdcard_path = if installation_type() == String::from("SD Card") {
    //     PathBuf::from(user_selected_sdcard_path())
    // } else if installation_type() == String::from("Ryujinx") {
    //     get_sd_card_folder().expect("Could not find Ryujinx folder")
    // } else {
    //     panic!("Pick an installation method.");
    // };

    let install_cobalt = move |_| async move {
        tracing::info!("Extracting release to {:?}", cobalt_mod_path);

        delete_bad_subsdk9().await;
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
            id: "hero",
            div { 
                div {
                    id: "welcome",
                    h1 {
                        "Welcome to the Cobalt Installer"
                    }
                    img {
                        id: "sammie",
                        src: "/assets/SAMMIE.png",
                        alt: "Sammie stares at you, judgingly",
                        onclick: move |_| {
                            num_clicks.set(num_clicks() + 1);
                        }
                    }
                }
            }
            div {
                id: "main-container",
                div {
                    id: "installation_type_container",
                    class: "message_zone first",
                    label { 
                        for: "installation_type_select",
                        "How would you like to install Cobalt?",
                    },
                    select {  
                        id: "installation_type_select",
                        value: installation_type,
                        onchange: move |e| {
                            installation_type.set(e.value());
                        },
                        option { label: "Install for Ryujinx", value: "Ryujinx" }
                        option { label: "Install onto SD card", value: "SD Card" }
                    }  
                }
                if installation_type() ==  "SD Card" {
                    SdCardSelector {
                        selected_sdcard_path: user_selected_sdcard_path
                    }
                }
                if installation_type() == "Ryujinx" {
                   RyujinxMessageZone {  }
                }
                
                div {
                    id: "action_zone", 
                    class: {
                        if is_install_ready {
                            "message_zone third"
                        } else {
                            "message_zone disabled"
                        }
                    },
                    div {
                        class: "action_zone_buttons",
                        button { 
                            id: "install_button",
                            class: "primary",
                            onclick: install_cobalt, disabled: !is_install_ready, "Install Cobalt" }
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
                    code { 
                        class: "status",
                        "Status: "
                        { status_message.clone() }
                    }
                }
                div {
                    id: "credits",
                    p {
                        "Having issues? "
                        a {
                            href: "https://discord.gg/BH6XhKsKdS",
                            "Get help!"
                        }
                    }
                    p {
                        "Sommie icon by badatgames26"
                    }
                }
            }
        }
    }
}

#[component]
pub fn RyujinxMessageZone() -> Element {
    rsx! {
        div
        {
            class: "message_zone second",
            div {
                {
                    if is_ryujinx_installed() {
                        rsx! {
                            "Ryujinx autodetected at "
                            code {
                                { ryujinx_data_path().unwrap().display().to_string() }
                            }
                        }
                    } else {
                        rsx! {
                            div {
                                "We couldn't find your Ryujinx installation."
                            }
                            div { 
                                "Please use the SD Card installation type instead."
                            }
                        }
                    }
                }
            }
        }
    }
}

#[component]
pub fn SdCardSelector(mut selected_sdcard_path: Signal<String>) -> Element {
    rsx! {
        div {
            id: "sd_select_container",
            class: "message_zone second",
            div {
                "Select your SD Card folder, and we'll install Cobalt there."
            }
            div {
                id: "sd_select_button_container",           
                label { 
                    id: "sd_select_label",
                    for: "sd_select",
                    "Select SD Card folder"
                }
                input {
                    id: "sd_select",
                    r#type: "file",
                    // Select a folder by setting the directory attribute
                    directory: true,
                    onchange: move |evt| {
                        if let Some(file_engine) = evt.files() {
                            if let Some(dir) = file_engine.files().iter().next() {
                                tracing::info!("You chose folder: {}", dir);
                                selected_sdcard_path.set(dir.to_owned());
                            }
                            
                        }
                    },
                    display: "none",
                }
                div {
                    code {
                        if selected_sdcard_path().len() == 0 {
                             { "No folder selected" }
                        } else {
                             { selected_sdcard_path }
                        }
                    }
                }
                if selected_sdcard_path().len() > 0 {
                    button {
                        class: "close",
                        onclick: move |_| {
                            selected_sdcard_path.set("".to_string());
                        },
                        "X",
                    }
                }
            }
        }
    }
}
