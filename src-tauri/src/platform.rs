//! Small platform helpers shared by the app.
//!
//! Linux is the full-featured target. Windows and macOS builds are previews:
//! the UI, setup files, Git and VS Code helpers work, while actions that depend
//! on apt, pkexec, Flatpak or Linux-only files stay disabled.

use crate::system;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

pub const LINUX: bool = cfg!(target_os = "linux");
pub const MACOS: bool = cfg!(target_os = "macos");
pub const WINDOWS: bool = cfg!(target_os = "windows");

pub fn name() -> &'static str {
    if LINUX { "Linux" } else if MACOS { "macOS" } else if WINDOWS { "Windows" } else { std::env::consts::OS }
}

#[cfg_attr(target_os = "linux", allow(dead_code))]
pub fn version() -> String {
    #[cfg(target_os = "macos")]
    {
        if let Ok(o) = Command::new("sw_vers").arg("-productVersion").output() {
            let v = String::from_utf8_lossy(&o.stdout).trim().to_string();
            if !v.is_empty() { return v; }
        }
    }
    #[cfg(target_os = "windows")]
    {
        if let Ok(o) = Command::new("cmd").args(["/C", "ver"]).output() {
            let v = String::from_utf8_lossy(&o.stdout).trim().to_string();
            if !v.is_empty() { return v; }
        }
    }
    String::new()
}

pub fn data_dir() -> Option<PathBuf> {
    if let Some(p) = std::env::var_os("XDG_DATA_HOME") { return Some(PathBuf::from(p)); }
    #[cfg(target_os = "windows")]
    {
        if let Some(p) = std::env::var_os("LOCALAPPDATA") { return Some(PathBuf::from(p)); }
    }
    let home = system::home()?;
    #[cfg(target_os = "macos")]
    { Some(home.join("Library/Application Support")) }
    #[cfg(not(target_os = "macos"))]
    { Some(home.join(".local/share")) }
}

pub fn config_dir() -> Option<PathBuf> {
    if let Some(p) = std::env::var_os("XDG_CONFIG_HOME") { return Some(PathBuf::from(p)); }
    #[cfg(target_os = "windows")]
    {
        if let Some(p) = std::env::var_os("APPDATA") { return Some(PathBuf::from(p)); }
    }
    let home = system::home()?;
    #[cfg(target_os = "macos")]
    { Some(home.join("Library/Application Support")) }
    #[cfg(not(target_os = "macos"))]
    { Some(home.join(".config")) }
}

/// Opens a fixed, already-validated URL in the user's browser.
pub fn open_url(url: &str) -> Result<(), String> {
    #[cfg(target_os = "linux")]
    let mut cmd = { let mut c = Command::new("xdg-open"); c.arg(url); c };
    #[cfg(target_os = "macos")]
    let mut cmd = { let mut c = Command::new("open"); c.arg(url); c };
    #[cfg(target_os = "windows")]
    let mut cmd = { let mut c = Command::new("rundll32"); c.args(["url.dll,FileProtocolHandler", url]); c };
    #[cfg(not(any(target_os = "linux", target_os = "macos", target_os = "windows")))]
    let mut cmd = { let mut c = Command::new("xdg-open"); c.arg(url); c };

    cmd.stdin(Stdio::null()).stdout(Stdio::null()).stderr(Stdio::null())
        .spawn().map(|_| ()).map_err(|e| format!("Could not open the browser: {e}"))
}

/// Builds a command for `program`. Windows shell shims (`.cmd`/`.bat`) are
/// wrapped in `cmd /C` because `CreateProcess` cannot run them directly.
pub fn command(program: &str) -> Command {
    #[cfg(target_os = "windows")]
    {
        if program.ends_with(".cmd") || program.ends_with(".bat") {
            let mut c = Command::new("cmd");
            c.arg("/C").arg(program);
            return c;
        }
    }
    Command::new(program)
}

/// A command wrapped in `timeout` where the platform has one. macOS and
/// Windows run the command directly; their Docker/SSH clients have their own
/// timeouts.
pub fn timeout(secs: u32, program: &str, args: &[&str]) -> Command {
    #[cfg(target_os = "linux")]
    {
        let mut c = Command::new("timeout");
        c.arg(secs.to_string()).arg(program).args(args);
        c
    }
    #[cfg(not(target_os = "linux"))]
    {
        let _ = secs;
        let mut c = command(program);
        c.args(args);
        c
    }
}

