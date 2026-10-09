//! GPU use of this process, read from the Windows performance counters that Task Manager shows.
//! 本进程的 GPU 占用，读取自任务管理器所显示的 Windows 性能计数器。
//!
//! The counters cover every adapter and every API (CUDA, Direct3D 12, Vulkan), so one reading
//! serves all devices. Utilization is a rate and needs two samples, so the first reading after
//! opening has none. Platforms without these counters report nothing rather than zero.
//! 这些计数器覆盖所有适配器与所有 API（CUDA、Direct3D 12、Vulkan），因此一次读数适用于所有设备。
//! 利用率是速率，需要两次采样，因此打开后的第一次读数没有利用率。没有这些计数器的平台不报告任何值，
//! 而不是报告 0。

/// One reading; values the platform cannot provide stay absent.
/// 一次读数；平台无法提供的值保持缺省。
#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct GpuUsage {
    /// Dedicated video memory committed by this process, summed over adapters.
    /// 本进程提交的专用显存，按适配器求和。
    pub dedicated_bytes: Option<u64>,
    /// Shared system memory used by this process for the GPU, summed over adapters.
    /// 本进程为 GPU 使用的共享系统内存，按适配器求和。
    pub shared_bytes: Option<u64>,
    /// Busiest engine type of this process since the previous reading, in percent.
    /// 自上次读数以来本进程最繁忙引擎类型的百分比。
    pub utilization_percent: Option<f64>,
}

/// Instance-name prefix of this process in the GPU counters.
/// GPU 计数器中本进程的实例名前缀。
#[cfg(any(windows, test))]
fn process_prefix(process_id: u32) -> String {
    format!("pid_{process_id}_")
}

/// Sums utilization per adapter and engine type, as Task Manager does, and returns the busiest.
/// 按适配器与引擎类型对利用率求和（与任务管理器一致），返回最繁忙者。
#[cfg(any(windows, test))]
fn busiest_engine<'a>(prefix: &str, engines: impl Iterator<Item = (&'a str, f64)>) -> f64 {
    let mut totals: std::collections::BTreeMap<(String, String), f64> = Default::default();
    for (name, value) in engines {
        let Some(rest) = name.strip_prefix(prefix) else {
            continue;
        };
        let adapter = rest.split("_phys_").next().unwrap_or(rest).to_string();
        let engine_type = rest
            .split_once("engtype_")
            .map_or("", |(_, engine_type)| engine_type)
            .to_string();
        *totals.entry((adapter, engine_type)).or_default() += value;
    }
    totals.into_values().fold(0.0, f64::max)
}

#[cfg(windows)]
mod platform {
    use windows_sys::Win32::System::Performance::{
        PDH_CSTATUS_NEW_DATA, PDH_CSTATUS_VALID_DATA, PDH_FMT, PDH_FMT_COUNTERVALUE_ITEM_W,
        PDH_FMT_DOUBLE, PDH_FMT_LARGE, PDH_HCOUNTER, PDH_HQUERY, PDH_MORE_DATA,
        PdhAddEnglishCounterW, PdhCloseQuery, PdhCollectQueryData, PdhGetFormattedCounterArrayW,
        PdhOpenQueryW,
    };

    use super::{GpuUsage, busiest_engine, process_prefix};

    /// Open counter query for this process.
    /// 本进程已打开的计数器查询。
    pub struct GpuCounters {
        query: PDH_HQUERY,
        dedicated: PDH_HCOUNTER,
        shared: PDH_HCOUNTER,
        engines: PDH_HCOUNTER,
        prefix: String,
        primed: bool,
    }

    /// Adds one wildcard counter in its English name, so it works under every display language.
    /// 以英文名称添加一个通配计数器，使其在任何显示语言下都可用。
    fn add_counter(query: PDH_HQUERY, path: &str) -> Option<PDH_HCOUNTER> {
        let wide: Vec<u16> = path.encode_utf16().chain(Some(0)).collect();
        let mut counter: PDH_HCOUNTER = std::ptr::null_mut();
        // SAFETY: `wide` is NUL-terminated and outlives the call; `counter` receives the handle.
        // 安全性：`wide` 以 NUL 结尾且在调用期间有效；`counter` 接收句柄。
        let status = unsafe { PdhAddEnglishCounterW(query, wide.as_ptr(), 0, &mut counter) };
        (status == 0).then_some(counter)
    }

