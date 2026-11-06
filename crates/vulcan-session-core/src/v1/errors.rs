//! V1 session domain validation errors.
//! V1 会话领域校验错误。
//!
//! These errors model invariant violations of the in-memory fact stream
//! (ids, seq order, API-group ownership, immutable fields). Storage-level
//! corruption errors live in the workspace store's `session_v1` module.
//! 这些错误建模内存事实流的不变量违反(ids、seq 顺序、API 组归属、不可变字段)。
//! 存储级损坏错误位于工作台存储的 `session_v1` 模块。

use std::fmt;

use super::ids::{JS_SAFE_INTEGER_MAX, LOCAL_ID_MIN};

/// One domain-level invariant violation inside the V1 session model.
/// V1 会话模型内的一条领域级不变量违反。
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DomainError {
    /// A concurrent answer mode write advanced the revision observed by the client.
    /// 并发回答模式写入推进了客户端观察到的修订号。
    AnswerModeConflict {
        /// Revision supplied by the client before its requested write.
        /// 客户端请求写入前提供的修订号。
        expected_revision: u64,
        /// Current authoritative revision; the client must refresh before retrying.
        /// 当前权威修订号；客户端必须刷新后再重试。
        actual_revision: u64,
    },
    /// A tool input or output payload is not a JSON object.
    /// 工具输入或输出载荷不是 JSON 对象。
    InvalidToolPayload {
        /// The observed JSON value kind.
        /// 实际观察到的 JSON 值类型。
        actual_kind: &'static str,
    },
    /// One typed persistent change-set violates its canonical producer contract.
    /// 一份强类型持久化 change-set 违反其规范产出契约。
    InvalidPersistentChangeSet {
        /// Exact logical field location reported by the shared validator.
        /// 共享校验器报告的精确逻辑字段位置。
        location: String,
        /// Concrete canonical contract violation.
        /// 具体的规范契约违例。
        detail: String,
    },
    /// One Provider tool identity required by the durable draft bridge is invalid.
    /// 工具草稿可靠桥接所需的某个 Provider 工具身份非法。
    InvalidToolIdentity {
        /// Invalid identity field name.
        /// 非法身份字段名。
        field: &'static str,
    },
    /// A terminal tool fact is missing its explicit persisted output.
    /// 一条工具终态事实缺少明确持久化的输出。
    MissingTerminalToolOutput {
        /// Targeted tool call id.
        /// 目标工具调用 id。
        tool_call_id: u64,
        /// Terminal status that requires an output.
        /// 要求输出的工具终态。
        status: &'static str,
    },
    /// A session title is empty after whitespace validation.
    /// 会话标题经过空白校验后为空。
    InvalidSessionTitle,
    /// A user input contains neither non-whitespace text nor an attached resource.
    /// 一条用户输入既没有非空白文本，也没有附件资源。
    EmptyUserInput,
    /// An external client action id is empty, padded, or too large.
    /// 一个外部客户端动作 id 为空、带首尾空白或过长。
    InvalidClientActionId {
        /// Maximum accepted UTF-8 byte length.
        /// 允许的最大 UTF-8 字节长度。
        max_bytes: usize,
    },
    /// One external client action id was reused with different durable semantics.
    /// 一个外部客户端动作 id 被复用于不同的持久化语义。
    ClientActionConflict {
        /// Exact conflicting external action id.
        /// 发生冲突的精确外部动作 id。
        action_id: String,
    },
    /// The open turn is not at a terminal boundary that permits continuation.
    /// 开放回合未处于允许继续的终态边界。
    ContinuationNotEligible {
        /// Stable machine-readable rejection reason.
        /// 稳定的机器可读拒绝原因。
        reason: &'static str,
    },
    /// Persisted continuation facts cannot reconstruct one exact receipt.
    /// 持久化的继续事实无法重建一个精确 receipt。
    InvalidContinuationHistory {
        /// Invalid continuation history invariant.
        /// 非法的继续历史不变量。
        field: &'static str,
    },
    /// One request-configuration field cannot be represented durably.
    /// 一个请求配置字段无法被稳定持久化表达。
    InvalidRequestConfiguration {
        /// Invalid request field name.
        /// 非法请求字段名。
        field: &'static str,
    },
    /// One session-selection field cannot be represented durably.
    /// 一个会话选择字段无法被稳定持久化表达。
    InvalidSelectionConfiguration {
        /// Invalid selection field name.
        /// 非法选择字段名。
        field: &'static str,
    },
    /// A persisted Unix-millisecond timestamp is outside the accepted range.
    /// 一个持久化 Unix 毫秒时间戳超出允许范围。
    InvalidTimestamp {
        /// Timestamp field name.
        /// 时间戳字段名。
        field: &'static str,
        /// Rejected timestamp value.
        /// 被拒绝的时间戳值。
        value: i64,
    },
    /// A physical API was requested before the runtime baseline was initialized.
    /// 在运行时基线初始化前请求了物理 API。
    RuntimeBaselineUninitialized,
    /// A second baseline initialization disagrees with the durable initialized baseline.
    /// 第二次基线初始化与已持久化初始化基线不一致。
    RuntimeBaselineAlreadyInitialized,
    /// A persisted fresh runtime carries facts that are legal only after initialization.
    /// 一个持久化 Fresh runtime 携带了仅在初始化后才合法的事实。
    InvalidRuntimeInitializationState {
        /// Conflicting runtime field name.
        /// 冲突的运行时字段名。
        field: &'static str,
    },
    /// One locked baseline segment has an invalid or ambiguous identity/order contract.
    /// 一条锁定基线片段具有无效或有歧义的身份/顺序契约。
    InvalidLockedSource {
        /// Invalid locked-source field or cross-field invariant.
        /// 无效的锁定来源字段或跨字段不变量。
        field: &'static str,
    },
    /// The immutable Provider projection proof disagrees with the current document boundary.
    /// 不可变 Provider 投影证明与当前文档边界不一致。
    InvalidProviderProjection {
        /// Invalid proof field or cross-field invariant.
        /// 无效的证明字段或跨字段不变量。
        field: &'static str,
    },
    /// One persisted Provider request-budget evidence value is malformed or inconsistent.
    /// 一份持久化 Provider 请求预算证据格式错误或不一致。
    InvalidProviderBudgetEvidence {
        /// Invalid evidence field or cross-field invariant.
        /// 非法证据字段或跨字段不变量。
        field: &'static str,
    },
    /// One physical API terminal fact has an impossible status, reason, error, or timing shape.
    /// 一条物理 API 终态事实具有不可能的状态、原因、错误或时间结构。
    InvalidApiTerminalFacts {
        /// Invalid terminal field or cross-field invariant.
        /// 非法的终态字段或跨字段不变量。
        field: &'static str,
    },
    /// One persisted statistic or subagent-return fact violates its typed contract.
    /// 一条持久化统计或子代理回流事实违反其强类型契约。
    InvalidStatistics {
        /// Invalid statistic or return field.
        /// 非法统计或回流字段。
        field: &'static str,
    },
    /// Answer code snapshots exceed their bounds or attach outside the terminal seal.
    /// 回答代码快照超出边界，或在终态封口之外附着。
    InvalidAnswerCodeSnapshots {
        /// Invalid snapshot field or cross-field invariant.
        /// 非法快照字段或跨字段不变量。
        field: &'static str,
    },
    /// A subagent return was built from child state that is not fully durable.
    /// 子代理回流基于尚未完全持久化的子会话状态构建。
    SubagentSourceNotDurable {
        /// Child in-memory revision.
        /// 子会话内存修订号。
        memory_revision: u64,
        /// Child durable revision.
        /// 子会话持久化修订号。
        durable_revision: u64,
    },
    /// A history generation was requested from an in-memory document that is not fully durable.
    /// 从尚未完全持久化的内存文档请求了历史 generation。
    GenerationSourceNotDurable {
        /// Current in-memory document revision.
        /// 当前内存文档修订号。
        memory_revision: u64,
        /// Last durably published document revision.
        /// 最近一次可靠发布的文档修订号。
        durable_revision: u64,
    },
    /// A history generation requires the complete stream but the document contains only a tail window.
    /// 历史 generation 需要完整事件流，但当前文档只包含尾部窗口。
    GenerationRequiresFullHistory {
        /// First resident stream seq proving that the prefix is absent.
        /// 证明前缀缺失的首个常驻流 seq。
        resident_start_seq: u64,
    },
    /// A history generation input violates one explicit domain invariant.
    /// 历史 generation 输入违反了一条明确领域不变量。
    InvalidGenerationInput {
        /// Stable machine-readable invalid field or invariant name.
        /// 稳定、机器可读的非法字段或不变量名称。
        field: &'static str,
    },
    /// A history generation was requested while one physical API was still active.
    /// 在一次物理 API 仍活动时请求了历史 generation。
    GenerationWhileApiActive {
        /// Active physical API identity.
        /// 活动物理 API 身份。
        api_call_id: u64,
    },
    /// One id is outside the legal `1..=2^53-1` JSON-safe range.
    /// 某个 id 超出合法的 `1..=2^53-1` JSON 安全范围。
    InvalidId {
        /// The entity kind owning the id.
        /// 拥有该 id 的实体种类。
        entity: &'static str,
        /// The offending raw value.
        /// 违规的原始值。
        value: u64,
        /// Inclusive lower boundary.
        /// 包含下界。
        min: u64,
        /// Inclusive upper boundary.
        /// 包含上界。
        max: u64,
    },
    /// A reference targets a missing or not-yet-created entity.
    /// 引用指向不存在或尚未创建的实体。
    ReferenceViolation {
        /// Entity kind of the missing reference.
        /// 缺失引用的实体种类。
        kind: &'static str,
        /// Referenced raw id value.
        /// 被引用的原始 id 值。
        value: u64,
    },
    /// A mutation violates the strictly contiguous seq invariant.
    /// 某次修改违反严格连续 seq 不变量。
    SequenceViolation {
        /// The expected next seq value.
        /// 期望的下一个 seq 值。
        expected: u64,
        /// The observed next seq value.
        /// 观察到的下一个 seq 值。
        actual: u64,
    },
    /// A caller attempted to change a frozen immutable field.
    /// 调用方尝试修改一个已冻结的不可变字段。
    ImmutableFieldMutation {
        /// Entity kind holding the field.
        /// 持有该字段的实体种类。
        entity: &'static str,
        /// Immutable field name.
        /// 不可变字段名。
        field: &'static str,
    },
    /// No supported persisted fingerprint contract matches this checkpoint.
    /// 没有受支持的持久化指纹契约匹配此检查点。
    ProviderSourceFingerprintMismatch {
        /// Checkpoint hosting sequence.
        /// 检查点承载事件序号。
        seq: u64,
        /// Digest read from storage.
        /// 从存储读取的摘要。
        stored: String,
        /// Digest under the stable immutable projection contract.
        /// 稳定不可变投影契约下的摘要。
        expected: String,
    },
    /// An update targeted an event belonging to another API group.
    /// 某次更新指向了属于其他 API 组的既有事件。
    CrossApiGroupUpdate {
        /// Seq of the targeted event.
        /// 被更新事件的 seq。
        event_seq: u64,
        /// API call id of the targeted event.
        /// 被更新事件的 api_call_id。
        api_call_id: u64,
    },
    /// A tool lifecycle mutation started from an unexpected persisted status.
    /// 工具生命周期修改从非预期的持久化状态开始。
    ToolStatusMismatch {
        /// Targeted tool call id.
        /// 目标工具调用 id。
        tool_call_id: u64,
        /// Required source status.
        /// 要求的源状态。
        expected: &'static str,
        /// Persisted source status actually found.
        /// 实际找到的持久化源状态。
        actual: &'static str,
    },
    /// A completed API group already carries a `provider_source`.
    /// 已完成的 API 组已经携带了 `provider_source`。
    ProviderSourceAlreadySet {
        /// API call id owning the group.
        /// 拥有该组的 api_call_id。
        api_call_id: u64,
    },
    /// A provider source is not on the current final Provider event of its API group.
    /// Provider 来源没有位于所属 API 组当前最后一条 Provider 事件上。
    ProviderSourcePlacement {
        /// API call id owning the group.
        /// 所属 API 调用 id。
        api_call_id: u64,
        /// Seq that currently carries the source.
        /// 当前承载来源的 seq。
        source_seq: u64,
        /// Expected final Provider seq, when the group has Provider output.
        /// 预期的最后一条 Provider seq（组存在 Provider 输出时）。
        expected_seq: Option<u64>,
        /// Whether the API group already has its terminal statistics event.
        /// API 组是否已经具有终态统计事件。
        api_closed: bool,
    },
    /// Terminal API metadata differs from the immutable start snapshot.
    /// API 终态元数据与不可变起始快照不一致。
    ApiRequestMismatch {
        /// Mismatched request field.
        /// 不一致的请求字段。
        field: &'static str,
        /// Value frozen at API start.
        /// API 开始时冻结的值。
        expected: String,
        /// Value supplied at API completion.
        /// API 完成时提供的值。
        actual: String,
    },
    /// A Goal objective is empty or exceeds the frozen domain limit.
    /// Goal 目标为空或超出已冻结的领域上限。
    InvalidGoalObjective {
        /// Maximum accepted Unicode scalar count.
        /// 允许的 Unicode 标量值最大数量。
        max_chars: usize,
    },
    /// A Goal token budget was explicitly set to zero.
    /// Goal Token 预算被显式设置为零。
    InvalidGoalBudget,
    /// A Goal status transition omitted its stable display reason code.
    /// Goal 状态转换缺少稳定的显示原因码。
    InvalidGoalReasonCode,
    /// A Goal transition requires an enabled or disabled state different from the current one.
    /// Goal 转换要求的启用状态与当前状态不同。
    GoalEnablementMismatch {
        /// Required enablement before the transition.
        /// 转换前要求的启用状态。
        expected_enabled: bool,
        /// Actual enablement found in metadata.
        /// metadata 中实际的启用状态。
        actual_enabled: bool,
    },
    /// A Goal lifecycle transition starts from an unsupported state.
    /// Goal 生命周期转换从不受支持的状态开始。
    GoalStateTransition {
        /// Human-readable current state name.
        /// 人类可读的当前状态名。
        current: &'static str,
        /// Human-readable requested state name.
        /// 人类可读的请求状态名。
        requested: &'static str,
    },
    /// Mutually exclusive session modes would both become enabled.
    /// 互斥的会话模式将同时被启用。
    ModeConflict {
        /// Mode requested by the caller.
        /// 调用方请求的模式。
        requested: &'static str,
        /// Already active mutually exclusive mode.
        /// 已激活的互斥模式。
        active: &'static str,
    },
    /// A mode transition was requested while one physical API is active.
    /// 在一次物理 API 仍活动时请求了模式转换。
    ModeTransitionWhileApiActive {
        /// Active physical API identity.
        /// 活动物理 API 身份。
        api_call_id: u64,
    },
    /// Pending in-turn inputs cannot promote while an API group is open.
    /// API 组仍然开放时不能迁移待处理过程内补充。
    PromoteWhileApiOpen,
    /// The session has no open turn for the requested action.
    /// 会话没有可供请求动作使用的开放回合。
    NoActiveTurn,
    /// The turn has no open API for the requested action.
    /// 回合没有可供请求动作使用的开放 API。
    NoActiveApi,
    /// A turn completion was requested while one physical API is still active.
    /// 在一次物理 API 仍活动时请求完成回合。
    TurnCompletionWhileApiActive {
        /// Open turn identity.
        /// 开放回合身份。
        turn_id: u64,
        /// Active physical API identity.
        /// 活动物理 API 身份。
        api_call_id: u64,
    },
    /// A turn close/continue action targets an already closed turn.
    /// 回合关闭/继续动作指向一个已经关闭的回合。
    TurnAlreadyClosed {
        /// The closed turn id.
        /// 已关闭的回合 id。
        turn_id: u64,
    },
    /// A turn is still open when a terminal action requires closure.
    /// 需要闭合的动作遇到仍然开放的回合。
    TurnNotClosed {
        /// The still-open turn id.
        /// 仍然开放的回合 id。
        turn_id: u64,
    },
    /// A goal-continuation turn is already active and must not be recreated.
    /// 已经存在一个 goal-continuation 回合，不得再次创建。
    GoalContinuationAlreadyActive {
        /// The already-active goal turn id.
        /// 已经活跃的 goal 回合 id。
        turn_id: u64,
    },
    /// A turn-scoped mutation targets a different open turn.
    /// 回合作用域修改指向了不同的开放回合。
    TurnIdentityMismatch {
        /// Turn id the caller intended to mutate.
        /// 调用方准备修改的回合 id。
        expected: u64,
        /// Turn id that is currently open.
        /// 当前实际开放的回合 id。
        actual: u64,
    },
    /// A completed turn lacks one final assistant semantic.
    /// 准备完成的回合缺少一条最终 assistant 语义。
    TurnMissingFinalAssistant {
        /// Turn that cannot be completed.
        /// 无法完成的回合。
        turn_id: u64,
    },
    /// A root input requires no open turn; queued inputs must wait for closure.
    /// 根输入要求当前没有开放回合；排队输入必须等待闭合。
    RootInputWhileTurnOpen {
        /// The still-open turn id.
        /// 仍然开放的回合 id。
        turn_id: u64,
    },
    /// The input queue is empty when a scheduled dispatch was requested.
    /// 请求调度排队输入时队列为空。
    QueueEmpty,
    /// One exact queued-input identity does not exist in authoritative metadata.
    /// 一个精确排队输入身份不存在于权威元数据中。
    QueuedInputNotFound {
        /// Missing session-local pending-input identity.
        /// 缺失的会话本地待处理输入身份。
        input_id: u64,
    },
    /// A requested complete queue order does not match the authoritative queue membership.
    /// 请求的完整队列顺序与权威队列成员不一致。
    InvalidQueuedInputOrder {
        /// Stable machine-readable invalid-order field.
        /// 稳定的机器可读非法顺序字段。
        field: &'static str,
    },
    /// Transient streaming entities cannot be sealed while still streaming.
    /// 流式实体仍在流式输出时不能被封口。
    EntitySealedWhileActive {
        /// Entity kind that is still active.
        /// 仍然活跃的实体种类。
        entity: &'static str,
        /// Seq of the active entity event.
        /// 活跃实体事件的 seq。
        seq: u64,
    },
    /// A durable confirmation targeted a revision the document never issued as
    /// a flush candidate; future or forged revisions are unusable.
    /// 持久化确认指向了文档从未作为 flush 候选发出的修订号；未来或被伪造的
    /// 修订号不可用。
    RevisionNeverIssued {
        /// The rejected revision.
        /// 被拒绝的修订号。
        revision: u64,
        /// The current memory revision at rejection time.
        /// 拒绝时当前的 memory 修订号。
        memory_revision: u64,
        /// The last revision already confirmed durable.
        /// 已经确认持久化的修订号。
        durable_revision: u64,
    },
    /// Rebuild found more than one turn without a closure boundary.
    /// 重建发现超过一个没有闭合边界的回合。
    MultipleOpenTurns,
    /// Rebuild found an API group whose events span more than one turn.
    /// 重建发现一个 API 组的事件横跨多个回合。
    GroupSpansTurns {
        /// The api call id spanning multiple turns.
        /// 横跨多个回合的 api call id。
        api_call_id: u64,
    },
    /// Rebuild found events of an already-closed API group (group reopened).
    /// 重建发现已闭合 API 组的事件再次出现（组被重新打开）。
    ClosedGroupReopened {
        /// The api call id of the reopened group.
        /// 被重新打开组的 api call id。
        api_call_id: u64,
    },
    /// Rebuild found two open API groups interleaved in the stream.
    /// 重建发现两个开放 API 组在流中交错。
    GroupsInterleaved {
        /// The earlier still-open group.
        /// 较早且仍开放的组。
        first: u64,
        /// The later group.
        /// 较晚出现的组。
        second: u64,
    },
    /// Rebuild found a duplicate or mis-ordered turn closure sequence.
    /// 重建发现重复或顺序错误的回合闭合序列。
    TurnClosureMismatch {
        /// The offending turn id.
        /// 违规的回合 id。
        turn_id: u64,
    },
    /// A terminal settlement intent targets a different session than its document.
    /// 一条终态结算意图指向的会话与其文档不同。
    TerminalSettlementSessionMismatch {
        /// Exact session id owned by the document.
        /// 文档持有的精确会话 id。
        session_id: String,
    },
    /// One exact terminal settlement record is missing from the document.
    /// 文档中缺失一条精确终态结算记录。
    TerminalSettlementMissing {
        /// Requested settlement id.
        /// 请求的结算身份。
        settlement_id: String,
    },
    /// A different unfinished terminal settlement already owns the session.
    /// 另一条尚未完成的终态结算已经持有该会话。
    TerminalSettlementConflict {
        /// Existing unfinished settlement id.
        /// 既有未完成结算身份。
        settlement_id: String,
    },
    /// A collaboration record or operation was not found in authoritative state.
    /// 权威协作状态中不存在目标记录或操作。
    CollaborationNotFound {
        /// Stable collaboration entity kind.
        /// 稳定协作实体类型。
        kind: &'static str,
        /// Exact requested identity.
        /// 精确请求身份。
        identity: String,
    },
    /// A collaboration operation reused an identity with different semantics.
    /// 协作操作以不同语义复用了同一身份。
    CollaborationConflict {
        /// Exact conflicting operation identity.
        /// 发生冲突的精确操作身份。
        operation_id: String,
    },
    /// A collaboration caller is not the participant authorized for the action.
    /// 协作调用者不是该动作获授权的参与者。
    CollaborationUnauthorized {
        /// Stable action name.
        /// 稳定动作名称。
        action: &'static str,
        /// Exact question or delivery identity.
        /// 精确问题或投递身份。
        identity: String,
    },
    /// A question has already been settled by an earlier operation.
    /// 问题已经由更早操作完成结算。
    CollaborationAlreadySettled {
        /// Question identity.
        /// 问题身份。
        question_id: String,
        /// Winning state name.
        /// 胜出状态名称。
        state: &'static str,
    },
    /// A collaboration state transition violates its explicit lifecycle.
    /// 协作状态转换违反明确生命周期。
    CollaborationStateViolation {
        /// Stable field or transition name.
        /// 稳定字段或转换名称。
        field: &'static str,
    },
    /// A dedicated-session profile violates its prompt bound or span ordering.
    /// 专职会话配置违反提示词边界或区间顺序。
    InvalidDedicatedProfile {
        /// Stable violated field name.
        /// 被违反的稳定字段名。
        field: &'static str,
    },
    /// A dedicated-only operation was requested on an ordinary session.
    /// 在普通会话上请求了仅限专职会话的操作。
    NotDedicatedSession,
    /// A creation-frozen dedicated-session setting was asked to change.
    /// 请求修改专职会话创建时冻结的设置。
    DedicatedSettingFrozen {
        /// Stable setting name.
        /// 稳定设置名。
        setting: &'static str,
    },
    /// The dedicated memory lock can change only between turns.
    /// 专职会话的记忆锁定只能在回合之间切换。
    DedicatedMemoryLockWhileTurnOpen {
        /// Open turn identity.
        /// 开放回合身份。
        turn_id: u64,
    },
}

