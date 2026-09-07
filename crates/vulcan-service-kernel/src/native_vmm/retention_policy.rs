//! Validated native cleanup defaults and conversion into an admitted Core policy.
//! 原生清洗默认值校验及向已准入 Core 策略的转换。

use std::time::{Duration, SystemTime, UNIX_EPOCH};
use tokio::time::Instant;
use vulcan_memorymesh_core::{MemoryLevel, RetentionPolicy};
use vulcan_service_api::VmmNativeRetentionSettingsSummary;

/// Computes a cadence delay from the last completed pass without resetting it on policy polls.
/// 根据上一轮完成时间计算周期等待，不因策略轮询而重置。
/// `None` requests an eager initial pass; elapsed or shortened intervals return zero.
/// `None` 要求立即执行首轮；已过去或缩短后的周期返回零。
pub(super) fn remaining_wait(
    now: Instant,
    previous: Option<Instant>,
    interval: Duration,
) -> Duration {
    previous.map_or(Duration::ZERO, |previous| {
        interval.saturating_sub(now.saturating_duration_since(previous))
    })
}

/// Returns host cadence defaults and business defaults owned by Core, without persisting them.
/// 返回宿主周期默认值和 Core 持有的业务默认值，不进行持久化。
pub(super) fn defaults() -> VmmNativeRetentionSettingsSummary {
    let policy = RetentionPolicy::v1();
    VmmNativeRetentionSettingsSummary {
        enabled: true,
        scan_interval_minutes: 30,
        profile_expiry_interval_seconds: 30,
        turn_hot_window_size: policy.turn_hot_window_size,
        session_idle_ms: policy.session_idle_ms,
        trash_retention_ms: policy.trash_retention_ms,
        protected_priority_floor: policy.protected_priority_floor,
        protected_memory_level_floor: policy.protected_memory_level_floor,
        skip_protected_shared_memories: policy.skip_protected_shared_memories,
    }
}

/// Rejects invalid settings instead of clamping them; accepts only representable native durations.
/// 拒绝无效设置而不截断，仅接纳原生时间表示范围内的时长。
/// `settings` is the complete submitted cleanup group; success returns no value.
/// `settings` 为完整提交的清洗配置组；成功时无返回内容。
pub(super) fn validate(settings: &VmmNativeRetentionSettingsSummary) -> Result<(), String> {
    // New recycle deadlines must fit after the current timestamp, not merely as standalone durations.
    // 新回收截止时间必须在当前时间戳之后仍可表示，不能只检查孤立时长。
    let now_ns = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_err(|error| format!("原生 VMM 清洗配置无法读取当前时间：{error}"))?
        .as_nanos();
    let now_ns =
        i64::try_from(now_ns).map_err(|error| format!("原生 VMM 当前时间超出纳秒范围：{error}"))?;
    validate_at(settings, now_ns)
}

/// Validates exact business values at an admitted wall-clock instant without altering them.
/// 在已准入的墙钟时刻验证准确业务值，不修改这些值。
/// `now_ns` is Unix nanoseconds; invalid settings or unrepresentable deadlines return an error.
/// `now_ns` 为 Unix 纳秒；设置无效或截止时间不可表示时返回错误。
fn validate_at(settings: &VmmNativeRetentionSettingsSummary, now_ns: i64) -> Result<(), String> {
    if settings.scan_interval_minutes == 0 || settings.profile_expiry_interval_seconds == 0 {
        return Err(String::from("原生 VMM 清洗周期必须大于零。"));
    }
    if settings.turn_hot_window_size == 0
        || settings.protected_priority_floor > 100
        || !(MemoryLevel::Session as u32..=MemoryLevel::Persistent as u32)
            .contains(&settings.protected_memory_level_floor)
    {
        return Err(String::from("原生 VMM 热回合窗口或保护阈值无效。"));
    }
    for duration in [settings.session_idle_ms, settings.trash_retention_ms] {
        if duration == 0 || u128::from(duration) * 1_000_000 > i64::MAX as u128 {
            return Err(String::from("原生 VMM 保留时长必须大于零且可表示为纳秒。"));
        }
    }
    let expiry = i128::from(now_ns) + i128::from(settings.trash_retention_ms) * 1_000_000;
    if now_ns < 0 || expiry > i128::from(i64::MAX) {
        return Err(String::from(
            "原生 VMM 回收保留时长过大：从当前时间计算的新批次截止时间超出可表示范围。",
        ));
    }
    Ok(())
}

