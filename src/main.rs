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
// Inter (SIL OFL): the UI face for buttons and app-chrome headings. Variable
// weight 400-800, latin subset. The @font-face lives in App so the url
// survives asset hashing.
const INTER: Asset = asset!("/assets/fonts/Inter.woff2");
// Somniel facility icons for the sidebar (same dump as the pointer): the
// forge, the market and the bedroom. Tinted via CSS mask like the pointer.
const ICON_INSTALL: Asset = asset!("/assets/icon_forge.png");
const ICON_BROWSE: Asset = asset!("/assets/icon_browse.png");
const ICON_MYMODS: Asset = asset!("/assets/icon_mymods.png");
// Pixel-art banana (GameBanana's mascot, white stripped to alpha) shown
// before the "Open on GameBanana" link in the mod detail overlay.
const ICON_GAMEBANANA: Asset = asset!("/assets/icon_gamebanana.png");
// Bond fragment crystal, trailing the Sammie-click easter egg message.
const ICON_BONDS: Asset = asset!("/assets/icon_bonds.png");
// Somniel time-of-day icons for the theme toggle: Day (light), Night (dark),
// Evening (auto — the in-between).
const ICON_DAY: Asset = asset!("/assets/icon_day2.png");
const ICON_NIGHT: Asset = asset!("/assets/icon_night2.png");
const ICON_EVENING: Asset = asset!("/assets/icon_evening2.png");
// Alear's map sprites, standing on the hero Install button like the site's
// クラン button character. The install views cycle the costumes per mount.
const SPRITE_LUEUR: Asset = asset!("/assets/sprite_lueur.png");
const SPRITE_LUEUR_F: Asset = asset!("/assets/sprite_lueur_f.png");
const SPRITE_LUEUR_PROMO: Asset = asset!("/assets/sprite_lueur_promo.png");
const SPRITE_LUEUR_F_PROMO: Asset = asset!("/assets/sprite_lueur_f_promo.png");
const SPRITE_FELL: Asset = asset!("/assets/sprite_fell.png");
const SPRITE_FELL_F: Asset = asset!("/assets/sprite_fell_f.png");

// The Discord mark (filled). Lives here rather than the desktop-only icons
// module because the footer link is shared with the Android build.
fn discord_icon(size: u32) -> Element {
    rsx! {
        svg {
            class: "icon",
            width: "{size}",
            height: "{size}",
            view_box: "0 0 24 24",
            fill: "currentColor",
            stroke: "none",
            path { d: "M20.317 4.3698a19.7913 19.7913 0 00-4.8851-1.5152.0741.0741 0 00-.0785.0371c-.211.3753-.4447.8648-.6083 1.2495-1.8447-.2762-3.68-.2762-5.4868 0-.1636-.3933-.4058-.8742-.6177-1.2495a.077.077 0 00-.0785-.037 19.7363 19.7363 0 00-4.8852 1.515.0699.0699 0 00-.0321.0277C.5334 9.0458-.319 13.5799.0992 18.0578a.0824.0824 0 00.0312.0561c2.0528 1.5076 4.0413 2.4228 5.9929 3.0294a.0777.0777 0 00.0842-.0276c.4616-.6304.8731-1.2952 1.226-1.9942a.076.076 0 00-.0416-.1057c-.6528-.2476-1.2743-.5495-1.8722-.8923a.077.077 0 01-.0076-.1277c.1258-.0943.2517-.1923.3718-.2914a.0743.0743 0 01.0776-.0105c3.9278 1.7933 8.18 1.7933 12.0614 0a.0739.0739 0 01.0785.0095c.1202.099.246.1981.3728.2924a.077.077 0 01-.0066.1276 12.2986 12.2986 0 01-1.873.8914.0766.0766 0 00-.0407.1067c.3604.698.7719 1.3628 1.225 1.9932a.076.076 0 00.0842.0286c1.961-.6067 3.9495-1.5219 6.0023-3.0294a.077.077 0 00.0313-.0552c.5004-5.177-.8382-9.6739-3.5485-13.6604a.061.061 0 00-.0312-.0286zM8.02 15.3312c-1.1825 0-2.1569-1.0857-2.1569-2.419 0-1.3332.9555-2.4189 2.157-2.4189 1.2108 0 2.1757 1.0952 2.1568 2.419 0 1.3332-.9555 2.4189-2.1569 2.4189zm7.9748 0c-1.1825 0-2.1569-1.0857-2.1569-2.419 0-1.3332.9554-2.4189 2.1569-2.4189 1.2108 0 2.1757 1.0952 2.1568 2.419 0 1.3332-.946 2.4189-2.1568 2.4189Z" }
        }
    }
}

