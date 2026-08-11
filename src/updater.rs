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
const REPO_NAME: &str = "cobalt-installer";

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