/// Resolves one immutable cleanup group while preserving host-owned bounded work limits.
/// 解析一份不可变清洗配置组，并保留宿主持有的有界工作上限。
/// Returns the exact executable policy or a configuration error; no AI route is consulted.
/// 返回准确的可执行策略或配置错误；不访问任何 AI 路由。
pub(super) fn resolve(
    settings: &VmmNativeRetentionSettingsSummary,
) -> Result<RetentionPolicy, String> {
    validate(settings)?;
    let mut policy = RetentionPolicy::v1();
    policy.turn_hot_window_size = settings.turn_hot_window_size;
    policy.session_idle_ms = settings.session_idle_ms;
    policy.trash_retention_ms = settings.trash_retention_ms;
    policy.protected_priority_floor = settings.protected_priority_floor;
    policy.protected_memory_level_floor = settings.protected_memory_level_floor;
    policy.skip_protected_shared_memories = settings.skip_protected_shared_memories;
    Ok(policy)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Verifies the future deadline, not just its duration, fits the persisted timestamp type.
    /// 验证未来截止时间而非仅时长本身能够放入持久化时间戳类型。
    #[test]
    fn recycle_duration_must_fit_after_the_current_time() {
        let now_ns = 1_800_000_000_000_000_000_i64;
        let mut settings = defaults();
        settings.trash_retention_ms = ((i64::MAX - now_ns) / 1_000_000) as u64;
        assert!(validate_at(&settings, now_ns).is_ok());
        settings.trash_retention_ms += 1;
        assert!(
            validate_at(&settings, now_ns)
                .expect_err("future overflow")
                .contains("截止时间")
        );
        settings = defaults();
        assert!(validate_at(&settings, -1).is_err());
    }

    /// Verifies direct conversion preserves every business rule and leaves work bounds in Core.
    /// 验证直接转换保留全部业务规则，工作上限仍归 Core 持有。
    #[test]
    fn settings_resolve_exact_rules_without_changing_work_limits() {
        let mut settings = defaults();
        settings.turn_hot_window_size = 2;
        settings.session_idle_ms = 17;
        settings.trash_retention_ms = 23;
        settings.protected_priority_floor = 92;
        settings.protected_memory_level_floor = 4;
        settings.skip_protected_shared_memories = false;
        let mut expected = RetentionPolicy::v1();
        expected.turn_hot_window_size = 2;
        expected.session_idle_ms = 17;
        expected.trash_retention_ms = 23;
        expected.protected_priority_floor = 92;
        expected.protected_memory_level_floor = 4;
        expected.skip_protected_shared_memories = false;
        assert_eq!(resolve(&settings).expect("valid policy"), expected);
    }

    /// Verifies invalid periods, thresholds, and durations never become silently adjusted values.
    /// 验证无效周期、阈值和时长绝不被静默调整。
    #[test]
    fn invalid_settings_are_rejected() {
        let mut invalid = defaults();
        invalid.scan_interval_minutes = 0;
        assert!(validate(&invalid).is_err());
        invalid = defaults();
        invalid.profile_expiry_interval_seconds = 0;
        assert!(validate(&invalid).is_err());
        for value in [0, 5, u32::MAX] {
            invalid = defaults();
            invalid.protected_memory_level_floor = value;
            assert!(validate(&invalid).is_err());
        }
        for value in [0, u64::MAX] {
            invalid = defaults();
            invalid.session_idle_ms = value;
            assert!(validate(&invalid).is_err());
            invalid = defaults();
            invalid.trash_retention_ms = value;
            assert!(validate(&invalid).is_err());
        }
        invalid = defaults();
        invalid.protected_priority_floor = 101;
        assert!(validate(&invalid).is_err());
        invalid = defaults();
        invalid.turn_hot_window_size = 0;
        assert!(validate(&invalid).is_err());
    }

    /// Verifies file polls and shortened or extended periods preserve the original schedule origin.
    /// 验证文件轮询及周期缩短或延长时保留原始调度起点。
    #[test]
    fn cadence_reloads_do_not_reset_elapsed_time() {
        let previous = Instant::now();
        let now = previous + Duration::from_secs(90);
        assert_eq!(
            remaining_wait(now, None, Duration::from_secs(120)),
            Duration::ZERO
        );
        assert_eq!(
            remaining_wait(now, Some(previous), Duration::from_secs(120)),
            Duration::from_secs(30)
        );
        assert_eq!(
            remaining_wait(now, Some(previous), Duration::from_secs(60)),
            Duration::ZERO
        );
        assert_eq!(
            remaining_wait(now, Some(previous), Duration::from_secs(180)),
            Duration::from_secs(90)
        );
    }
}
