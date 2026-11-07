//! Self-update for the running DevSetup build.
//!
//! The UI only asks for a check or an install. Everything else is fixed here: the
//! release endpoint and signing key come from the bundled config, a pending update
//! is kept as the raw signed manifest so the upgrade restarts from the same data,
//! and the downloaded artifact is verified again before anything is installed.
use crate::exec;
use serde::Serialize;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, Mutex, MutexGuard};
use std::time::{Duration, Instant};
use tauri::{AppHandle, Emitter, State};
use tauri_plugin_updater::{Update, UpdaterExt};

/// Event stream for the update card: checking, downloading, installing, restarting, failed.
pub const PROGRESS_EVENT: &str = "app-update-progress";
const CHECK_TIMEOUT: Duration = Duration::from_secs(20);
const DOWNLOAD_TIMEOUT: Duration = Duration::from_secs(600);
const PROGRESS_INTERVAL: Duration = Duration::from_millis(150);

/// State shared by the update commands.
#[derive(Default)]
pub struct AppUpdate {
    /// Raw release manifest of the update the last check found.
    pending: Mutex<Option<Pending>>,
    /// One check or install at a time.
    busy: AtomicBool,
}

#[derive(Clone)]
struct Pending {
    version: String,
    raw: serde_json::Value,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AppUpdateState {
    pub current: String,
    /// `appimage`, `deb`, `rpm`, `nsis`, `msi`, `app` or `development`.
    pub kind: String,
    /// False for dev builds and unpackaged binaries, where there is nothing to replace.
    pub can_update: bool,
}

#[derive(Serialize, Clone)]
pub struct AppUpdateInfo {
    pub version: String,
    pub current: String,
    pub date: Option<String>,
    pub notes: Option<String>,
}

#[derive(Serialize, Clone)]
#[serde(rename_all = "camelCase")]
struct AppUpdateProgress {
    phase: &'static str,
    downloaded: u64,
    total: Option<u64>,
    message: String,
}

struct Found {
    info: AppUpdateInfo,
    raw: serde_json::Value,
}

fn lock<T>(m: &Mutex<T>) -> MutexGuard<'_, T> {
    m.lock().unwrap_or_else(|e| e.into_inner())
}

/// What the running binary was installed as, as far as the updater can tell.
fn install_kind() -> &'static str {
    use tauri::utils::config::BundleType;
    match tauri::utils::platform::bundle_type() {
        Some(BundleType::AppImage) => "appimage",
        Some(BundleType::Deb) => "deb",
        Some(BundleType::Rpm) => "rpm",
        Some(BundleType::Nsis) => "nsis",
        Some(BundleType::Msi) => "msi",
        Some(BundleType::App) | Some(BundleType::Dmg) => "app",
        _ => "development",
    }
}

fn can_update() -> bool {
    !tauri::is_dev() && install_kind() != "development"
}

fn emit_progress(app: &AppHandle, phase: &'static str, downloaded: u64, total: Option<u64>, message: &str) {
    let _ = app.emit(PROGRESS_EVENT, AppUpdateProgress {
        phase,
        downloaded,
        total,
        message: message.to_string(),
    });
}

fn info(update: &Update) -> AppUpdateInfo {
    AppUpdateInfo {
        version: update.version.clone(),
        current: update.current_version.clone(),
        date: update.date.map(|d| d.to_string()),
        notes: update.body.clone(),
    }
}

/// Turns the updater's errors into something a user can act on.
fn friendly(err: tauri_plugin_updater::Error) -> String {
    use tauri_plugin_updater::Error as E;
    match err {
        E::EmptyEndpoints => "This build has no update endpoint configured. Install a newer release manually.".into(),
        E::ReleaseNotFound | E::Reqwest(_) | E::Network(_) => "Could not reach the release server. Check your connection and try again.".into(),
        E::TargetNotFound(_) | E::TargetsNotFound(_) | E::UnsupportedOs | E::UnsupportedArch => {
            "This build has no update package for your platform. Install the new release manually.".into()
        }
        E::Minisign(_) | E::SignatureUtf8(_) | E::Base64(_) | E::MissingSignedVersion | E::SignedVersionMismatch { .. } => {
            "The update package failed its security check and was not installed.".into()
        }
        E::AuthenticationFailed | E::PackageInstallFailed | E::DebInstallFailed => {
            "The update could not be installed. The password prompt may have been cancelled.".into()
        }
        E::TempDirNotOnSameMountPoint | E::TempDirNotFound | E::Io(_) => {
            "The update could not be written to disk. Free some space and try again.".into()
        }
        E::InvalidUpdaterFormat | E::BinaryNotFoundInArchive => {
            "The downloaded package is not a valid update for this build.".into()
        }
        E::InsecureTransportProtocol => "The update endpoint is not secure. Install the new release manually.".into(),
        other => format!("Update failed: {other}"),
    }
}

