//! Project-local source observation, independent from checkpoint query admission.
//! 项目内源码观察，独立于检查点查询准入。

use super::clock::{VceWatchClockSample, VceWatchDeliveryClock};
use std::collections::BTreeMap;
use std::path::Path;
use std::path::PathBuf;
use std::time::{Instant, SystemTime};
use uuid::Uuid;
use vulcan_file_watcher::live::LiveWatchObservationHealth;

/// Sample delivery while holding clock then observations; newly expired evidence invalidates every retained scope.
/// 持有时钟锁再持有观察锁采样投递；新过期证据使每个保留范围失效。
/// clock/observations are service-local authority; polled is true only in the actual event loop.
/// clock/observations 是服务内权威；polled 仅在实际事件循环内为真。
/// Returns liveness evidence without starting work, dropping dirty changes or acknowledging source contents.
/// 返回存活证据，不启动工作、不丢弃脏变化，也不确认源码内容。
pub(super) fn sample_source_delivery(
    clock: &mut VceWatchDeliveryClock,
    observations: &mut BTreeMap<String, VceWatchSourceObservation>,
    polled: bool,
) -> VceWatchClockSample {
    // Time records a consumer observation, never an index timestamp or source-change timestamp.
    // 时间记录消费者观察，绝不是索引时间戳或源码变化时间戳。
    let sample = clock.sample(Instant::now(), SystemTime::now(), polled);
    if sample.new_gap {
        for observation in observations.values_mut() {
            observation.mark_gap();
        }
    }
    sample
}

/// Ephemeral source evidence; absence of pending events alone never proves synchronization.
/// 临时源码证据；单靠没有待处理事件绝不证明已经同步。
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct VceSourceFreshness {
    /// Whether the exact physical registration and both event consumers are live.
    /// 精确物理注册和两个事件消费者是否存活。
    pub(crate) watching: bool,
    /// Whether reported changes remain beyond the last acknowledged source boundary.
    /// 是否存在超出最近确认源码边界的已报告变化。
    pub(crate) pending_changes: bool,
    /// Whether full manifest reconciliation is required after a registration or observation gap.
    /// 注册或观察缺口后是否要求完整清单核对。
    pub(crate) reconciliation_required: bool,
}

impl Default for VceSourceFreshness {
    /// Missing observation is unknown, never a silently successful synchronization.
    /// 缺少观察表示未知，绝不是静默成功同步。
    fn default() -> Self {
        Self {
            watching: false,
            pending_changes: false,
            reconciliation_required: true,
        }
    }
}

/// Exact pre-scan authority retained by one source batch, never reconstructed from a timestamp.
/// 一个源码批次保留的精确扫描前权威，绝不从时间戳重建。
#[derive(Clone, Debug, serde::Serialize)]
pub(crate) struct VceSourceObservationTicket {
    /// Unique registration lifetime, including repeated registrations of the same root.
    /// 唯一注册生命周期，包括同一根的重复注册。
    registration_id: Uuid,
    /// Last reported change included by this source batch.
    /// 本源码批次包含的最近报告变化。
    change_revision: u64,
    /// Local gap sequence, including scheduler observation interruptions.
    /// 本地缺口序号，包含调度观察中断。
    continuity_revision: u64,
    /// Whether this ticket was captured before the required full reconciliation.
    /// 此凭据是否在所需完整核对之前捕获。
    pub(crate) reconciliation_required: bool,
}

impl VceSourceObservationTicket {
    /// Compare only exact registration and continuity; different change counts may be separate queued batches.
    /// 仅比较精确注册和连续性；不同变化计数可能属于不同排队批次。
    /// other is the current pre-scan ticket; returns whether an incremental batch is still authorized.
    /// other 为当前扫描前凭据；返回增量批次是否仍被授权。
    pub(crate) fn same_continuity(&self, other: &Self) -> bool {
        self.registration_id == other.registration_id
            && self.continuity_revision == other.continuity_revision
    }

    /// Combine fully coalesced path sets only within one continuous registration; otherwise require a full scan.
    /// 仅在同一连续注册内合并完整合并路径集，否则要求完整扫描。
    pub(super) fn merge(&self, other: &Self) -> Option<Self> {
        if !self.same_continuity(other) {
            return None;
        }
        Some(Self {
            change_revision: self.change_revision.max(other.change_revision),
            reconciliation_required: self.reconciliation_required || other.reconciliation_required,
            ..self.clone()
        })
    }
}

