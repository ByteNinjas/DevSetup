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

/// Candidate file names for `cmd` in one directory, in lookup order.
///
/// Windows runs commands through their extension, so only extension-qualified
/// names are tried there: VS Code ships both a non-runnable `code` shell script
/// and the real `code.cmd` shim, and resolution must pick the shim. A command
/// that already carries an extension is used as is.
fn candidate_names(cmd: &str, extensions: &[&str]) -> Vec<String> {
    if extensions.is_empty() || std::path::Path::new(cmd).extension().is_some() {
        return vec![cmd.to_string()];
    }
    extensions.iter().map(|ext| format!("{cmd}{ext}")).collect()
}

fn find_in_dirs(dirs: &[PathBuf], cmd: &str) -> Option<PathBuf> {
    let names = candidate_names(cmd, platform::executable_extensions());
    for dir in dirs {
        for name in &names {
            let candidate = dir.join(name);
            if platform::is_executable(&candidate) { return Some(candidate); }
        }
    }
    None
}

/// First executable called `cmd` in PATH or in the folders DevSetup installs tools into.
pub fn find_in_path(cmd: &str) -> Option<PathBuf> {
    find_in_dirs(&search_dirs(), cmd)
}

pub fn on_path(cmd: &str) -> bool { find_in_path(cmd).is_some() }

#[cfg(test)]
mod tests {
    use super::*;

    const WINDOWS_EXTS: &[&str] = &[".exe", ".cmd", ".bat", ".com"];

    #[test]
    fn windows_candidates_skip_the_extensionless_file() {
        let names = candidate_names("code", WINDOWS_EXTS);
        assert_eq!(names, ["code.exe", "code.cmd", "code.bat", "code.com"]);
        assert!(!names.iter().any(|n| n == "code"), "the extensionless Git-Bash script must never be picked");
    }

    #[test]
    fn unix_candidates_use_the_command_verbatim() {
        assert_eq!(candidate_names("code", &[]), ["code"]);
    }

    #[test]
    fn qualified_commands_are_not_extended_again() {
        assert_eq!(candidate_names("code.cmd", WINDOWS_EXTS), ["code.cmd"]);
        assert_eq!(candidate_names("code.exe", WINDOWS_EXTS), ["code.exe"]);
    }
}

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