impl DomainError {
    /// Builds one out-of-range id error.
    /// 构建一条 id 越界错误。
    pub fn invalid_id(entity: &'static str, value: u64) -> Self {
        Self::InvalidId {
            entity,
            value,
            min: LOCAL_ID_MIN,
            max: JS_SAFE_INTEGER_MAX,
        }
    }

    /// Builds one reference violation.
    /// 构建一条引用违反错误。
    pub fn reference(kind: &'static str, value: u64) -> Self {
        Self::ReferenceViolation { kind, value }
    }
}

impl fmt::Display for DomainError {
    /// Formats the domain error for diagnostics.
    /// 为诊断格式化领域错误。
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::AnswerModeConflict {
                expected_revision,
                actual_revision,
            } => write!(
                formatter,
                "answer mode revision conflict: expected {expected_revision}, current {actual_revision}"
            ),
            Self::InvalidToolPayload { actual_kind } => write!(
                formatter,
                "tool payload must be a JSON object, found {actual_kind}"
            ),
            Self::InvalidPersistentChangeSet { location, detail } => write!(
                formatter,
                "persistent change-set field `{location}` is invalid: {detail}"
            ),
            Self::InvalidToolIdentity { field } => {
                write!(formatter, "provider tool identity field {field} is invalid")
            }
            Self::MissingTerminalToolOutput {
                tool_call_id,
                status,
            } => write!(
                formatter,
                "terminal tool {tool_call_id} with status {status} is missing its persisted output"
            ),
            Self::InvalidSessionTitle => formatter.write_str("session title must not be empty"),
            Self::EmptyUserInput => {
                formatter.write_str("user input must contain text or at least one resource")
            }
            Self::InvalidClientActionId { max_bytes } => write!(
                formatter,
                "client action id must be exact, non-empty, and at most {max_bytes} UTF-8 bytes"
            ),
            Self::ClientActionConflict { action_id } => write!(
                formatter,
                "client action id `{action_id}` was already persisted with different semantics"
            ),
            Self::ContinuationNotEligible { reason } => {
                write!(formatter, "open turn is not eligible to continue: {reason}")
            }
            Self::InvalidContinuationHistory { field } => {
                write!(
                    formatter,
                    "persisted continuation history is invalid at {field}"
                )
            }
            Self::InvalidRequestConfiguration { field } => {
                write!(formatter, "request configuration field {field} is invalid")
            }
            Self::InvalidSelectionConfiguration { field } => {
                write!(
                    formatter,
                    "selection configuration field {field} is invalid"
                )
            }
            Self::InvalidTimestamp { field, value } => {
                write!(formatter, "timestamp {field} has invalid value {value}")
            }
            Self::RuntimeBaselineUninitialized => {
                formatter.write_str("runtime baseline is not initialized")
            }
            Self::RuntimeBaselineAlreadyInitialized => {
                formatter.write_str("runtime baseline is already initialized with different data")
            }
            Self::InvalidRuntimeInitializationState { field } => write!(
                formatter,
                "fresh runtime baseline contains initialized-only field {field}"
            ),
            Self::InvalidLockedSource { field } => {
                write!(formatter, "invalid locked baseline source field {field}")
            }
            Self::InvalidProviderProjection { field } => {
                write!(formatter, "invalid Provider projection proof field {field}")
            }
            Self::InvalidProviderBudgetEvidence { field } => {
                write!(
                    formatter,
                    "invalid Provider request budget evidence field {field}"
                )
            }
            Self::InvalidApiTerminalFacts { field } => {
                write!(
                    formatter,
                    "invalid physical API terminal fact field {field}"
                )
            }
            Self::InvalidStatistics { field } => {
                write!(formatter, "invalid persisted statistic field {field}")
            }
            Self::InvalidAnswerCodeSnapshots { field } => {
                write!(formatter, "invalid answer code snapshot field {field}")
            }
            Self::SubagentSourceNotDurable {
                memory_revision,
                durable_revision,
            } => write!(
                formatter,
                "subagent source document is not fully durable (memory {memory_revision}, durable {durable_revision})"
            ),
            Self::GenerationSourceNotDurable {
                memory_revision,
                durable_revision,
            } => write!(
                formatter,
                "history generation source is not fully durable (memory {memory_revision}, durable {durable_revision})"
            ),
            Self::GenerationRequiresFullHistory { resident_start_seq } => write!(
                formatter,
                "history generation requires a complete stream starting at seq 1, but resident history starts at {resident_start_seq}"
            ),
            Self::InvalidGenerationInput { field } => {
                write!(formatter, "history generation input is invalid at {field}")
            }
            Self::GenerationWhileApiActive { api_call_id } => write!(
                formatter,
                "history generation cannot be built while api call {api_call_id} is active"
            ),
            Self::InvalidId {
                entity,
                value,
                min,
                max,
            } => write!(
                formatter,
                "{entity} id {value} is outside the JSON-safe range {min}..={max}"
            ),
            Self::ReferenceViolation { kind, value } => {
                write!(formatter, "reference to missing {kind} id {value}")
            }
            Self::SequenceViolation { expected, actual } => {
                write!(formatter, "expected next seq {expected} but found {actual}")
            }
            Self::ImmutableFieldMutation { entity, field } => {
                write!(formatter, "{entity}.{field} is immutable")
            }
            Self::ProviderSourceFingerprintMismatch {
                seq,
                stored,
                expected,
            } => write!(
                formatter,
                "provider_source fingerprint mismatch at seq {seq}: stored={stored}, immutable_projection_v1={expected}; no supported fingerprint format matched; automatic rewriting refused"
            ),
            Self::CrossApiGroupUpdate {
                event_seq,
                api_call_id,
            } => write!(
                formatter,
                "event seq {event_seq} belongs to api_call_id {api_call_id} and cannot be updated cross-group"
            ),
            Self::ToolStatusMismatch {
                tool_call_id,
                expected,
                actual,
            } => write!(
                formatter,
                "tool {tool_call_id} requires status {expected} but found {actual}"
            ),
            Self::ProviderSourceAlreadySet { api_call_id } => write!(
                formatter,
                "api group {api_call_id} already carries a provider_source"
            ),
            Self::ProviderSourcePlacement {
                api_call_id,
                source_seq,
                expected_seq,
                api_closed,
            } => write!(
                formatter,
                "api group {api_call_id} provider_source at seq {source_seq} must be on current final Provider seq {expected_seq:?} (closed={api_closed})"
            ),
            Self::ApiRequestMismatch {
                field,
                expected,
                actual,
            } => write!(
                formatter,
                "api request field {field} changed from `{expected}` to `{actual}` within one call"
            ),
            Self::InvalidGoalObjective { max_chars } => write!(
                formatter,
                "goal objective must be non-empty and at most {max_chars} characters"
            ),
            Self::InvalidGoalBudget => {
                formatter.write_str("goal token budget must be greater than zero")
            }
            Self::InvalidGoalReasonCode => {
                formatter.write_str("goal status reason code must not be empty")
            }
            Self::GoalEnablementMismatch {
                expected_enabled,
                actual_enabled,
            } => write!(
                formatter,
                "goal enabled state must be {expected_enabled} but is {actual_enabled}"
            ),
            Self::GoalStateTransition { current, requested } => write!(
                formatter,
                "goal cannot transition from {current} to {requested}"
            ),
            Self::InvalidDedicatedProfile { field } => {
                write!(
                    formatter,
                    "dedicated session profile field {field} is invalid"
                )
            }
            Self::NotDedicatedSession => {
                formatter.write_str("operation requires a dedicated session")
            }
            Self::DedicatedSettingFrozen { setting } => write!(
                formatter,
                "dedicated session setting {setting} is fixed at creation"
            ),
            Self::DedicatedMemoryLockWhileTurnOpen { turn_id } => write!(
                formatter,
                "dedicated memory lock cannot change while turn {turn_id} is open"
            ),
            Self::ModeConflict { requested, active } => write!(
                formatter,
                "session mode {requested} conflicts with active mode {active}"
            ),
            Self::ModeTransitionWhileApiActive { api_call_id } => write!(
                formatter,
                "session mode cannot change while api call {api_call_id} is active"
            ),
            Self::PromoteWhileApiOpen => {
                formatter.write_str("promotion requires the api group to be closed")
            }
            Self::NoActiveTurn => formatter.write_str("no active turn"),
            Self::NoActiveApi => formatter.write_str("no active api"),
            Self::TurnCompletionWhileApiActive {
                turn_id,
                api_call_id,
            } => write!(
                formatter,
                "turn {turn_id} cannot complete while api call {api_call_id} is active"
            ),
            Self::TurnAlreadyClosed { turn_id } => {
                write!(formatter, "turn {turn_id} is already closed")
            }
            Self::TurnNotClosed { turn_id } => {
                write!(formatter, "turn {turn_id} is still open")
            }
            Self::GoalContinuationAlreadyActive { turn_id } => {
                write!(
                    formatter,
                    "goal-continuation turn {turn_id} is already active"
                )
            }
            Self::TurnIdentityMismatch { expected, actual } => write!(
                formatter,
                "turn-scoped mutation expected open turn {expected} but found {actual}"
            ),
            Self::TurnMissingFinalAssistant { turn_id } => write!(
                formatter,
                "turn {turn_id} cannot complete without a final assistant semantic"
            ),
            Self::RootInputWhileTurnOpen { turn_id } => {
                write!(
                    formatter,
                    "root input rejected while turn {turn_id} is open"
                )
            }
            Self::QueueEmpty => formatter.write_str("the queued-input list is empty"),
            Self::QueuedInputNotFound { input_id } => {
                write!(formatter, "queued input {input_id} was not found")
            }
            Self::InvalidQueuedInputOrder { field } => {
                write!(formatter, "queued-input order is invalid at {field}")
            }
            Self::EntitySealedWhileActive { entity, seq } => write!(
                formatter,
                "{entity} at seq {seq} is still active and cannot be sealed"
            ),
            Self::RevisionNeverIssued {
                revision,
                memory_revision,
                durable_revision,
            } => write!(
                formatter,
                "revision {revision} was never issued as a flush candidate \
                 (memory {memory_revision}, durable {durable_revision})"
            ),
            Self::MultipleOpenTurns => {
                formatter.write_str("more than one turn lacks a closure boundary")
            }
            Self::GroupSpansTurns { api_call_id } => write!(
                formatter,
                "api group {api_call_id} spans events of more than one turn"
            ),
            Self::ClosedGroupReopened { api_call_id } => write!(
                formatter,
                "api group {api_call_id} has events after its terminal api_stats"
            ),
            Self::GroupsInterleaved { first, second } => write!(
                formatter,
                "open api groups {first} and {second} interleave in the stream"
            ),
            Self::TurnClosureMismatch { turn_id } => write!(
                formatter,
                "turn {turn_id} has a duplicate or mis-ordered closure sequence"
            ),
            Self::TerminalSettlementSessionMismatch { session_id } => write!(
                formatter,
                "terminal settlement intent targets a different session than `{session_id}`"
            ),
            Self::TerminalSettlementMissing { settlement_id } => {
                write!(
                    formatter,
                    "terminal settlement `{settlement_id}` is missing"
                )
            }
            Self::TerminalSettlementConflict { settlement_id } => write!(
                formatter,
                "terminal settlement `{settlement_id}` is still unfinished"
            ),
            Self::CollaborationNotFound { kind, identity } => {
                write!(formatter, "collaboration {kind} `{identity}` was not found")
            }
            Self::CollaborationConflict { operation_id } => write!(
                formatter,
                "collaboration operation `{operation_id}` was already persisted with different semantics"
            ),
            Self::CollaborationUnauthorized { action, identity } => write!(
                formatter,
                "collaboration action `{action}` is not authorized for `{identity}`"
            ),
            Self::CollaborationAlreadySettled { question_id, state } => write!(
                formatter,
                "collaboration question `{question_id}` is already settled as {state}"
            ),
            Self::CollaborationStateViolation { field } => write!(
                formatter,
                "collaboration state transition is invalid at {field}"
            ),
        }
    }
}

impl std::error::Error for DomainError {}