/// Retain dirty evidence across unwatch while resetting continuity on every new exact registration.
/// 跨取消监听保留脏证据，同时在每次新精确注册时重置连续性。
pub(super) struct VceWatchSourceObservation {
    /// Canonical source root that owns all revisions in this record.
    /// 持有本记录全部修订的规范源码根。
    pub(super) project_root: PathBuf,
    /// Live registration identity; None means observation has explicitly stopped.
    /// 存活注册身份；None 表示观察已明确停止。
    registration_id: Option<Uuid>,
    /// Monotonic delivered source-change boundary.
    /// 单调已投递源码变化边界。
    change_revision: u64,
    /// Highest source boundary covered by a successful exact batch.
    /// 成功精确批次覆盖的最高源码边界。
    acknowledged_revision: u64,
    /// Observation discontinuity boundary, never cleared by a status read.
    /// 观察中断边界，绝不因状态读取清除。
    continuity_revision: u64,
    /// Full reconciliation remains required until a matching successful scan acknowledges it.
    /// 直到匹配成功扫描确认前，始终要求完整核对。
    reconciliation_required: bool,
    /// Last sampled shared backend health; kept separate from successful source reconciliation.
    /// 最近采样的共享后端健康，独立于成功源码核对。
    health: Option<LiveWatchObservationHealth>,
}

impl VceWatchSourceObservation {
    /// Start unknown evidence for root; no source IO, registration or index task is created.
    /// 为 root 开始未知证据；不创建源码输入输出、监听注册或索引任务。
    pub(super) fn new(root: PathBuf) -> Self {
        Self {
            project_root: root,
            registration_id: None,
            change_revision: 0,
            acknowledged_revision: 0,
            continuity_revision: 0,
            reconciliation_required: true,
            health: None,
        }
    }

    /// Admit a fresh registration with sampled health, retaining known changes from the same root.
    /// 使用采样健康准入新注册，保留同一根的已知变化。
    /// Returns nothing; replacement of another root must use a new record instead.
    /// 无返回值；替换另一根必须使用新记录。
    pub(super) fn register(&mut self, health: Option<LiveWatchObservationHealth>) {
        self.registration_id = Some(Uuid::new_v4());
        self.health = health;
        self.mark_gap();
    }

    /// Stop exact observation without erasing unsynchronized changes; takes no input and returns nothing.
    /// 停止精确观察，不擦除未同步变化；无输入或返回值。
    pub(super) fn unwatch(&mut self) {
        self.registration_id = None;
        self.health = None;
        self.mark_gap();
    }

    /// Retain a reported delivery gap; never schedule a scan or alter query availability.
    /// 保留已报告投递缺口；绝不调度扫描或改变查询可用性。
    pub(super) fn mark_gap(&mut self) {
        self.continuity_revision += 1;
        self.reconciliation_required = true;
    }

    /// Record one coalesced source event and return its exact boundary for the queued work.
    /// 记录一次合并源码事件，返回排队工作的精确边界。
    pub(super) fn note_changes(&mut self) -> Option<VceSourceObservationTicket> {
        self.change_revision += 1;
        self.ticket()
    }

    /// Refresh reported backend evidence only; unavailable health or a new gap invalidates prior continuity.
    /// 仅刷新已报告后端证据；健康不可用或新缺口使先前连续性失效。
    /// health belongs to this exact live registration. No return value or source IO.
    /// health 属于本精确存活注册。无返回值或源码输入输出。
    pub(super) fn observe_health(&mut self, health: Option<LiveWatchObservationHealth>) {
        if self.health != health {
            self.mark_gap();
        }
        self.health = health;
    }

    /// Return current diagnostic evidence without asserting that unobserved files are unchanged.
    /// 返回当前诊断证据，不断言未观察文件没有变化。
    pub(super) fn freshness(&self) -> VceSourceFreshness {
        // This conjunction is the declared backend health contract, not an inferred fallback.
        // 此合取条件是已声明后端健康契约，不是推断式兜底。
        let watching = self.registration_id.is_some()
            && self
                .health
                .is_some_and(|health| health.event_loop_running && health.physically_registered);
        VceSourceFreshness {
            watching,
            pending_changes: self.change_revision > self.acknowledged_revision,
            reconciliation_required: self.reconciliation_required || !watching,
        }
    }