// Alear for a mounting install view, fed to CSS as (light, dark) sprite
// variables — dark theme shows the matching-gender Fell form. Cycles base M →
// base F → Divine Dragon M → Divine Dragon F per mount, no randomness. On
// desktop the slot persists with the other prefs, so the cycle continues
// across launches; Android falls back to a per-session counter.
fn use_hero_sprite() -> (Asset, Asset) {
    #[cfg(feature = "desktop")]
    let slot = {
        let mut stored = use_storage::<LocalStorage, usize>("hero_sprite_slot".into(), || 0);
        use_hook(move || {
            let s = stored() % 4;
            stored.set((s + 1) % 4);
            s
        })
    };
    #[cfg(not(feature = "desktop"))]
    let slot = use_hook(|| {
        use std::sync::atomic::{AtomicUsize, Ordering};
        static SLOT: AtomicUsize = AtomicUsize::new(0);
        SLOT.fetch_add(1, Ordering::Relaxed) % 4
    });
    match slot {
        0 => (SPRITE_LUEUR, SPRITE_FELL),
        1 => (SPRITE_LUEUR_F, SPRITE_FELL_F),
        2 => (SPRITE_LUEUR_PROMO, SPRITE_FELL),
        _ => (SPRITE_LUEUR_F_PROMO, SPRITE_FELL_F),
    }
}

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
mod ftp;
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
#[cfg(feature = "desktop")]
mod updater;

