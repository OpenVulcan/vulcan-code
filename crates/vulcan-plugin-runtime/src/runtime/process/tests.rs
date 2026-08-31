//! Windows process-path projection regression.
//! Windows进程路径投影回归。

use super::build_plugin_process_spec;
use serde_json::{Map, Value};
use std::collections::BTreeMap;
use std::path::PathBuf;

/// Verifies plugin process specifications expose standard Windows paths to the Lua bridge.
/// 验证插件进程规格向 Lua 桥暴露标准 Windows 路径。
#[cfg(windows)]
#[test]
fn plugin_process_spec_normalizes_windows_verbatim_paths() {
    let spec = build_plugin_process_spec(
        PathBuf::from(r"\\?\D:\openvulcan\runtime\node.exe"),
        PathBuf::from(r"\\?\D:\openvulcan\plugins\context7"),
        Vec::new(),
        "exec",
        &Map::new(),
        None,
        BTreeMap::new(),
    )
    .expect("build plugin process specification");

    assert_eq!(
        spec.get("program").and_then(Value::as_str),
        Some(r"D:\openvulcan\runtime\node.exe")
    );
    assert_eq!(
        spec.get("cwd").and_then(Value::as_str),
        Some(r"D:\openvulcan\plugins\context7")
    );
}