    /// Reads every instance of a wildcard counter as `(instance name, value)`.
    /// 以 `(实例名, 值)` 读取通配计数器的每个实例。
    fn read_array<T>(
        counter: PDH_HCOUNTER,
        format: PDH_FMT,
        value: impl Fn(&PDH_FMT_COUNTERVALUE_ITEM_W) -> T,
    ) -> Option<Vec<(String, T)>> {
        let mut bytes = 0_u32;
        let mut count = 0_u32;
        // SAFETY: a null buffer with zero size asks for the required size only.
        // 安全性：空缓冲区与零大小只请求所需大小。
        let status = unsafe {
            PdhGetFormattedCounterArrayW(
                counter,
                format,
                &mut bytes,
                &mut count,
                std::ptr::null_mut(),
            )
        };
        if status == 0 {
            return Some(Vec::new());
        }
        if status != PDH_MORE_DATA {
            return None;
        }
        let item_size = std::mem::size_of::<PDH_FMT_COUNTERVALUE_ITEM_W>();
        let mut buffer =
            vec![PDH_FMT_COUNTERVALUE_ITEM_W::default(); (bytes as usize).div_ceil(item_size)];
        // SAFETY: the buffer holds at least `bytes` bytes, aligned for the item type; the instance
        // names it points to live inside the same buffer.
        // 安全性：缓冲区至少有 `bytes` 字节且按条目类型对齐；其指向的实例名位于同一缓冲区内。
        let status = unsafe {
            PdhGetFormattedCounterArrayW(
                counter,
                format,
                &mut bytes,
                &mut count,
                buffer.as_mut_ptr(),
            )
        };
        if status != 0 {
            return None;
        }
        let items = buffer.get(..count as usize)?;
        Some(
            items
                .iter()
                .filter(|item| {
                    matches!(
                        item.FmtValue.CStatus,
                        PDH_CSTATUS_VALID_DATA | PDH_CSTATUS_NEW_DATA
                    )
                })
                .map(|item| {
                    // SAFETY: `szName` is a NUL-terminated string inside `buffer`.
                    // 安全性：`szName` 是位于 `buffer` 内、以 NUL 结尾的字符串。
                    let name = unsafe {
                        let mut length = 0;
                        while *item.szName.add(length) != 0 {
                            length += 1;
                        }
                        String::from_utf16_lossy(std::slice::from_raw_parts(item.szName, length))
                    };
                    (name, value(item))
                })
                .collect(),
        )
    }

    impl GpuCounters {
        /// Opens the counters for `process_id`; `None` when the system has no GPU counters.
        /// 为 `process_id` 打开计数器；系统没有 GPU 计数器时返回 `None`。
        pub fn open(process_id: u32) -> Option<Self> {
            let mut query: PDH_HQUERY = std::ptr::null_mut();
            // SAFETY: a null data source selects live data; `query` receives the handle.
            // 安全性：空数据源表示实时数据；`query` 接收句柄。
            if unsafe { PdhOpenQueryW(std::ptr::null(), 0, &mut query) } != 0 {
                return None;
            }
            let counters = (|| {
                Some(Self {
                    query,
                    dedicated: add_counter(query, r"\GPU Process Memory(*)\Dedicated Usage")?,
                    shared: add_counter(query, r"\GPU Process Memory(*)\Shared Usage")?,
                    engines: add_counter(query, r"\GPU Engine(*)\Utilization Percentage")?,
                    prefix: process_prefix(process_id),
                    primed: false,
                })
            })();
            if counters.is_none() {
                // SAFETY: the query was opened above and is not used afterwards.
                // 安全性：查询已在上方打开，之后不再使用。
                unsafe { PdhCloseQuery(query) };
            }
            let mut counters = counters?;
            counters.collect();
            Some(counters)
        }