async fn check_for_update(app: &AppHandle) -> Result<Option<Found>, String> {
    let updater = app.updater_builder().timeout(CHECK_TIMEOUT).build().map_err(friendly)?;
    let update = updater.check().await.map_err(friendly)?;
    Ok(update.map(|u| Found { info: info(&u), raw: u.raw_json }))
}

async fn check_and_store(app: &AppHandle, store: &AppUpdate) -> Result<Option<AppUpdateInfo>, String> {
    emit_progress(app, "checking", 0, None, "");
    let found = check_for_update(app).await?;
    let info = found.as_ref().map(|f| f.info.clone());
    *lock(&store.pending) = found.map(|f| Pending { version: f.info.version, raw: f.raw });
    Ok(info)
}

async fn do_install(app: &AppHandle, store: &AppUpdate) -> Result<(), String> {
    // The UI normally installs what the last check found. If that result is gone
    // (page reload, check from another window), make one more check now.
    let pending = lock(&store.pending).clone();
    let pending = match pending {
        Some(p) => p,
        None => match check_for_update(app).await? {
            Some(found) => {
                let p = Pending { version: found.info.version, raw: found.raw };
                *lock(&store.pending) = Some(Pending { version: p.version.clone(), raw: p.raw.clone() });
                p
            }
            None => return Err("DevSetup is already up to date.".into()),
        },
    };

    let updater = app.updater().map_err(friendly)?;
    let mut update = updater
        .restore_update(pending.raw)
        .map_err(friendly)?
        .ok_or_else(|| "That update is no longer available. Check for updates again.".to_string())?;
    if update.version != pending.version {
        return Err("The pending update changed. Check for updates again.".into());
    }
    update.timeout = Some(DOWNLOAD_TIMEOUT);

    let downloaded = Arc::new(AtomicU64::new(0));
    let counter = downloaded.clone();
    let mut last_emit = Instant::now();
    let progress_app = app.clone();
    let on_chunk = move |chunk: usize, total: Option<u64>| {
        let done = counter.fetch_add(chunk as u64, Ordering::Relaxed) + chunk as u64;
        let complete = matches!(total, Some(t) if done >= t);
        if complete || last_emit.elapsed() >= PROGRESS_INTERVAL {
            last_emit = Instant::now();
            emit_progress(&progress_app, "downloading", done, total, "");
        }
    };
    let counter = downloaded.clone();
    let finish_app = app.clone();
    let on_finish = move || emit_progress(&finish_app, "downloading", counter.load(Ordering::Relaxed), None, "");

    let bytes = update.download(on_chunk, on_finish).await.map_err(friendly)?;

    emit_progress(app, "installing", downloaded.load(Ordering::Relaxed), None, "");
    let installer = update.clone();
    let installed = tauri::async_runtime::spawn_blocking(move || installer.install(bytes))
        .await
        .map_err(|e| format!("Update failed: {e}"))?;
    installed.map_err(friendly)?;

    emit_progress(app, "restarting", 0, None, "");
    // Give the webview a moment to paint the final state before the process goes away.
    std::thread::sleep(Duration::from_millis(350));
    let _: () = app.restart();
}

#[tauri::command]
pub fn app_update_state(app: AppHandle) -> AppUpdateState {
    AppUpdateState {
        current: app.package_info().version.to_string(),
        kind: install_kind().to_string(),
        can_update: can_update(),
    }
}

#[tauri::command]
pub async fn app_update_check(app: AppHandle, store: State<'_, AppUpdate>) -> Result<Option<AppUpdateInfo>, String> {
    if !can_update() {
        return Err("Updates are only available in packaged builds.".into());
    }
    if store.busy.swap(true, Ordering::SeqCst) {
        return Err("An update is already being checked or installed.".into());
    }
    let result = check_and_store(&app, &store).await;
    store.busy.store(false, Ordering::SeqCst);
    result
}

#[tauri::command]
pub async fn app_update_install(app: AppHandle, store: State<'_, AppUpdate>) -> Result<(), String> {
    if !can_update() {
        return Err("Updates are only available in packaged builds.".into());
    }
    if store.busy.swap(true, Ordering::SeqCst) {
        return Err("An update is already being checked or installed.".into());
    }
    // Do not let a setup/update run start while the binary is being replaced.
    if exec::RUNNING.swap(true, Ordering::SeqCst) {
        store.busy.store(false, Ordering::SeqCst);
        return Err("Another operation is running. Wait for it to finish and try again.".into());
    }

    match do_install(&app, &store).await {
        Ok(()) => {
            store.busy.store(false, Ordering::SeqCst);
            exec::RUNNING.store(false, Ordering::SeqCst);
            Ok(())
        }
        Err(e) => {
            emit_progress(&app, "failed", 0, None, &e);
            store.busy.store(false, Ordering::SeqCst);
            exec::RUNNING.store(false, Ordering::SeqCst);
            Err(e)
        }
    }
}
