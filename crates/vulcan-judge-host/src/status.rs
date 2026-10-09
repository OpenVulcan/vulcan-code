//! Process resource readings and latency statistics.
//! 进程资源读数与延迟统计。

/// Memory of the current process; a reading the platform cannot provide stays absent.
/// 当前进程的内存；平台无法提供的读数保持缺省。
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct ProcessMemory {
    /// Physical memory in use.
    /// 使用中的物理内存。
    pub working_set_bytes: Option<u64>,
    /// Private committed memory.
    /// 私有提交内存。
    pub private_bytes: Option<u64>,
}

/// Reads the memory counters of the current process.
/// 读取当前进程的内存计数器。
#[cfg(windows)]
pub fn process_memory() -> ProcessMemory {
    use windows_sys::Win32::System::ProcessStatus::{
        GetProcessMemoryInfo, PROCESS_MEMORY_COUNTERS, PROCESS_MEMORY_COUNTERS_EX,
    };
    use windows_sys::Win32::System::Threading::GetCurrentProcess;

    // SAFETY: the counters struct is plain data, its size is passed as `cb`, and the pseudo handle
    // of the current process needs no closing.
    // 安全性：计数器结构是纯数据，其大小作为 `cb` 传入；当前进程的伪句柄无需关闭。
    unsafe {
        let mut counters: PROCESS_MEMORY_COUNTERS_EX = std::mem::zeroed();
        let size = std::mem::size_of::<PROCESS_MEMORY_COUNTERS_EX>() as u32;
        counters.cb = size;
        let ok = GetProcessMemoryInfo(
            GetCurrentProcess(),
            (&raw mut counters).cast::<PROCESS_MEMORY_COUNTERS>(),
            size,
        );
        if ok == 0 {
            return ProcessMemory::default();
        }
        ProcessMemory {
            working_set_bytes: Some(counters.WorkingSetSize as u64),
            private_bytes: Some(counters.PrivateUsage as u64),
        }
    }
}

/// Reads the memory counters of the current process.
/// 读取当前进程的内存计数器。
#[cfg(not(windows))]
pub fn process_memory() -> ProcessMemory {
    ProcessMemory::default()
}

/// Nearest-rank percentile of unsorted samples; absent when there are none.
/// 未排序样本的最近秩百分位；没有样本时缺省。
pub fn percentile(samples: &[f64], fraction: f64) -> Option<f64> {
    if samples.is_empty() {
        return None;
    }
    let mut sorted = samples.to_vec();
    sorted.sort_by(f64::total_cmp);
    let rank = (fraction * sorted.len() as f64).ceil() as usize;
    Some(sorted[rank.clamp(1, sorted.len()) - 1])
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn percentiles_use_nearest_rank() {
        let samples: Vec<f64> = (1..=20).rev().map(f64::from).collect();
        assert_eq!(percentile(&samples, 0.5), Some(10.0));
        assert_eq!(percentile(&samples, 0.95), Some(19.0));
        assert_eq!(percentile(&[3.0], 0.95), Some(3.0));
        assert_eq!(percentile(&[], 0.5), None);
    }

    #[cfg(windows)]
    #[test]
    fn the_current_process_reports_memory() {
        let memory = process_memory();
        assert!(memory.working_set_bytes.is_some_and(|bytes| bytes > 0));
        assert!(memory.private_bytes.is_some_and(|bytes| bytes > 0));
    }
}
