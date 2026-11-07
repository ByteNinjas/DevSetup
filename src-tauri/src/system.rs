use crate::manifest::{Method, Tool};
use crate::platform;
use serde::Serialize;
use std::collections::HashSet;
use std::path::PathBuf;

#[derive(Serialize)]
pub struct Distro { pub id: String, pub name: String, pub version: String }

pub fn distro() -> Distro {
    #[cfg(target_os = "linux")]
    {
        let txt = std::fs::read_to_string("/etc/os-release").unwrap_or_default();
        let get = |k: &str| {
            let prefix = format!("{k}=");
            txt.lines().find_map(|l| l.strip_prefix(prefix.as_str()).map(|v| v.trim_matches('"').to_string())).unwrap_or_default()
        };
        Distro { id: get("ID"), name: get("PRETTY_NAME"), version: get("VERSION_ID") }
    }
    #[cfg(not(target_os = "linux"))]
    {
        Distro {
            id: std::env::consts::OS.to_string(),
            name: platform::name().to_string(),
            version: platform::version(),
        }
    }
}

pub fn home() -> Option<PathBuf> {
    #[cfg(target_os = "windows")]
    {
        std::env::var_os("USERPROFILE").map(PathBuf::from).or_else(|| {
            let drive = std::env::var_os("HOMEDRIVE")?;
            let path = std::env::var_os("HOMEPATH")?;
            Some(PathBuf::from(drive).join(path))
        })
    }
    #[cfg(not(target_os = "windows"))]
    {
        std::env::var_os("HOME").map(PathBuf::from)
    }
}

pub fn data_dir() -> Option<PathBuf> { platform::data_dir() }

pub fn config_dir() -> Option<PathBuf> { platform::config_dir() }

fn search_dirs() -> Vec<PathBuf> {
    let mut dirs: Vec<PathBuf> = std::env::var_os("PATH").map(|p| std::env::split_paths(&p).collect()).unwrap_or_default();
    if let Some(h) = home() {
        for d in [".local/bin", ".local/share/mise/shims", ".cargo/bin", ".bun/bin"] { dirs.push(h.join(d)); }
    }
    dirs
}

/// First executable called `cmd` in PATH or in the folders DevSetup installs tools into.
pub fn find_in_path(cmd: &str) -> Option<PathBuf> {
    let extensions = platform::executable_extensions();
    for dir in search_dirs() {
        let direct = dir.join(cmd);
        if platform::is_executable(&direct) { return Some(direct); }
        for ext in extensions {
            let candidate = dir.join(format!("{cmd}{ext}"));
            if platform::is_executable(&candidate) { return Some(candidate); }
        }
    }
    None
}

pub fn on_path(cmd: &str) -> bool { find_in_path(cmd).is_some() }

/// Names of installed Debian/Ubuntu packages, read once from the dpkg status file.
pub fn apt_installed() -> HashSet<String> {
    #[cfg(target_os = "linux")]
    {
        let txt = std::fs::read_to_string("/var/lib/dpkg/status").unwrap_or_default();
        let mut set = HashSet::new();
        for block in txt.split("\n\n") {
            let (mut name, mut ok) = (None, false);
            for l in block.lines() {
                if let Some(v) = l.strip_prefix("Package: ") { name = Some(v.trim().to_string()); }
                else if l == "Status: install ok installed" { ok = true; }
            }
            if let (Some(n), true) = (name, ok) { set.insert(n); }
        }
        set
    }
    #[cfg(not(target_os = "linux"))]
    { HashSet::new() }
}

pub(crate) fn flatpak_has(app_id: &str) -> bool {
    #[cfg(target_os = "linux")]
    {
        PathBuf::from("/var/lib/flatpak/app").join(app_id).exists()
            || home().map(|h| h.join(".local/share/flatpak/app").join(app_id).exists()).unwrap_or(false)
    }
    #[cfg(not(target_os = "linux"))]
    {
        let _ = app_id;
        false
    }
}

pub fn is_installed(t: &Tool, apt: &HashSet<String>) -> bool {
    let by_method = match &t.method {
        Method::Apt { package } => apt.contains(package),
        Method::Flatpak { app_id } => flatpak_has(app_id),
        _ => false,
    };
    by_method || (!t.check.is_empty() && on_path(&t.check))
}
