//! Process-tree diagnostics: which descendants of a job still run and how much CPU time they used.
//! 进程树诊断：任务的哪些后代进程仍在运行，以及它们使用了多少 CPU 时间。
//!
//! The snapshot uses the system's own process listing (`ps` on Unix, CIM on Windows) so no process
//! inspection library is needed; it is taken only on request.
//! 快照使用系统自带的进程列表（Unix 上为 `ps`，Windows 上为 CIM），因此无需进程检查库；只在请求时采集。

use std::collections::{HashMap, HashSet};
use std::time::Duration;

use serde::Serialize;

use crate::process_creation::new_background_tokio_command;

/// Longest time one process listing may take.
/// 单次进程列表采集的最长时间。
const PROCESS_LISTING_TIMEOUT: Duration = Duration::from_secs(10);

/// Most processes one snapshot reports.
/// 单次快照报告的最多进程数。
const PROCESS_SNAPSHOT_LIMIT: usize = 64;

/// One process of a job's tree.
/// 任务进程树中的一个进程。
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct ShellJobProcess {
    /// Process id.
    /// 进程 ID。
    pub pid: u32,
    /// Parent process id.
    /// 父进程 ID。
    pub parent_pid: u32,
    /// Executable or command name.
    /// 可执行文件或命令名。
    pub name: String,
    /// CPU time used so far in milliseconds.
    /// 迄今使用的 CPU 时间（毫秒）。
    pub cpu_time_ms: u64,
}

/// One row of the system process listing.
/// 系统进程列表中的一行。
#[derive(Debug, Clone)]
struct ListedProcess {
    /// Process id.
    /// 进程 ID。
    pid: u32,
    /// Parent process id.
    /// 父进程 ID。
    parent_pid: u32,
    /// CPU time in milliseconds.
    /// CPU 时间（毫秒）。
    cpu_time_ms: u64,
    /// Name.
    /// 名称。
    name: String,
}

/// Lists the still-running processes of the tree rooted at `root_pid`, root first.
/// 列出以 `root_pid` 为根的进程树中仍在运行的进程，根进程在前。
///
/// # Returns
/// 返回值
/// Returns an error when the system process listing could not be read.
/// 无法读取系统进程列表时返回错误。
pub async fn snapshot_process_tree(root_pid: u32) -> Result<Vec<ShellJobProcess>, String> {
    let listed = list_processes().await?;
    Ok(tree_of(root_pid, &listed))
}

/// Selects the tree rooted at `root_pid` from one listing, breadth first.
/// 从一份列表中按广度优先选出以 `root_pid` 为根的进程树。
fn tree_of(root_pid: u32, listed: &[ListedProcess]) -> Vec<ShellJobProcess> {
    let mut children: HashMap<u32, Vec<&ListedProcess>> = HashMap::new();
    for process in listed {
        children
            .entry(process.parent_pid)
            .or_default()
            .push(process);
    }
    let mut result = Vec::new();
    let mut seen = HashSet::new();
    let mut queue: Vec<&ListedProcess> = listed
        .iter()
        .filter(|process| process.pid == root_pid)
        .collect();
    while let Some(process) = (!queue.is_empty()).then(|| queue.remove(0)) {
        if !seen.insert(process.pid) || result.len() >= PROCESS_SNAPSHOT_LIMIT {
            continue;
        }
        result.push(ShellJobProcess {
            pid: process.pid,
            parent_pid: process.parent_pid,
            name: process.name.clone(),
            cpu_time_ms: process.cpu_time_ms,
        });
        if let Some(next) = children.get(&process.pid) {
            queue.extend(next.iter().copied());
        }
    }
    result
}

