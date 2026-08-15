// Self-update (desktop only): check our own GitHub releases for a newer installer, download the
// signed asset for this platform, install it, and relaunch. Built on release-hub's GitHubSource.
//
// For this to actually find updates, each release must attach, per platform, an installer asset
// whose name contains the Rust target triple (e.g. `aarch64-apple-darwin`, `x86_64-pc-windows-msvc`)
// with a supported extension, plus a sibling minisign signature (`<asset>.sig` or `.minisig`). The
// asset is verified against MINISIGN_PUBKEY below. Generate a keypair with `minisign -G`: paste the
// PUBLIC key line here, keep the secret key in CI. See release.yml.

use release_hub::{Config, GitHubSource, Update, Updater, UpdaterBuilder};

const REPO_OWNER: &str = "DivineDragonFanClub";
const REPO_NAME: &str = "cobalt-manager";

// The public half of the minisign keypair the release assets are signed with (the private key lives
// in CI as MINISIGN_SECRET_KEY). release-hub verifies each downloaded asset against this before
// install. This must be the FULL two-line minisign .pub content (comment line, then the base64 key):
// release-hub feeds it to minisign-verify's PublicKey::decode, which expects both lines. A bare key
// line fails with "Invalid encoding in minisign data".
const MINISIGN_PUBKEY: &str = "untrusted comment: minisign public key\nRWS4awesL6pkQfXQKSRf7bpCYQ239Vs3R9jo1t5A8RLMHMIALNWAnRnR";

fn build_updater() -> anyhow::Result<Updater> {
    let config = Config {
        pubkey: MINISIGN_PUBKEY.to_string(),
        ..Default::default()
    };
    let updater = UpdaterBuilder::new(env!("CARGO_PKG_NAME"), env!("CARGO_PKG_VERSION"), config)
        .source(Box::new(GitHubSource::new(REPO_OWNER, REPO_NAME)))
        .build()?;
    Ok(updater)
}

// A newer installer if one is published for this platform, else None. Returns an error on network
// or verification trouble, which the caller can quietly ignore (a failed check shouldn't nag).
pub async fn check() -> anyhow::Result<Option<Update>> {
    // No key configured yet means nothing is verifiable, so there's nothing to offer.
    if MINISIGN_PUBKEY.is_empty() {
        return Ok(None);
    }
    Ok(build_updater()?.check().await?)
}

// Download + install the update (reporting bytes as they arrive), then relaunch into the new build.
// On success the process is replaced/relaunched, so this usually doesn't return.
pub async fn install_and_relaunch(update: Update, on_bytes: impl FnMut(usize)) -> anyhow::Result<()> {
    update.download_and_install(on_bytes).await?;
    build_updater()?.relaunch()?;
    Ok(())
}

// One-time cleanup of the old, differently-named Windows install.
//
// We used to ship as "Cobalt Installer". The rebrand to "Cobalt Manager" changes the NSIS product
// name, so when an old build self-updates, the new installer lands in a fresh folder and leaves the
// old "Cobalt Installer" behind in Add/Remove Programs. The old build's updater code is frozen and
// can't clean up after itself, so the new build does it on launch instead: find the leftover entry
// and run its silent uninstaller. Once it's gone this finds nothing and does nothing, so it's safe
// to call on every startup. Best-effort, any failure is ignored (a stray old entry is harmless).
#[cfg(target_os = "windows")]
pub fn cleanup_previous_install() {
    use std::process::Command;
    use winreg::enums::{HKEY_CURRENT_USER, HKEY_LOCAL_MACHINE};
    use winreg::RegKey;

    const OLD_DISPLAY_NAME: &str = "Cobalt Installer";
    const UNINSTALL_PATH: &str = r"Software\Microsoft\Windows\CurrentVersion\Uninstall";

    // NSIS per-user installs register under HKCU, per-machine under HKLM. Check both.
    for root in [HKEY_CURRENT_USER, HKEY_LOCAL_MACHINE] {
        let Ok(uninstall) = RegKey::predef(root).open_subkey(UNINSTALL_PATH) else {
            continue;
        };
        for name in uninstall.enum_keys().flatten() {
            let Ok(entry) = uninstall.open_subkey(&name) else {
                continue;
            };
            let display: String = entry.get_value("DisplayName").unwrap_or_default();
            if display != OLD_DISPLAY_NAME {
                continue;
            }
            // Prefer the ready-made silent form, else take the plain uninstall path and add NSIS's
            // silent switch ourselves. Run it through cmd so the quoted path + args parse for us.
            let cmd: Option<String> = entry
                .get_value("QuietUninstallString")
                .ok()
                .or_else(|| {
                    entry
                        .get_value::<String, _>("UninstallString")
                        .ok()
                        .map(|s| format!("{s} /S"))
                });
            if let Some(cmd) = cmd {
                let _ = Command::new("cmd").args(["/C", &cmd]).spawn();
            }
        }
    }
}