        /// Takes one sample; the first one only primes the utilization rate.
        /// 采集一次样本；第一次只用于初始化利用率速率。
        fn collect(&mut self) -> bool {
            // SAFETY: the query handle stays open for the lifetime of `self`.
            // 安全性：查询句柄在 `self` 的生命周期内保持打开。
            let collected = unsafe { PdhCollectQueryData(self.query) } == 0;
            let had_previous = self.primed;
            self.primed = collected;
            collected && had_previous
        }

        /// Reads the GPU use of the process since the previous reading.
        /// 读取自上次读数以来本进程的 GPU 占用。
        pub fn read(&mut self) -> GpuUsage {
            let rate_ready = self.collect();
            if !self.primed {
                return GpuUsage::default();
            }
            let prefix = self.prefix.as_str();
            let memory = |counter| {
                read_array(counter, PDH_FMT_LARGE, |item| {
                    // SAFETY: the value was formatted as a 64-bit integer.
                    // 安全性：该值已按 64 位整数格式化。
                    unsafe { item.FmtValue.Anonymous.largeValue }
                })
                .map(|values| {
                    values
                        .iter()
                        .filter(|(name, _)| name.starts_with(prefix))
                        .map(|(_, bytes)| u64::try_from(*bytes).unwrap_or(0))
                        .sum::<u64>()
                })
            };
            let utilization_percent = rate_ready
                .then(|| {
                    read_array(self.engines, PDH_FMT_DOUBLE, |item| {
                        // SAFETY: the value was formatted as a double.
                        // 安全性：该值已按双精度浮点格式化。
                        unsafe { item.FmtValue.Anonymous.doubleValue }
                    })
                })
                .flatten()
                .map(|values| {
                    busiest_engine(
                        prefix,
                        values.iter().map(|(name, value)| (name.as_str(), *value)),
                    )
                });
            GpuUsage {
                dedicated_bytes: memory(self.dedicated),
                shared_bytes: memory(self.shared),
                utilization_percent,
            }
        }
    }

    impl Drop for GpuCounters {
        fn drop(&mut self) {
            // SAFETY: the query is open and closed exactly once here.
            // 安全性：查询处于打开状态，并在此处恰好关闭一次。
            unsafe { PdhCloseQuery(self.query) };
        }
    }
}

#[cfg(not(windows))]
mod platform {
    use super::GpuUsage;

    /// Placeholder on platforms without per-process GPU counters.
    /// 没有进程级 GPU 计数器的平台上的占位实现。
    pub struct GpuCounters;

    impl GpuCounters {
        /// No counters exist on this platform.
        /// 本平台不存在计数器。
        pub fn open(_process_id: u32) -> Option<Self> {
            None
        }

        /// Never called, since [`GpuCounters::open`] returns `None`.
        /// 不会被调用，因为 [`GpuCounters::open`] 返回 `None`。
        pub fn read(&mut self) -> GpuUsage {
            GpuUsage::default()
        }
    }
}

pub use platform::GpuCounters;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn utilization_sums_engines_of_one_type_and_ignores_other_processes() {
        let prefix = process_prefix(42);
        let engines = [
            ("pid_42_luid_0x1_phys_0_eng_0_engtype_3D", 20.0),
            ("pid_42_luid_0x1_phys_0_eng_1_engtype_3D", 15.0),
            ("pid_42_luid_0x1_phys_0_eng_2_engtype_Compute_0", 30.0),
            ("pid_420_luid_0x1_phys_0_eng_0_engtype_3D", 99.0),
            ("pid_7_luid_0x1_phys_0_eng_0_engtype_3D", 99.0),
        ];
        assert_eq!(busiest_engine(&prefix, engines.into_iter()), 35.0);
    }

    #[cfg(windows)]
    #[test]
    fn the_current_process_reads_gpu_memory_when_counters_exist() {
        if let Some(mut counters) = GpuCounters::open(std::process::id()) {
            let usage = counters.read();
            assert!(usage.dedicated_bytes.is_some());
            assert!(usage.utilization_percent.is_some());
        }
    }
}