/// Reads the system process listing.
/// 读取系统进程列表。
async fn list_processes() -> Result<Vec<ListedProcess>, String> {
    #[cfg(windows)]
    let mut command = {
        let mut command = new_background_tokio_command("powershell.exe");
        command.args([
            "-NoProfile",
            "-NonInteractive",
            "-Command",
            "Get-CimInstance Win32_Process | ForEach-Object { \"$($_.ProcessId)`t$($_.ParentProcessId)`t$([uint64]$_.KernelModeTime + [uint64]$_.UserModeTime)`t$($_.Name)\" }",
        ]);
        command
    };
    #[cfg(not(windows))]
    let mut command = {
        let mut command = new_background_tokio_command("ps");
        command.args(["-A", "-o", "pid=,ppid=,time=,comm="]);
        command
    };
    command.kill_on_drop(true);
    let output = tokio::time::timeout(PROCESS_LISTING_TIMEOUT, command.output())
        .await
        .map_err(|_| String::from("Listing processes took too long."))?
        .map_err(|error| format!("Failed to list processes: {error}"))?;
    if !output.status.success() {
        return Err(format!(
            "Listing processes failed with exit code {:?}.",
            output.status.code()
        ));
    }
    let text = String::from_utf8_lossy(&output.stdout);
    Ok(text.lines().filter_map(parse_listing_line).collect())
}

/// Parses one listing line.
/// 解析一行列表。
#[cfg(windows)]
fn parse_listing_line(line: &str) -> Option<ListedProcess> {
    let mut fields = line.trim_end_matches('\r').splitn(4, '\t');
    let pid = fields.next()?.trim().parse().ok()?;
    let parent_pid = fields.next()?.trim().parse().ok()?;
    // CIM reports kernel and user time in 100-nanosecond units.
    // CIM 以 100 纳秒为单位报告内核与用户时间。
    let cpu_units: u64 = fields.next()?.trim().parse().ok()?;
    let name = fields.next().unwrap_or_default().trim().to_string();
    Some(ListedProcess {
        pid,
        parent_pid,
        cpu_time_ms: cpu_units / 10_000,
        name,
    })
}

/// Parses one listing line.
/// 解析一行列表。
#[cfg(not(windows))]
fn parse_listing_line(line: &str) -> Option<ListedProcess> {
    let mut fields = line.split_whitespace();
    let pid = fields.next()?.parse().ok()?;
    let parent_pid = fields.next()?.parse().ok()?;
    let cpu_time_ms = parse_ps_time(fields.next()?)?;
    let name = fields.collect::<Vec<_>>().join(" ");
    Some(ListedProcess {
        pid,
        parent_pid,
        cpu_time_ms,
        name,
    })
}

/// Parses `ps` cumulative time `[[dd-]hh:]mm:ss[.cc]` into milliseconds.
/// 把 `ps` 的累计时间 `[[dd-]hh:]mm:ss[.cc]` 解析为毫秒。
#[cfg_attr(windows, allow(dead_code))]
fn parse_ps_time(text: &str) -> Option<u64> {
    let (days, rest) = match text.split_once('-') {
        Some((days, rest)) => (days.parse::<u64>().ok()?, rest),
        None => (0, text),
    };
    let mut seconds = 0.0_f64;
    for part in rest.split(':') {
        seconds = seconds * 60.0 + part.parse::<f64>().ok()?;
    }
    Some(days * 86_400_000 + (seconds * 1000.0) as u64)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The tree selection follows parent links from the root and ignores unrelated processes.
    /// 进程树选择沿父链接从根出发，忽略无关进程。
    #[test]
    fn tree_selection_follows_parent_links_from_the_root() {
        let listed = [(1, 0), (10, 1), (11, 10), (12, 10), (20, 1), (30, 99)]
            .into_iter()
            .map(|(pid, parent_pid)| ListedProcess {
                pid,
                parent_pid,
                cpu_time_ms: 0,
                name: format!("p{pid}"),
            })
            .collect::<Vec<_>>();
        let pids: Vec<u32> = tree_of(10, &listed)
            .iter()
            .map(|process| process.pid)
            .collect();
        assert_eq!(pids, vec![10, 11, 12]);
    }

    /// `ps` times with days, hours and fractions convert to milliseconds.
    /// 带天、小时与小数的 `ps` 时间转换为毫秒。
    #[test]
    fn ps_time_converts_to_milliseconds() {
        assert_eq!(parse_ps_time("00:01:02"), Some(62_000));
        assert_eq!(parse_ps_time("1-00:00:01"), Some(86_401_000));
        assert_eq!(parse_ps_time("0:01.50"), Some(1_500));
    }
}
