// VULCAN MODIFICATION: replaces the `codex_utils_cargo_bin` helpers used by the upstream host
// tests. Cargo exposes the host binary path to integration tests through `CARGO_BIN_EXE_*`.
// `write_executable` is copied from codex-rs/utils/cargo-bin/src/executable.rs at
// c419fd01a70dd1d05356da443ac44328bda2e6f4.

use std::io;
use std::path::PathBuf;

/// Returns the path of the host binary built for these integration tests.
pub(crate) fn cargo_bin(name: &str) -> io::Result<PathBuf> {
    if name != "vulcan-code-mode-host" {
        return Err(io::Error::new(
            io::ErrorKind::NotFound,
            format!("integration tests only build `vulcan-code-mode-host`, not `{name}`"),
        ));
    }
    Ok(PathBuf::from(env!("CARGO_BIN_EXE_vulcan-code-mode-host")))
}

#[cfg(unix)]
#[allow(dead_code)]
pub(crate) fn write_executable(path: &std::path::Path, script: &str) -> io::Result<()> {
    use std::fs;
    use std::os::unix::fs::PermissionsExt;

    #[cfg(target_os = "linux")]
    {
        // A sibling test can fork while a parent-owned file is open for writing.
        // CLOEXEC does not release that inherited descriptor until the sibling execs.
        let output = std::process::Command::new("/bin/sh")
            .arg("-c")
            .arg("printf '%s' \"$1\" > \"$2\"")
            .arg("write-executable-fixture")
            .arg(script)
            .arg(path)
            .output()?;
        if !output.status.success() {
            return Err(io::Error::other(format!(
                "failed to write executable fixture {}: {}",
                path.display(),
                String::from_utf8_lossy(&output.stderr),
            )));
        }
    }
    #[cfg(not(target_os = "linux"))]
    fs::write(path, script)?;
    fs::set_permissions(path, fs::Permissions::from_mode(/*mode*/ 0o755))
}
