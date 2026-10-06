//! `vulcan-tui-i18n sync [--check] [--webui-src <dir>]`
//!
//! Pulls the keys used by the TUI from the WebUI catalogs. See `src/i18n_sync/mod.rs`.
//! 从 WebUI 目录拉取 TUI 使用的文案键。详见 `src/i18n_sync/mod.rs`。

use std::path::PathBuf;
use std::process::ExitCode;

use anyhow::{Result, bail};
#[path = "../i18n_sync/mod.rs"]
mod i18n_sync;

use i18n_sync::{SyncOptions, load_config, run};

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
    let repo_root = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let mut args = std::env::args().skip(1);
    let Some(command) = args.next() else {
        bail!("usage: vulcan-tui-i18n sync [--check] [--webui-src <dir>]");
    };
    if command != "sync" {
        bail!("unknown command `{command}`; expected `sync`");
    }
    let mut check = false;
    let mut webui_src: Option<PathBuf> = None;
    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--check" => check = true,
            "--webui-src" => match args.next() {
                Some(dir) => webui_src = Some(PathBuf::from(dir)),
                None => bail!("--webui-src needs a directory"),
            },
            other => bail!("unknown argument `{other}`"),
        }
    }
    let webui_src = match webui_src {
        Some(dir) => dir,
        None => repo_root.join(load_config(&repo_root)?.webui_src),
    };
    let report = run(&SyncOptions {
        repo_root,
        webui_src: webui_src.clone(),
        check,
    })?;

    println!("upstream: {}", webui_src.display());
    println!(
        "used keys: {}, locales: {}",
        report.used_keys,
        report.locales.join(", ")
    );
    for (locale, keys) in &report.missing {
        println!(
            "fallback to default in {locale}: {} key(s): {}",
            keys.len(),
            keys.join(", ")
        );
    }
    for warning in &report.warnings {
        println!("warning: {warning}");
    }
    for file in &report.written {
        println!("wrote {file}");
    }
    for file in &report.removed {
        println!("removed {file}");
    }
    if check && !report.out_of_date.is_empty() {
        for file in &report.out_of_date {
            println!("out of date: {file}");
        }
        println!("run `cargo run --bin vulcan-tui-i18n -- sync` to update");
        return Ok(ExitCode::FAILURE);
    }
    if report.written.is_empty() && report.removed.is_empty() {
        println!("catalogs are up to date");
    }
    Ok(ExitCode::SUCCESS)
}
