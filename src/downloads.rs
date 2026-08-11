// Background install manager (desktop only).
//
// Installs kicked off from the Browse detail modal used to run inside that modal's own component, so
// closing the modal cancelled the download. Instead the modal just hands a request to a coroutine
// that lives in `Body` (always mounted), which does the work there. Progress lives in a shared list
// (provided via context) that the modal, the My Mods view, and the sidebar all read, so a download
// keeps going and stays visible no matter what's on screen.
//
// The install is transactional: a mod and its required Engage mods are all downloaded first, then
// installed together, and if any piece fails the ones we just added are removed again. So the user
// never ends up with a half-installed mod or its dependencies without the mod itself.

use std::collections::HashSet;
use std::path::{Path, PathBuf};

use dioxus::prelude::*;
use futures_util::StreamExt;

use crate::gamebanana::{self, ModDetail, ModFile};
use crate::install;

// Where a given install is in its lifecycle. Finished installs are removed from the list (the My
// Mods scan then shows the real mod), so there's no Done here; only in-flight and failed states.
#[derive(Clone, PartialEq)]
pub enum Phase {
    // Pulling a file down. `what` names it when it's a dependency, empty for the main mod.
    Downloading { what: String, done: u64, total: u64 },
    // A step with no byte progress (checking requirements, unzipping). `what` is the label to show.
    Working { what: String },
    Error(String),
}

// One in-progress (or failed) install, shown as a row in My Mods.
#[derive(Clone, PartialEq)]
pub struct ActiveDownload {
    pub id: u64,
    pub name: String,
    pub thumb: Option<String>,
    pub phase: Phase,
}

// What the modal sends to the install coroutine.
pub struct InstallRequest {
    pub detail: ModDetail,
    pub file: ModFile,
    pub sd_root: PathBuf,
}

// The shared list, handed around by context.
pub type Downloads = Signal<Vec<ActiveDownload>>;

fn set_phase(mut list: Downloads, id: u64, phase: Phase) {
    if let Some(e) = list.write().iter_mut().find(|e| e.id == id) {
        e.phase = phase;
    }
}

fn remove(mut list: Downloads, id: u64) {
    list.write().retain(|e| e.id != id);
}

// Is there at least one install actually running right now (for the sidebar spinner)?
pub fn is_busy(list: &[ActiveDownload]) -> bool {
    list.iter().any(|d| !matches!(d.phase, Phase::Error(_)))
}

// Install a mod and all its FE Engage GameBanana requirements as one unit. Everything is downloaded
// first (network is the flaky part), then installed together; if any install fails, the ones this
// call added are removed again so it's all-or-nothing. Progress is reported through on_phase. On
// failure returns Err with a message (and has already rolled back).
pub async fn install_transactional(
    sd_root: &Path,
    detail: &ModDetail,
    file: &ModFile,
    mut on_phase: impl FnMut(Phase),
) -> Result<(), String> {
    // What's already there, so we skip re-installing it and never roll a pre-existing mod back.
    let already: HashSet<u64> = install::installed_gamebanana_ids(sd_root).into_keys().collect();

    // 1. Work out the required Engage mods (details + files). Downloads nothing yet.
    on_phase(Phase::Working { what: "Checking requirements…".into() });
    let mut plan: Vec<(ModDetail, ModFile)> = vec![(detail.clone(), file.clone())];
    plan.extend(gamebanana::resolve_engage_requirements(detail, &already).await);

    // 2. Download everything up front. A failure here means nothing has touched the disk yet.
    let mut blobs: Vec<(ModDetail, Vec<u8>)> = Vec::with_capacity(plan.len());
    for (i, (d, f)) in plan.iter().enumerate() {
        let what = if i == 0 { String::new() } else { format!("required mod: {}", d.name) };
        let mut last_pct = u64::MAX;
        let result = gamebanana::download(&f.download_url, |done, total| {
            let pct = if total > 0 { done * 100 / total } else { 0 };
            if pct != last_pct {
                last_pct = pct;
                on_phase(Phase::Downloading { what: what.clone(), done, total });
            }
        })
        .await;
        match result {
            Ok(bytes) => blobs.push((d.clone(), bytes)),
            Err(e) => return Err(format!("Download failed: {e}")),
        }
    }

    // 3. Install everything. If one fails, undo the ones we already added (leaving pre-existing mods).
    on_phase(Phase::Working { what: "Installing…".into() });
    let mut added: Vec<u64> = Vec::new();
    for (d, bytes) in &blobs {
        match install::install_gamebanana_mod(sd_root, d, bytes) {
            Ok(()) => added.push(d.id),
            Err(e) => {
                for id in &added {
                    if !already.contains(id) {
                        install::uninstall_gamebanana_mod(sd_root, *id);
                    }
                }
                return Err(format!("Install failed: {e}"));
            }
        }
    }
    Ok(())
}

// The coroutine body: process install requests one at a time, updating the shared list as each goes.
pub async fn run_installer(list: Downloads, mut rx: UnboundedReceiver<InstallRequest>) {
    while let Some(req) = rx.next().await {
        process(list, req).await;
    }
}

async fn process(mut list: Downloads, req: InstallRequest) {
    let InstallRequest { detail, file, sd_root } = req;
    let id = detail.id;

    // Replace any earlier attempt for this mod, then start fresh.
    remove(list, id);
    list.write().push(ActiveDownload {
        id,
        name: detail.name.clone(),
        thumb: detail.preview.images.first().map(|i| i.thumb_url()),
        phase: Phase::Working { what: "Preparing…".into() },
    });

    match install_transactional(&sd_root, &detail, &file, |phase| set_phase(list, id, phase)).await {
        // Done: drop the entry so the My Mods scan shows the finished mod instead.
        Ok(()) => remove(list, id),
        Err(e) => set_phase(list, id, Phase::Error(e)),
    }
}
