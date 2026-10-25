//! Global Vulcan environment variables seen by MCP servers. MCP servers are process-wide, so only
//! the global layer applies to them; the service publishes it at startup and after each change, and
//! a running server keeps the environment it was launched with until it restarts.
//! MCP 服务可见的全局 Vulcan 环境变量。MCP 服务是进程级的，因此只有全局层适用于它们；服务在启动时以及
//! 每次变更后发布该层，运行中的服务保留其启动时的环境，直到重启。

use std::collections::BTreeMap;
use std::sync::RwLock;

/// Published global variables keyed by their ASCII-uppercase name, with the stored spelling.
/// 已发布的全局变量，以 ASCII 大写名称为键，保留存储时的拼写。
static OVERLAY: RwLock<BTreeMap<String, (String, String)>> = RwLock::new(BTreeMap::new());

/// Replaces the published global variables with `variables` (name and value pairs).
/// 用 `variables`（名称与值）替换已发布的全局变量。
pub fn publish_mcp_environment_overlay(variables: Vec<(String, String)>) {
    let overlay = variables
        .into_iter()
        .map(|(name, value)| (name.to_ascii_uppercase(), (name, value)))
        .collect();
    match OVERLAY.write() {
        Ok(mut guard) => *guard = overlay,
        Err(poisoned) => *poisoned.into_inner() = overlay,
    }
}

/// Published global variables as name and value pairs, in name order.
/// 已发布的全局变量（名称与值），按名称排序。
pub(crate) fn mcp_environment_overlay() -> Vec<(String, String)> {
    let guard = match OVERLAY.read() {
        Ok(guard) => guard,
        Err(poisoned) => poisoned.into_inner(),
    };
    guard.values().cloned().collect()
}

/// Looks one variable up in the published global variables first, then in the process
/// environment. Names compare case-insensitively against the published variables.
/// 先在已发布的全局变量中查找一个变量，再查进程环境。与已发布变量比较名称时不区分大小写。
pub(crate) fn lookup_mcp_environment(name: &str) -> Option<String> {
    let published = {
        let guard = match OVERLAY.read() {
            Ok(guard) => guard,
            Err(poisoned) => poisoned.into_inner(),
        };
        guard
            .get(&name.to_ascii_uppercase())
            .map(|(_, value)| value.clone())
    };
    published.or_else(|| std::env::var(name).ok())
}