// Everything about locating an emulator on the host filesystem is desktop only.
// On Android we don't hunt for install folders, the user hands us Eden's folder
// through the system picker instead (see the `saf` module below).
#[cfg(feature = "desktop")]
struct Emulator {
    name: &'static str,
    // Linux can install an emulator a few different ways (native config dir, Flatpak sandbox), so
    // each gets a list of candidate data dirs. We use the first that exists.
    linux_data_paths: &'static [&'static str],
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
            "linux" => {
                let home = home_dir()?;
                let candidates: Vec<PathBuf> = self.linux_data_paths.iter().map(|p| home.join(p)).collect();
                // Prefer a layout that's actually there, else fall back to the first as the target.
                candidates
                    .iter()
                    .find(|p| p.exists())
                    .cloned()
                    .or_else(|| candidates.into_iter().next())
            }
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
        // Native config dir, then the Flatpak sandbox path (both from divine-builder's autodetect).
        linux_data_paths: &[".config/Ryujinx", ".var/app/org.ryujinx.Ryujinx/config/Ryujinx"],
        macos_data_path: "Library/Application Support/Ryujinx",
        windows_data_folder: "Ryujinx",
        sd_card_folder: "sdcard",
    },
    Emulator {
        name: "Citron",
        linux_data_paths: &[".local/share/citron"], // from the docs https://citron-emu.org/docs/installation
        macos_data_path: ".local/share/citron",
        windows_data_folder: "citron",
        sd_card_folder: "sdmc",
    },
    Emulator {
        name: "Eden",
        linux_data_paths: &[".local/share/eden"], // Assuming based on how Eden has the same structure as Citron, it's not mentioned in the docs.
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

// Removable drives only (SD card / USB), where a real console's SD shows up. Used by the SD card
// picker so the user can pick a plugged-in card instead of hunting for it with Browse. We rely on
// the OS "removable" flag so internal/system drives never show up in the list.
#[cfg(feature = "desktop")]
fn mounted_volumes() -> Vec<PathBuf> {
    let disks = sysinfo::Disks::new_with_refreshed_list();
    let mut vols: Vec<PathBuf> = disks
        .iter()
        .filter(|d| d.is_removable())
        .map(|d| d.mount_point().to_path_buf())
        .collect();
    vols.sort();
    vols.dedup();
    vols
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

        // reqwest (ring) and release-hub's stack (aws-lc-rs) both bring a rustls crypto provider, so
        // rustls can't auto-pick one and panics on first TLS use. Install a process default up front.
        let _ = rustls::crypto::aws_lc_rs::default_provider().install_default();

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

// This installer's own repo, for the self-update checks (desktop uses release-hub; Android hits the
// releases API directly since release-hub is desktop-only). Only read from the Android check.
#[allow(dead_code)]
const INSTALLER_REPO: &str = "DivineDragonFanClub/cobalt-installer";

// Compare two dotted version strings, returning true if `latest` is strictly newer than `current`.
// Numeric per-component compare, missing components count as 0, and any `-prerelease` suffix is
// ignored (only the numeric version is compared, pre-release tags aren't distinguished). Our release
// tags are plain vX.Y.Z, so that's fine. Shared, platform-agnostic. Only called from the Android
// check today (and the tests), so it looks unused in a plain desktop build.
#[allow(dead_code)]
fn version_gt(latest: &str, current: &str) -> bool {
    fn parts(v: &str) -> Vec<u64> {
        v.split('-')
            .next()
            .unwrap_or(v)
            .split('.')
            .map(|p| p.trim().parse::<u64>().unwrap_or(0))
            .collect()
    }
    let (a, b) = (parts(latest), parts(current));
    for i in 0..a.len().max(b.len()) {
        let (x, y) = (a.get(i).copied().unwrap_or(0), b.get(i).copied().unwrap_or(0));
        if x != y {
            return x > y;
        }
    }
    false
}

// Android has no release-hub, so we check GitHub's releases API for a newer APK ourselves. Returns
// (tag, release page url) when the latest published release is newer than the running build. Android
// itself enforces update integrity (an update APK must be signed with the same key), so there's no
// signature check here, the user just gets sent to the release to download it.
#[cfg(target_os = "android")]
async fn check_android_update() -> Option<(String, String)> {
    #[derive(serde::Deserialize)]
    struct Release {
        tag_name: String,
        html_url: String,
    }
    let client = reqwest::Client::builder()
        .user_agent(concat!("CobaltInstaller/", env!("CARGO_PKG_VERSION")))
        .build()
        .ok()?;
    let release: Release = client
        .get(format!("https://api.github.com/repos/{INSTALLER_REPO}/releases/latest"))
        .send()
        .await
        .ok()?
        .error_for_status()
        .ok()?
        .json()
        .await
        .ok()?;
    let latest = release.tag_name.trim_start_matches('v');
    version_gt(latest, env!("CARGO_PKG_VERSION")).then(|| (release.tag_name, release.html_url))
}

// On Android the target lives under Android/data, which is off limits to plain
// file access. All the writing happens on the Kotlin side (see android/MainActivity.kt),
// this module just calls those methods over JNI. The method names and signatures
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

    // Open a URL in the system browser (an ACTION_VIEW intent on the Kotlin side). Used by the
    // update banner to send the user to the GitHub release to download the new APK.
    pub fn open_url(url: &str) -> anyhow::Result<()> {
        with_activity(|env, activity| {
            let jurl = env.new_string(url)?;
            env.call_method(
                activity,
                "openUrl",
                "(Ljava/lang/String;)V",
                &[JValue::Object(&jurl)],
            )?;
            Ok(())
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

// Build an FTP config from the stored host/port. sys-ftpd is anonymous:5000, so we start from that
// default and just override the port if the user set a different one.
#[cfg(feature = "desktop")]
fn ftp_config(host: &str, port: &str) -> ftp::FtpConfig {
    let mut config = ftp::FtpConfig::sys_ftpd(host.trim());
    if let Ok(p) = port.trim().parse() {
        config.port = p;
    }
    config
}

// Connect to the console just to confirm it's reachable, reporting the result in the status line.
// The blocking connect runs on its own thread so the UI doesn't freeze while it waits.
#[cfg(feature = "desktop")]
async fn test_ftp(config: ftp::FtpConfig, mut status: Signal<String>) {
    status.set("Testing connection…".to_string());
    let (tx, rx) = futures_channel::oneshot::channel();
    std::thread::spawn(move || {
        let _ = tx.send(ftp::test_connection(&config));
    });
    match rx.await {
        Ok(Ok(())) => status.set("Connected to the console.".to_string()),
        Ok(Err(e)) => status.set(format!("Couldn't connect: {e}")),
        Err(_) => status.set("The connection test was interrupted.".to_string()),
    }
}

// Install/update Cobalt onto a console over FTP. Download the release, extract it into a temp
// staging folder (reusing the same extract path as a local install), then upload that whole tree to
// the SD root over FTP. The blocking upload runs on a background thread; we await its result.
#[cfg(feature = "desktop")]
async fn install_cobalt_ftp(config: ftp::FtpConfig, mut status: Signal<String>) {
    status.set("Downloading release".to_string());
    let bytes = match download_release().await.bytes().await {
        Ok(b) => b,
        Err(e) => {
            status.set(format!("Download failed: {e}"));
            return;
        }
    };

    status.set("Preparing files".to_string());
    let staging = std::env::temp_dir().join(format!("cobalt_ftp_stage_{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&staging);
    extract_release(&bytes, staging.clone()).await;
    create_mods_directory(staging.clone()).await;

    status.set("Uploading to the console…".to_string());
    let (tx, rx) = futures_channel::oneshot::channel();
    let staging_for_thread = staging.clone();
    std::thread::spawn(move || {
        // Upload the staged tree to the SD root ("/"), then clean up the temp folder.
        let res = ftp::upload_tree(&config, &staging_for_thread, "/", |_, _| {});
        let _ = std::fs::remove_dir_all(&staging_for_thread);
        let _ = tx.send(res);
    });

    match rx.await {
        Ok(Ok(())) => status.set("Installed to the console!".to_string()),
        Ok(Err(e)) => status.set(format!("FTP upload failed: {e}")),
        Err(_) => status.set("The upload was interrupted.".to_string()),
    }
}

#[component]
fn App() -> Element {
    #[cfg(feature = "desktop")]
    {
        let window = use_window();
        window.set_always_on_top(false);
    }
    // Dev-only inspector bridge: external tools eval JS in the running app
    // over HTTP. Never compiled into release builds.
    #[cfg(all(feature = "desktop", debug_assertions))]
    use_hook(|| {
        let mut eval_rx = dioxus_inspector::start_bridge(9223, "CobaltInstaller");
        spawn(async move {
            while let Some(cmd) = eval_rx.recv().await {
                let result = document::eval(&cmd.script).await;
                let response = match result {
                    Ok(val) => dioxus_inspector::EvalResponse::success(val.to_string()),
                    Err(e) => dioxus_inspector::EvalResponse::error(e.to_string()),
                };
                let _ = cmd.response_tx.send(response);
            }
        });
    });
    rsx! {
        document::Link { rel: "icon", href: FAVICON }
        document::Link { rel: "stylesheet", href: MAIN_CSS }
        document::Style {
            {
                format!(
                    "@font-face {{ font-family: \"Inter\"; src: url(\"{INTER}\") format(\"woff2\"); font-weight: 400 800; font-style: normal; font-display: swap; }}\n\
                         .ico_install {{ background-image: url(\"{ICON_INSTALL}\"); }}\n\
                         .ico_browse {{ background-image: url(\"{ICON_BROWSE}\"); }}\n\
                         .ico_mymods {{ background-image: url(\"{ICON_MYMODS}\"); }}\n\
                 .tt_day {{ background-image: url(\"{ICON_DAY}\"); }}\n\
                 .tt_night {{ background-image: url(\"{ICON_NIGHT}\"); }}\n\
                 .tt_evening {{ background-image: url(\"{ICON_EVENING}\"); }}\n\
                         .gb_link::before {{ content: \"\"; display: inline-block; width: 15px; height: 15px; margin-right: 6px; vertical-align: -2px; background: url(\"{ICON_GAMEBANANA}\") no-repeat center / contain; image-rendering: pixelated; }}\n\
                         .bonds::after {{ content: \"\"; display: inline-block; width: 14px; height: 16px; margin-left: 6px; vertical-align: -3px; background: url(\"{ICON_BONDS}\") no-repeat center / contain; }}",
                )
            }
        }
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


// Footer theme switcher, cycling auto → light → dark. "auto" follows the OS;
// the other two force a scheme by setting data-theme on <html>, which flips the
// light-dark() tokens in main.css. The choice is remembered across launches.
#[cfg(feature = "desktop")]
#[component]
fn ThemeToggle() -> Element {
    let mut theme = use_storage::<LocalStorage, String>("theme".into(), || "auto".to_string());

    // Apply on startup and whenever the user cycles the toggle.
    use_effect(move || {
        let js = match theme().as_str() {
            t @ ("light" | "dark") => format!("document.documentElement.dataset.theme = '{t}';"),
            _ => "delete document.documentElement.dataset.theme;".to_string(),
        };
        document::eval(&js);
    });

    let (icon_class, label, next) = match theme().as_str() {
        "light" => ("tt_icon tt_day", "Light", "dark"),
        "dark" => ("tt_icon tt_night", "Dark", "auto"),
        _ => ("tt_icon tt_evening", "Auto", "light"),
    };

    rsx! {
        button {
            class: "theme_toggle",
            title: "Theme: auto follows your system. Click to switch.",
            onclick: move |_| theme.set(next.to_string()),
            span { class: icon_class }
            span { "{label}" }
        }
    }
}

// The Android build keeps the OS scheme; no toggle yet.
#[cfg(not(feature = "desktop"))]
#[component]
fn ThemeToggle() -> Element {
    rsx! {}
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
            // Desktop moved the brand into the sidebar (see Body); the header
            // row only remains on Android.
            if cfg!(not(feature = "desktop")) {
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
                    a {
                        class: "header_help",
                        href: "https://discord.gg/BH6XhKsKdS",
                        "Need help?"
                    }
                }
            }
            div { id: "main-container",
                Body { status_message }
                footer { id: "credits",
                    span { "Sommie icon by badatgames26" }
                    span { class: "sep", "·" }
                    span { "v{env!(\"CARGO_PKG_VERSION\")}" }
                    span { class: "sep", "·" }
                    a { class: "footer_discord", href: "https://discord.gg/BH6XhKsKdS",
                        {discord_icon(13)}
                        span { "Join the Discord" }
                    }
                    span { class: "sep", "·" }
                    ThemeToggle {}
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

    // The Sammie-click easter egg followed the logo into the sidebar. Every
    // fifth click pops the (dismissable) bond fragments modal.
    let mut num_clicks = use_signal(|| 0);
    let mut show_bonds = use_signal(|| false);
    use_effect(move || {
        if num_clicks() > 0 && num_clicks() % 5 == 0 {
            show_bonds.set(true);
        }
    });
    // Set when the user clicks "View" on a mod in My Mods: the Browse tab picks it up, switches to
    // the right source, and opens that mod's detail overlay in-app.
    let mut view_request = use_signal(|| None::<install::ModSource>);

    // Self-update: check our own GitHub releases once on launch. A failed check just leaves the
    // banner hidden (we never nag), so this quietly does nothing until releases are signed.
    let mut update_available = use_signal(|| None::<release_hub::Update>);
    let mut update_status = use_signal(|| None::<String>);
    use_future(move || async move {
        if let Ok(Some(update)) = updater::check().await {
            update_available.set(Some(update));
        }
    });

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
        if let Some(update) = update_available() {
            div { class: "nxm_banner update_banner",
                span {
                    if let Some(msg) = update_status() {
                        "{msg}"
                    } else {
                        "Cobalt Installer {update.version} is available."
                    }
                }
                if update_status().is_none() {
                    button {
                        class: "primary",
                        onclick: {
                            let up = update.clone();
                            move |_| {
                                update_status.set(Some("Downloading and installing…".to_string()));
                                let up = up.clone();
                                spawn(async move {
                                    if let Err(e) = updater::install_and_relaunch(up, |_| {}).await {
                                        update_status.set(Some(format!("Update failed: {e}")));
                                    }
                                });
                            }
                        },
                        "Update & restart"
                    }
                    button { class: "close", onclick: move |_| update_available.set(None), "X" }
                }
            }
        }
        if let Some(msg) = nxm_status() {
            div { class: "nxm_banner",
                span { "{msg}" }
                button { class: "close", onclick: move |_| nxm_status.set(None), "X" }
            }
        }
        if !onboarded() {
            Onboarding {
                status_message,
                installation_type,
                user_selected_sdcard_path,
                onboarded,
            }
        } else {
            if show_bonds() {
                div {
                    class: "bonds_overlay",
                    onclick: move |_| show_bonds.set(false),
                    div {
                        class: "bonds_modal",
                        onclick: move |e| e.stop_propagation(),
                        img { src: ICON_BONDS, alt: "Bond fragments" }
                        p { class: "bonds_text", "50 bond fragments obtained." }
                        button {
                            class: "ghost",
                            onclick: move |_| show_bonds.set(false),
                            "OK"
                        }
                    }
                }
            }
            div { class: "app_shell",
                nav { class: "sidebar",
                    // Brand bar: not a nav item — only Sammie reacts (the easter egg).
                    div { class: "sidebar_brand",
                        img {
                            id: "sammie",
                            src: SAMMIE,
                            alt: "Sammie stares at you, judgingly",
                            onclick: move |_| {
                                num_clicks.set(num_clicks() + 1);
                            },
                        }
                        div { class: "sidebar_brand_text",
                            span { class: "sidebar_title", "Cobalt Installer" }
                            span { class: "sidebar_tagline", "Mods for Fire Emblem Engage" }
                        }
                    }
                    button {
                        class: if active_tab() == Tab::Install { "nav_item active" } else { "nav_item" },
                        onclick: move |_| active_tab.set(Tab::Install),
                        span { class: "nav_icon game ico_install" }
                        span { class: "nav_label", "Install Cobalt" }
                    }
                    button {
                        class: if active_tab() == Tab::Browse { "nav_item active" } else { "nav_item" },
                        disabled: !cobalt_ready,
                        title: if cobalt_ready { "" } else { "Install Cobalt first to browse mods" },
                        onclick: move |_| active_tab.set(Tab::Browse),
                        span { class: "nav_icon game ico_browse" }
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
                        span { class: "nav_icon game ico_mymods" }
                        span { class: "nav_label", "My Mods" }
                        if !cobalt_ready {
                            span { class: "nav_lock", {icons::lock(13)} }
                        }
                    }
                }
                div { class: "tab_content",
                    match active_tab() {
                        Tab::Install => rsx! {
                            Controls {
                                status_message,
                                installation_type,
                                user_selected_sdcard_path,
                            }
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
    let (sprite_light, sprite_dark) = use_hero_sprite();
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
        div {
            id: "onboarding",
            style: "--hero-sprite-light: url('{sprite_light}'); --hero-sprite-dark: url('{sprite_dark}')",
            div { class: "onboard_card",
                h2 { "Set up your device" }
                p { class: "onboard_sub",
                    "Where do you play Fire Emblem Engage? We'll check whether Cobalt is already installed there."
                }

                section { class: "panel",
                    div { class: "field",
                        label {
                            r#for: "onboard_device_select",
                            class: "field_label",
                            "Which device do you use?"
                        }
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
                                class: "engage",
                                onclick: move |_| onboarded.set(true),
                                "Continue to mods"
                            }
                        } else if target_ready {
                            div { class: "onboard_status",
                                "Cobalt isn't installed on {installation_type()} yet. Install it here to continue."
                            }
                            div { class: "action_zone_buttons",
                                button { class: "engage", onclick: install_cobalt, "Install Cobalt" }
                            }
                            code { class: if status_message().contains("bond fragments") { "status bonds" } else { "status" },
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

// Android body: the Eden installer, plus a "new version" banner. Android can't self-install (no
// release-hub, and we don't publish on Play), so the banner just links out to the GitHub release.
#[cfg(target_os = "android")]
#[component]
fn Body(status_message: Signal<String>) -> Element {
    let mut update = use_signal(|| None::<(String, String)>);
    use_future(move || async move {
        if let Some(info) = check_android_update().await {
            update.set(Some(info));
        }
    });

    rsx! {
        if let Some((tag, url)) = update() {
            div { class: "nxm_banner update_banner",
                span { "Cobalt Installer {tag} is available." }
                button {
                    class: "primary",
                    onclick: move |_| {
                        let _ = saf::open_url(&url);
                    },
                    "Get it on GitHub"
                }
                button { class: "close", onclick: move |_| update.set(None), "X" }
            }
        }
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
    let (sprite_light, sprite_dark) = use_hero_sprite();
    // FTP console target: host + port persist across launches, defaulting to sys-ftpd's port.
    let mut ftp_host = use_storage::<LocalStorage, String>("ftp_host".into(), String::new);
    let mut ftp_port = use_storage::<LocalStorage, String>("ftp_port".into(), || "5000".to_string());

    let is_ftp = installation_type() == "FTP";

    let is_install_ready = {
        if is_ftp {
            !ftp_host().trim().is_empty()
        } else if installation_type() == "SD Card" {
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
        // FTP delivers over the network to a console instead of a local folder.
        if installation_type() == "FTP" {
            install_cobalt_ftp(ftp_config(&ftp_host(), &ftp_port()), status_message).await;
            return;
        }

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
        section {
            class: "panel",
            style: "--hero-sprite-light: url('{sprite_light}'); --hero-sprite-dark: url('{sprite_dark}')",
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
                    option { label: "Console over FTP", value: "FTP" }
                }
                if is_ftp {
                    div { class: "ftp_fields",
                        div { class: "ftp_row",
                            input {
                                class: "field_input",
                                r#type: "text",
                                placeholder: "Console IP (e.g. 192.168.1.42)",
                                value: "{ftp_host}",
                                oninput: move |e| ftp_host.set(e.value()),
                            }
                            input {
                                class: "field_input port",
                                r#type: "text",
                                placeholder: "Port",
                                value: "{ftp_port}",
                                oninput: move |e| ftp_port.set(e.value()),
                            }
                            button {
                                class: "secondary",
                                disabled: ftp_host().trim().is_empty(),
                                onclick: move |_| async move {
                                    test_ftp(ftp_config(&ftp_host(), &ftp_port()), status_message).await;
                                },
                                "Test connection"
                            }
                        }
                        p { class: "field_hint",
                            "Your console needs an FTP server running (sys-ftpd: anonymous, port 5000). Install writes straight to its SD card over the network."
                        }
                    }
                } else if installation_type() == "SD Card" {
                    SdCardSelector { selected_sdcard_path: user_selected_sdcard_path }
                } else if get_emulator(&installation_type()).is_some() {
                    EmulatorMessageZone { emulator_name: installation_type() }
                }
            }

            div { class: "actions_row",
                button {
                    id: "install_button",
                    class: "engage",
                    onclick: install_cobalt,
                    disabled: !is_install_ready,
                    "Install Cobalt"
                }
                if !is_ftp {
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
            }
            if status_message() != "Waiting for you" {
                p { class: if status_message().contains("bond fragments") { "status_line bonds" } else { "status_line" },
                    {status_message}
                }
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
    let (sprite_light, sprite_dark) = use_hero_sprite();
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
        div { id: "installation_type_container", class: "message_zone first",
            div { "This installs Cobalt into the Eden emulator." }
        }
        div { class: "message_zone second",
            if tree_uri().is_some() {
                div { "Eden folder access granted." }
            } else {
                div { "First, grant access to Eden's folder." }
                div {
                    "In the file picker, open the menu (top-left), choose Eden, then tap \"Use this folder\"."
                }
            }
            button { class: "secondary", onclick: grant_access, "Grant Eden folder access" }
        }
        div {
            id: "action_zone",
            style: "--hero-sprite-light: url('{sprite_light}'); --hero-sprite-dark: url('{sprite_dark}')",
            class: {if is_install_ready { "message_zone third" } else { "message_zone disabled" }},
            div { class: "action_zone_buttons",
                button {
                    id: "install_button",
                    class: "engage",
                    onclick: install_cobalt,
                    disabled: !is_install_ready,
                    "Install Cobalt"
                }
            }
            code { class: if status_message().contains("bond fragments") { "status bonds" } else { "status" },
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
    let volumes = mounted_volumes();
    rsx! {
        div { id: "sd_select_button_container", class: "file_pick",
            label {
                id: "sd_select_label",
                class: "pick_btn",
                r#for: "sd_select",
                "Choose folder…"
            }
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
            // Quick-pick a plugged-in SD/USB volume instead of browsing for it.
            if !volumes.is_empty() {
                select {
                    class: "field_input volume_select",
                    value: "",
                    onchange: move |e| {
                        let v = e.value();
                        if !v.is_empty() {
                            selected_sdcard_path.set(v);
                        }
                    },
                    option { value: "", "Pick a volume…" }
                    for vol in volumes {
                        option { value: "{vol.display()}",
                            {
                                vol.file_name()
                                    .map(|n| n.to_string_lossy().to_string())
                                    .unwrap_or_else(|| vol.display().to_string())
                            }
                        }
                    }
                }
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

#[cfg(test)]
mod version_tests {
    use super::version_gt;

    #[test]
    fn newer_versions_win() {
        assert!(version_gt("0.1.3", "0.1.2"));
        assert!(version_gt("0.2.0", "0.1.9"));
        assert!(version_gt("1.0.0", "0.9.9"));
        // Equal or older is not "newer".
        assert!(!version_gt("0.1.2", "0.1.2"));
        assert!(!version_gt("0.1.1", "0.1.2"));
        // Missing components count as 0.
        assert!(version_gt("0.2", "0.1.9"));
        // A pre-release suffix is ignored, so it compares by the numeric version only.
        assert!(!version_gt("0.1.2", "0.1.2-rc1"));
        assert!(version_gt("0.1.3", "0.1.2-rc1"));
    }
}
