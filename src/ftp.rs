// FTP delivery to a modded console (desktop only).
//
// A hacked Switch running sys-ftpd exposes its SD filesystem over plain FTP (anonymous, port 5000).
// That lets us install straight to the console over the LAN instead of writing to a local emulator
// folder or a physically-inserted SD card. This is a blocking client, callers run it on a background
// thread (see the delivery code in main.rs) so the UI stays responsive.

use std::collections::HashSet;
use std::path::{Path, PathBuf};

use suppaftp::types::FileType;
use suppaftp::FtpStream;

// A saved FTP connection. Defaults match sys-ftpd: anonymous login on port 5000.
#[derive(Clone, PartialEq, Debug)]
pub struct FtpConfig {
    pub host: String,
    pub port: u16,
    pub anonymous: bool,
    pub user: String,
    pub password: String,
}

impl FtpConfig {
    pub fn sys_ftpd(host: impl Into<String>) -> Self {
        FtpConfig {
            host: host.into(),
            port: 5000,
            anonymous: true,
            user: String::new(),
            password: String::new(),
        }
    }

    fn connect(&self) -> anyhow::Result<FtpStream> {
        let mut ftp = FtpStream::connect((self.host.as_str(), self.port))?;
        if self.anonymous {
            ftp.login("anonymous", "")?;
        } else {
            ftp.login(&self.user, &self.password)?;
        }
        Ok(ftp)
    }
}

// Connect and log in, then hang up. Used by the "Test connection" button so a bad host/port reports
// quickly instead of failing partway through an install.
pub fn test_connection(config: &FtpConfig) -> anyhow::Result<()> {
    let mut ftp = config.connect()?;
    let _ = ftp.quit();
    Ok(())
}

// Upload everything under `local_root` to `remote_base/<relative path>` on the device, making remote
// directories as needed. `remote_base` is an absolute remote path like "/" (the SD root, for the
// Cobalt release) or "/engage/mods/MyMod" (for a single mod). Reports (done, total) file counts.
pub fn upload_tree<F: FnMut(u64, u64)>(
    config: &FtpConfig,
    local_root: &Path,
    remote_base: &str,
    mut on_progress: F,
) -> anyhow::Result<()> {
    let files = collect_files(local_root);
    let total = files.len() as u64;

    let mut ftp = config.connect()?;
    // Binary mode, or the bundles get mangled by line-ending translation.
    ftp.transfer_type(FileType::Binary)?;

    let base = remote_base.trim_end_matches('/');
    let mut ensured: HashSet<String> = HashSet::new();
    ensure_dir(&mut ftp, base, &mut ensured);

    let mut done = 0u64;
    for file in &files {
        // Relative path under the staging root, in forward-slash form for the remote.
        let rel = file
            .strip_prefix(local_root)
            .unwrap_or(file)
            .to_string_lossy()
            .replace('\\', "/");
        let remote = format!("{base}/{rel}");

        if let Some(slash) = remote.rfind('/') {
            ensure_dir(&mut ftp, &remote[..slash], &mut ensured);
        }

        let mut f = std::fs::File::open(file)?;
        ftp.put_file(&remote, &mut f)?;
        done += 1;
        on_progress(done, total);
    }

    let _ = ftp.quit();
    Ok(())
}

// Make a remote directory and every parent above it, remembering what we've already made so we don't
// re-issue mkdir for shared parents. FTP mkdir errors on an existing directory, so we ignore errors.
fn ensure_dir(ftp: &mut FtpStream, remote_dir: &str, ensured: &mut HashSet<String>) {
    let mut cur = String::new();
    for part in remote_dir.split('/').filter(|s| !s.is_empty()) {
        cur.push('/');
        cur.push_str(part);
        if ensured.insert(cur.clone()) {
            let _ = ftp.mkdir(&cur);
        }
    }
}

// Every file under a folder, walked recursively (directories are created on the fly during upload).
fn collect_files(root: &Path) -> Vec<PathBuf> {
    fn walk(dir: &Path, out: &mut Vec<PathBuf>) {
        let Ok(entries) = std::fs::read_dir(dir) else {
            return;
        };
        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_dir() {
                walk(&path, out);
            } else {
                out.push(path);
            }
        }
    }
    let mut out = Vec::new();
    walk(root, &mut out);
    out
}
