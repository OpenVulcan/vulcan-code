// `vulcan-tui-assets sync [--check]`
//
// Copies brand assets from the WebUI into `assets/` byte for byte, as listed in `assets/sync.json`.
// The TUI never edits these files; `--check` fails when a local copy differs from upstream.
// 按 `assets/sync.json` 的清单，将 WebUI 的品牌资源逐字节复制到 `assets/`。TUI 从不修改这些文件；
// `--check` 在本地副本与上游不一致时失败。

use std::path::{Path, PathBuf};
use std::process::ExitCode;

use anyhow::{Context, Result, bail};
use serde::Deserialize;

#[derive(Deserialize)]
struct Config {
    files: Vec<Entry>,
}

#[derive(Deserialize)]
struct Entry {
    /// Upstream path, relative to the repository root.
    /// 上游路径，相对于仓库根目录。
    from: String,
    /// Local path, relative to the repository root.
    /// 本地路径，相对于仓库根目录。
    to: String,
}

fn main() -> ExitCode {
    match real_main() {
        Ok(code) => code,
        Err(error) => {
            eprintln!("error: {error:#}");
            ExitCode::FAILURE
        }
    }
}

fn real_main() -> Result<ExitCode> {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let mut args = std::env::args().skip(1);
    if args.next().as_deref() != Some("sync") {
        bail!("usage: vulcan-tui-assets sync [--check]");
    }
    let check = match args.next().as_deref() {
        None => false,
        Some("--check") => true,
        Some(other) => bail!("unknown argument `{other}`"),
    };
    let config_path = root.join("assets/sync.json");
    let config: Config = serde_json::from_str(
        &std::fs::read_to_string(&config_path)
            .with_context(|| format!("read {}", config_path.display()))?,
    )
    .with_context(|| format!("parse {}", config_path.display()))?;

    let mut stale = Vec::new();
    for entry in &config.files {
        let from = root.join(&entry.from);
        let to = root.join(&entry.to);
        let upstream = std::fs::read(&from).with_context(|| format!("read {}", from.display()))?;
        let local = std::fs::read(&to).ok();
        if local.as_deref() == Some(upstream.as_slice()) {
            continue;
        }
        if check {
            stale.push(entry.to.clone());
            continue;
        }
        write(&to, &upstream)?;
        println!("wrote {}", entry.to);
    }
    if !stale.is_empty() {
        for path in &stale {
            println!("out of date: {path}");
        }
        println!("run `cargo run --bin vulcan-tui-assets -- sync` to update");
        return Ok(ExitCode::FAILURE);
    }
    println!("assets are up to date");
    Ok(ExitCode::SUCCESS)
}

fn write(path: &Path, bytes: &[u8]) -> Result<()> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    std::fs::write(path, bytes).with_context(|| format!("write {}", path.display()))
}
