//! Bounded evidence of actual VCE event-consumer polling, not inferred filesystem activity.
//! VCE 事件消费者实际轮询的有界证据，不推断文件系统活动。

use std::time::{Duration, Instant, SystemTime};

/// Healthy idle consumers still poll once per second; no source IO or refresh is performed.
/// 健康空闲消费者仍每秒轮询一次；不执行源码输入输出或刷新。
pub(super) const VCE_WATCH_OBSERVATION_INTERVAL: Duration = Duration::from_secs(1);
/// Observation becomes uncertain after five missed polling intervals, independent of update cooldown.
/// 丢失五个轮询间隔后观察变为不确定，独立于更新冷却。
const VCE_WATCH_OBSERVATION_MAX_GAP: Duration = Duration::from_secs(5);

/// A sampling result distinguishes current liveness from a newly detected continuity break.
/// 采样结果区分当前存活与新检测到的连续性中断。
pub(super) struct VceWatchClockSample {
    /// True only while polling evidence is within its declared bound or the consumer has just polled.
    /// 仅轮询证据处于声明边界内或消费者刚完成轮询时为真。
    pub(super) live: bool,
    /// True once per observed interruption, so repeated status reads do not create new gaps.
    /// 每次观察中断仅为真一次，避免重复状态读取创建新缺口。
    pub(super) new_gap: bool,
}

/// Retain monotonic and wall-clock evidence from the actual consumer, never from status readers.
/// 保留实际消费者的单调及墙上时钟证据，绝不使用状态读取者充当消费者。
pub(super) struct VceWatchDeliveryClock {
    /// Last monotonic instant sampled by the consumer itself.
    /// 消费者自身最近采样的单调时刻。
    last_poll: Instant,
    /// Last wall-clock sample detects gaps not represented by a platform's monotonic clock.
    /// 最近墙上时钟采样检测平台单调时钟未反映的缺口。
    last_wall: SystemTime,
    /// A reported interruption remains unlive until the consumer actually resumes polling.
    /// 已报告中断保持非存活，直到消费者实际恢复轮询。
    gap_reported: bool,
}

impl VceWatchDeliveryClock {
    /// Start an observation window at monotonic/wall; takes no source identity and returns local state.
    /// 在 monotonic/wall 开始观察窗口；不接收源码身份，返回本地状态。
    pub(super) fn new(monotonic: Instant, wall: SystemTime) -> Self {
        Self {
            last_poll: monotonic,
            last_wall: wall,
            gap_reported: false,
        }
    }