    /// Capture one exact boundary when observation is live; returns None without valid evidence.
    /// 观察存活时捕获一个精确边界；缺少有效证据时返回 None。
    pub(super) fn ticket(&self) -> Option<VceSourceObservationTicket> {
        if !self.freshness().watching {
            return None;
        }
        Some(VceSourceObservationTicket {
            registration_id: self.registration_id?,
            change_revision: self.change_revision,
            continuity_revision: self.continuity_revision,
            reconciliation_required: self.reconciliation_required,
        })
    }

    /// Acknowledge a successful scan's exact covered boundary while retaining later source edits.
    /// 确认成功扫描精确覆盖边界，同时保留后续源码编辑。
    /// ticket was captured before scanning; full indicates actual full manifest reconciliation. Returns nothing.
    /// ticket 在扫描前捕获；full 表示实际完整清单核对。无返回值。
    pub(super) fn acknowledge(&mut self, ticket: &VceSourceObservationTicket, full: bool) {
        let Some(current) = self.ticket() else {
            return;
        };
        if !ticket.same_continuity(&current) || (!full && self.reconciliation_required) {
            return;
        }
        self.acknowledged_revision = self.acknowledged_revision.max(ticket.change_revision);
        if full {
            self.reconciliation_required = false;
        }
    }
}

impl super::VceWatcherService {
    /// Sample health and source state under existing registration ownership, without any source or database IO.
    /// 在既有注册归属下采样健康和源码状态，不执行源码或数据库输入输出。
    /// project_id/root select exact authority; inspect reads or acknowledges that state and supplies the return value.
    /// project_id/root 选择精确权威；inspect 读取或确认该状态并提供返回值。
    fn with_source_observation<T>(
        &self,
        project_id: &str,
        root: &Path,
        inspect: impl FnOnce(&mut VceWatchSourceObservation) -> T,
    ) -> Option<T> {
        // Keep registration stable through the observation; poisoned authority produces no evidence.
        // 观察期间保持注册稳定；损坏权威不产生证据。
        let registrations = self.inner.registrations.lock().ok()?;
        // Sampling under the same lock order prevents a resumed poll from hiding an unobserved gap.
        // 按相同锁顺序采样，避免恢复轮询隐藏尚未观察的缺口。
        let mut delivery_clock = self.inner.delivery_clock.lock().ok()?;
        let mut observations = self.inner.observations.lock().ok()?;
        let delivery = sample_source_delivery(&mut delivery_clock, &mut observations, false);
        let observation = observations.get_mut(project_id)?;
        if observation.project_root != root {
            return None;
        }
        // An explicitly stopping service cannot certify continuous delivery even if the OS backend is alive.
        // 明确停止中的服务即使操作系统后端存活，也不能确认连续投递。
        // A surviving physical backend does not prove this consumer is still delivering events.
        // 物理后端存活不能证明本消费者仍在投递事件。
        let consumer_running = self
            .inner
            .event_task
            .lock()
            .ok()?
            .as_ref()
            .is_some_and(|task| !task.is_finished());
        let health = if *self.inner.shutdown_sender.borrow() || !consumer_running || !delivery.live
        {
            None
        } else {
            registrations
                .get(project_id)
                .and_then(|registration| registration.registration.observation_health())
        };
        observation.observe_health(health);
        Some(inspect(observation))
    }

    /// Return lightweight current source evidence for project_id/root; unknown when no exact observation exists.
    /// 返回 project_id/root 的轻量当前源码证据；没有精确观察时为未知。
    pub(crate) fn source_freshness(&self, project_id: &str, root: &Path) -> VceSourceFreshness {
        self.with_source_observation(project_id, root, |observation| observation.freshness())
            .unwrap_or_default()
    }

    /// Capture source authority immediately before a scan; None requires conservative full reconciliation.
    /// 紧接扫描前捕获源码权威；None 要求保守完整核对。
    pub(crate) fn begin_source_reconciliation(
        &self,
        project_id: &str,
        root: &Path,
    ) -> Option<VceSourceObservationTicket> {
        self.with_source_observation(project_id, root, |observation| observation.ticket())
            .flatten()
    }