pub fn hostname() -> String {
    if let Ok(o) = Command::new("hostname").output() {
        let h = String::from_utf8_lossy(&o.stdout).trim().to_string();
        if !h.is_empty() { return h; }
    }
    #[cfg(target_os = "windows")]
    {
        if let Ok(h) = std::env::var("COMPUTERNAME") { if !h.is_empty() { return h; } }
    }
    "my computer".to_string()
}

/// Extensions that count as executable on this platform.
pub fn executable_extensions() -> &'static [&'static str] {
    #[cfg(target_os = "windows")]
    { &[".exe", ".cmd", ".bat", ".com"] }
    #[cfg(not(target_os = "windows"))]
    { &[] }
}

pub fn is_executable(path: &Path) -> bool {
    let Ok(md) = std::fs::metadata(path) else { return false };
    if !md.is_file() { return false; }
    #[cfg(unix)]
    { use std::os::unix::fs::PermissionsExt; md.permissions().mode() & 0o111 != 0 }
    #[cfg(not(unix))]
    { true }
}

/// Creates a directory that only the current user can access on Unix.
pub fn private_dir(path: &Path) -> std::io::Result<()> {
    let mut b = std::fs::DirBuilder::new();
    b.recursive(true);
    #[cfg(unix)]
    { use std::os::unix::fs::DirBuilderExt; b.mode(0o700); }
    b.create(path)
}

/// Creates or truncates a file that only the current user can access on Unix.
pub fn private_file(path: &Path) -> std::io::Result<std::fs::File> {
    let mut o = std::fs::OpenOptions::new();
    o.write(true).create(true).truncate(true);
    #[cfg(unix)]
    { use std::os::unix::fs::OpenOptionsExt; o.mode(0o600); }
    o.open(path)
}

/// Cross-platform random bytes for stack passwords.
pub fn random_bytes(buf: &mut [u8]) -> Result<(), String> {
    getrandom::fill(buf).map_err(|e| format!("Could not generate a password: {e}"))
}

pub fn path_separator() -> char {
    #[cfg(target_os = "windows")]
    { ';' }
    #[cfg(not(target_os = "windows"))]
    { ':' }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn random_bytes_fill_the_buffer_and_differ() {
        let mut a = [0u8; 32];
        let mut b = [0u8; 32];
        random_bytes(&mut a).unwrap();
        random_bytes(&mut b).unwrap();
        assert!(a.iter().any(|&x| x != 0));
        assert_ne!(a, b);
    }

    #[test]
    fn path_separator_matches_the_platform() {
        #[cfg(target_os = "windows")]
        assert_eq!(path_separator(), ';');
        #[cfg(not(target_os = "windows"))]
        assert_eq!(path_separator(), ':');
    }

    #[test]
    fn executable_extensions_match_the_platform() {
        #[cfg(target_os = "windows")]
        assert_eq!(executable_extensions(), [".exe", ".cmd", ".bat", ".com"].as_slice());
        #[cfg(not(target_os = "windows"))]
        assert!(executable_extensions().is_empty());
    }

    #[cfg(unix)]
    #[test]
    fn private_paths_are_owner_only() {
        use std::io::Write;
        use std::os::unix::fs::PermissionsExt;
        let dir = std::env::temp_dir().join(format!("devsetup-platform-{}-private", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        private_dir(&dir).unwrap();
        assert_eq!(std::fs::metadata(&dir).unwrap().permissions().mode() & 0o777, 0o700);
        let file = dir.join("secret");
        let mut f = private_file(&file).unwrap();
        f.write_all(b"secret").unwrap();
        assert_eq!(std::fs::metadata(&file).unwrap().permissions().mode() & 0o777, 0o600);
        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[cfg(unix)]
    #[test]
    fn is_executable_checks_the_mode_bit() {
        use std::os::unix::fs::PermissionsExt;
        let path = std::env::temp_dir().join(format!("devsetup-platform-{}-exec", std::process::id()));
        std::fs::write(&path, b"#!/bin/sh\n").unwrap();
        std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o644)).unwrap();
        assert!(!is_executable(&path));
        std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o755)).unwrap();
        assert!(is_executable(&path));
        std::fs::remove_file(&path).unwrap();
    }
}