    /// Sample current time without treating a status lookup as a successful consumer poll.
    /// 采样当前时间，不将状态查询当成成功消费者轮询。
    /// monotonic/wall are captured together; consumer_polled is true only in the event task.
    /// monotonic/wall 一起捕获；consumer_polled 仅在事件任务内为真。
    /// Returns live/new-gap evidence; no timer, filesystem operation or indexing task is created.
    /// 返回存活及新缺口证据；不创建计时器、文件系统操作或索引任务。
    pub(super) fn sample(
        &mut self,
        monotonic: Instant,
        wall: SystemTime,
        consumer_polled: bool,
    ) -> VceWatchClockSample {
        // Both clocks are required evidence, not alternative timestamp fallbacks.
        // 两个时钟都是必要证据，而不是备选时间戳兜底。
        let monotonic_valid = monotonic
            .checked_duration_since(self.last_poll)
            .is_some_and(|elapsed| elapsed <= VCE_WATCH_OBSERVATION_MAX_GAP);
        let wall_valid = wall
            .duration_since(self.last_wall)
            .is_ok_and(|elapsed| elapsed <= VCE_WATCH_OBSERVATION_MAX_GAP);
        // A clock correction is uncertainty, never proof that files changed or that the OS slept.
        // 时钟校正表示不确定，绝不是文件变化或操作系统休眠的证明。
        let new_gap = (!monotonic_valid || !wall_valid) && !self.gap_reported;
        self.gap_reported |= new_gap;
        if consumer_polled {
            self.last_poll = monotonic;
            self.last_wall = wall;
            self.gap_reported = false;
        }
        VceWatchClockSample {
            live: !self.gap_reported,
            new_gap,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Idle polling without file events remains live; status reads cannot extend that evidence.
    /// 没有文件事件的空闲轮询保持存活；状态读取不能延长此证据。
    /// Takes no input and returns after checking exact timeout and one-time gap reporting.
    /// 无输入；检查精确超时及一次性缺口报告后返回。
    #[test]
    fn status_reads_never_extend_consumer_liveness() {
        // Origin is shared by independent monotonic and wall-clock test samples.
        // 原点由独立单调及墙上时钟测试采样共享。
        let origin = Instant::now();
        let wall = SystemTime::UNIX_EPOCH + Duration::from_secs(100);
        // Clock receives actual idle consumer polls before status-only observations.
        // 时钟先接收实际空闲消费者轮询，再接收仅状态观察。
        let mut clock = VceWatchDeliveryClock::new(origin, wall);
        for seconds in 1..=10 {
            // Elapsed advances both clocks without a source event or indexing operation.
            // elapsed 推进两个时钟，不产生源码事件或索引操作。
            let elapsed = Duration::from_secs(seconds);
            let sample = clock.sample(origin + elapsed, wall + elapsed, true);
            assert!(sample.live);
            assert!(!sample.new_gap);
        }
        assert!(
            clock
                .sample(
                    origin + Duration::from_secs(15),
                    wall + Duration::from_secs(15),
                    false
                )
                .live
        );
        // Expired status reports one gap and remains expired across repeated reads.
        // 过期状态报告一次缺口，并在重复读取中保持过期。
        let expired = clock.sample(
            origin + Duration::from_secs(16),
            wall + Duration::from_secs(16),
            false,
        );
        assert!(!expired.live);
        assert!(expired.new_gap);
        let repeated = clock.sample(
            origin + Duration::from_secs(17),
            wall + Duration::from_secs(17),
            false,
        );
        assert!(!repeated.live);
        assert!(!repeated.new_gap);
        // Only an actual resumed poll renews liveness; it does not acknowledge any source scan.
        // 仅实际恢复轮询续期存活；它不会确认任何源码扫描。
        let resumed = clock.sample(
            origin + Duration::from_secs(18),
            wall + Duration::from_secs(18),
            true,
        );
        assert!(resumed.live);
        assert!(!resumed.new_gap);
        assert!(
            clock
                .sample(
                    origin + Duration::from_secs(24),
                    wall + Duration::from_secs(24),
                    true
                )
                .new_gap
        );
    }

    /// Wall-clock-only gaps and backward corrections must invalidate observation without guessing their cause.
    /// 仅墙上时钟缺口及回拨必须使观察失效，不猜测其原因。
    /// Takes no input; returns after independent forward, backward and monotonic-only gap assertions.
    /// 无输入；独立断言前跳、回拨及仅单调时钟缺口后返回。
    #[test]
    fn either_clock_discontinuity_is_unknown_evidence() {
        // Origin establishes a platform-independent pair of test clocks.
        // origin 建立一对不依赖平台的测试时钟。
        let origin = Instant::now();
        let wall = SystemTime::UNIX_EPOCH + Duration::from_secs(100);
        // Each pair isolates a different broken clock from its still-continuous counterpart.
        // 每对时刻将一个中断时钟与另一个仍连续的时钟隔离。
        for (monotonic, observed_wall) in [
            (
                origin + Duration::from_secs(1),
                wall + Duration::from_secs(60),
            ),
            (
                origin + Duration::from_secs(1),
                wall - Duration::from_secs(1),
            ),
            (
                origin + Duration::from_secs(60),
                wall + Duration::from_secs(1),
            ),
        ] {
            // Clock is fresh for each isolated observation.
            // 每次隔离观察都使用新时钟。
            let mut clock = VceWatchDeliveryClock::new(origin, wall);
            let sample = clock.sample(monotonic, observed_wall, true);
            assert!(sample.new_gap);
            assert!(sample.live);
            assert!(!clock.sample(monotonic, observed_wall, false).new_gap);
        }
    }
}