    /// Acknowledge only a successful exact source scope after publication; failures never call this method.
    /// 仅在发布后确认成功的精确源码范围；失败绝不调用本方法。
    /// project_id/root own ticket; full identifies the actual scan mode. Returns nothing or starts no task.
    /// project_id/root 持有 ticket；full 标识实际扫描模式。不返回值，也不启动任务。
    pub(crate) fn finish_source_reconciliation(
        &self,
        project_id: &str,
        root: &Path,
        ticket: &VceSourceObservationTicket,
        full: bool,
    ) {
        self.with_source_observation(project_id, root, |observation| {
            observation.acknowledge(ticket, full)
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Expired consumer polling invalidates all source scopes but preserves changes and requires a real scan.
    /// 消费者轮询过期使全部源码范围失效，但保留变化并要求真实扫描。
    /// Takes no input; returns after repeated status reads, resumed polling and scan acknowledgement assertions.
    /// 无输入；断言重复状态读取、恢复轮询及扫描确认后返回。
    #[test]
    fn delivery_resume_does_not_acknowledge_sources() {
        // Each source starts with verified continuity and one subsequent delivered edit.
        // 每个源码以已验证连续性及随后一次已投递编辑开始。
        let mut observations = BTreeMap::new();
        let mut old_tickets = BTreeMap::new();
        for project_id in ["project-a", "project-b"] {
            // Source evidence is isolated despite sharing the same consumer clock.
            // 尽管共享同一消费者时钟，源码证据仍隔离。
            let mut observation = observed_source();
            let initial = observation.ticket().expect("initial boundary");
            observation.acknowledge(&initial, true);
            let dirty = observation.note_changes().expect("queued edit");
            observations.insert(project_id.to_owned(), observation);
            old_tickets.insert(project_id.to_owned(), dirty);
        }
        // Old polling evidence models an actual scheduling gap without sleeping or querying files.
        // 旧轮询证据模拟实际调度缺口，不休眠，也不查询文件。
        let elapsed = std::time::Duration::from_secs(60);
        let mut clock =
            VceWatchDeliveryClock::new(Instant::now() - elapsed, SystemTime::now() - elapsed);
        let expired = sample_source_delivery(&mut clock, &mut observations, false);
        assert!(expired.new_gap);
        assert!(!expired.live);
        for (project_id, observation) in &mut observations {
            observation.acknowledge(&old_tickets[project_id], true);
            assert!(observation.freshness().pending_changes);
            assert!(observation.freshness().reconciliation_required);
        }
        // Repeated reads preserve one invalidation, and an actual poll renews liveness but not freshness.
        // 重复读取保留一次失效；实际轮询续期存活而不续期新鲜度。
        assert!(!sample_source_delivery(&mut clock, &mut observations, false).new_gap);
        let resumed = sample_source_delivery(&mut clock, &mut observations, true);
        assert!(resumed.live);
        assert!(!resumed.new_gap);
        for observation in observations.values_mut() {
            assert!(observation.freshness().reconciliation_required);
            assert!(observation.freshness().pending_changes);
            // Successful full scanning alone may acknowledge the new source boundary.
            // 仅成功完整扫描可以确认新的源码边界。
            let reconciled = observation.ticket().expect("resumed boundary");
            observation.acknowledge(&reconciled, true);
            assert!(!observation.freshness().reconciliation_required);
            assert!(!observation.freshness().pending_changes);
        }
    }

    /// Build healthy evidence for one isolated registration; no filesystem or runtime is used.
    /// 为一个隔离注册构造健康证据；不使用文件系统或运行时。
    /// Returns a record requiring its first full reconciliation; takes no input.
    /// 返回需要首次完整核对的记录；无输入。
    fn observed_source() -> VceWatchSourceObservation {
        // Observation belongs to one fixed synthetic project root.
        // 观察属于一个固定的合成项目根。
        let mut observation = VceWatchSourceObservation::new(PathBuf::from("/source"));
        observation.register(Some(LiveWatchObservationHealth {
            event_loop_running: true,
            physically_registered: true,
            loss_revision: 0,
        }));
        observation
    }

    /// Verify registration alone and incremental completion cannot establish initial freshness.
    /// 验证仅注册及增量完成不能建立初始新鲜度。
    /// Takes no input; returns after asserting only a full acknowledgement establishes continuity.
    /// 无输入；断言仅完整确认可建立连续性后返回。
    #[test]
    fn full_reconciliation_is_required_before_incremental_acknowledgement() {
        // Initial observation has live delivery but no verified source manifest.
        // 初始观察有存活投递，但没有已验证源码清单。
        let mut observation = observed_source();
        assert_eq!(
            VceSourceFreshness::default(),
            VceSourceFreshness {
                watching: false,
                pending_changes: false,
                reconciliation_required: true,
            }
        );
        // Ticket freezes the queued source boundary, not the eventual completion time.
        // 凭据冻结排队源码边界，而非最终完成时间。
        let ticket = observation.note_changes().expect("live observation");
        observation.acknowledge(&ticket, false);
        assert!(observation.freshness().pending_changes);
        assert!(observation.freshness().reconciliation_required);
        observation.acknowledge(&ticket, true);
        assert!(!observation.freshness().pending_changes);
        assert!(!observation.freshness().reconciliation_required);
    }

    /// Preserve changes arriving after both full-scan and incremental batch boundaries.
    /// 保留完整扫描及增量批次边界之后到达的变化。
    /// Takes no input; returns after both newer changes survive older completion receipts.
    /// 无输入；确认两次较新变化均在旧完成回执后保留再返回。
    #[test]
    fn completion_never_acknowledges_later_edits() {
        // Observation starts before the first full scan.
        // 观察开始于首次完整扫描之前。
        let mut observation = observed_source();
        // Full receipt covers only the source revision known before scanning.
        // 完整回执仅覆盖扫描前已知源码修订。
        let full = observation.ticket().expect("full boundary");
        observation.note_changes();
        observation.acknowledge(&full, true);
        assert!(observation.freshness().pending_changes);
        assert!(!observation.freshness().reconciliation_required);
        // Queued boundary must not expand to edits seen just before an older batch starts.
        // 排队边界不得扩展为旧批次开始前刚看到的编辑。
        let queued = observation.ticket().expect("queued boundary");
        observation.note_changes();
        observation.acknowledge(&queued, false);
        assert!(observation.freshness().pending_changes);
        // Latest receipt proves the remaining event was actually covered.
        // 最新回执证明剩余事件确实被覆盖。
        let latest = observation.ticket().expect("latest boundary");
        observation.acknowledge(&latest, false);
        assert!(!observation.freshness().pending_changes);
    }

    /// Reject full completions crossing reported gaps, missing health, or a replaced registration.
    /// 拒绝跨越已报告缺口、健康缺失或注册替换的完整完成回执。
    /// Takes no input; returns after preserving dirty state across each invalidation.
    /// 无输入；确认每种失效后均保留脏状态再返回。
    #[test]
    fn continuity_changes_invalidate_old_full_receipts() {
        // Each variant uses a fresh isolated source authority.
        // 每种情况使用新的隔离源码权威。
        for variant in 0..3 {
            // Observation records work before the continuity break.
            // 观察记录连续性中断前的工作。
            let mut observation = observed_source();
            // Old ticket cannot authorize completion in a newer observation scope.
            // 旧凭据不能授权在较新观察范围中完成。
            let ticket = observation.note_changes().expect("old boundary");
            match variant {
                0 => observation.mark_gap(),
                1 => observation.observe_health(None),
                _ => {
                    // Health is deliberately unchanged to verify registration identity, not names.
                    // 故意保持健康不变，以验证注册身份而非名称。
                    let health = observation.health;
                    observation.unwatch();
                    assert!(!observation.freshness().watching);
                    assert!(observation.freshness().pending_changes);
                    observation.register(health);
                }
            }
            observation.acknowledge(&ticket, true);
            assert!(observation.freshness().pending_changes);
            assert!(observation.freshness().reconciliation_required);
        }
    }

    /// Merge retry boundaries only within the same registration and delivery continuity.
    /// 仅在相同注册及投递连续性内合并重试边界。
    /// Takes no input; returns after verifying both merge order and rejected stale scopes.
    /// 无输入；验证合并顺序及陈旧范围被拒绝后返回。
    #[test]
    fn retry_ticket_merge_retains_newest_exact_boundary() {
        // Observation provides monotonic delivered revisions.
        // 观察提供单调投递修订。
        let mut observation = observed_source();
        // First and second tickets represent two coalesced batches.
        // 第一及第二凭据表示两个合并批次。
        let first = observation.note_changes().expect("first boundary");
        let second = observation.note_changes().expect("second boundary");
        assert_eq!(
            first.merge(&second).expect("same scope").change_revision,
            second.change_revision
        );
        assert_eq!(
            second.merge(&first).expect("same scope").change_revision,
            second.change_revision
        );
        observation.mark_gap();
        // New continuity must not be certified by either old batch.
        // 新连续性不得由任一旧批次确认。
        let after_gap = observation.ticket().expect("new scope");
        assert!(second.merge(&after_gap).is_none());
    }
}
