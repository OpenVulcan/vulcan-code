//! Durable main-session Hook identity bound to a canonical business turn.
//! 绑定规范业务回合的可靠主会话 Hook 身份。

use super::{DomainError, JS_SAFE_INTEGER_MAX, Seq, TurnId};
use serde::{Deserialize, Serialize};
use std::collections::BTreeSet;

/// Published owner of a business turn whose first Hook has never been committed.
/// 首个 Hook 尚未提交的业务回合的已发布所有者。
/// This transient proof is revalidated under host admission and is never a historical compatibility format.
/// 此临时凭据必须在宿主准入内重验，绝不是历史兼容格式。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct InitialBusinessPreparation {
    /// Exact physical owner of the canonical input.
    /// 规范输入的精确物理所有者。
    pub session_id: String,
    /// Event-tree generation that owns the original input.
    /// 持有原输入的事件树代次。
    pub segment_generation: u64,
    /// Published revision captured with the full original history.
    /// 与完整原历史一起捕获的已发布修订。
    pub revision: u64,
    /// Original admitted business turn, never newly allocated during recovery.
    /// 原已准入业务回合，恢复时绝不重新分配。
    pub turn_id: TurnId,
    /// Exact canonical event that opened this business turn.
    /// 开启此业务回合的精确规范事件。
    pub start_seq: Seq,
    /// Domain origin keeps human, agent-task and other business sources distinct.
    /// 领域来源区分人类、代理任务及其他业务来源。
    pub origin: super::document::TurnOrigin,
}

/// Required current-contract container; an unprepared session has no binding.
/// 当前契约必需容器；尚未准备的会话没有绑定。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct BusinessHookMetadata {
    /// Exact contract version; historical formats are not inferred.
    /// 精确契约版本；不推断历史格式。
    pub version: u32,
    /// Most recently admitted business Hook, retained after completion for exact replay checks.
    /// 最近准入的业务 Hook，完成后保留以检查精确重放。
    pub binding: Option<BusinessHookBinding>,
    /// Ordered human input observations for the latest business turn, including admission before Hook preparation.
    /// 最近业务回合的有序人类输入观察，包括 Hook 准备前的准入。
    pub observed_inputs: Vec<BusinessHookObservedInput>,
}

impl Default for BusinessHookMetadata {
    /// Creates the empty current-contract container without executable ownership.
    /// 创建不含可执行归属的空当前契约容器。
    fn default() -> Self {
        Self {
            version: 1,
            binding: None,
            observed_inputs: Vec::new(),
        }
    }
}

/// One immutable user input observed while the bound business turn was active.
/// 绑定业务回合活动期间观察到的一条不可变用户输入。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct BusinessHookObservedInput {
    /// Canonical business turn observing admission, even before its Hook seed is prepared.
    /// 观察准入的规范业务回合，即使其 Hook 种子尚未准备。
    pub turn_id: TurnId,
    /// Stable canonical browser action, also used by queued Hook notification deduplication.
    /// 稳定规范浏览器动作，同时用于排队 Hook 通知去重。
    pub client_action_id: super::ClientActionId,
    /// Exact canonical input identity allocated in the same transaction.
    /// 同一事务中分配的精确规范输入身份。
    pub input_id: super::PendingInputId,
    /// Canonical publication revision at which this business turn first observed the input.
    /// 此业务回合首次观察输入时的规范发布修订。
    pub observed_revision: u64,
    /// Explicit admission kind; a User protocol role alone never proves the source.
    /// 明确准入种类；单独的 User 协议角色不能证明来源。
    pub kind: BusinessHookInputKind,
    /// Original text; later edits or removals do not rewrite this observed input.
    /// 原始正文；后续编辑或删除不改写这条已观察输入。
    pub content: String,
}

/// Canonical human admission kinds with distinct Hook projection and execution timing.
/// Hook 投影及执行时机各不相同的规范人类准入种类。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum BusinessHookInputKind {
    /// A future queued input observed by the currently open business task.
    /// 当前开放业务任务观察到的未来排队输入。
    Queued,
    /// Direct direction guidance admitted to the current business task.
    /// 准入当前业务任务的直接方向引导。
    Interrupt,
}

/// Immutable Hook admission facts; neither display message IDs nor new model text can replace them.
/// 不可变 Hook 准入事实；显示消息 ID 或新模型正文均不能替代。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct BusinessHookBinding {
    /// Physical session owning the Hook, independent of the selected UI conversation.
    /// 拥有 Hook 的物理会话，独立于界面选中会话。
    pub session_id: String,
    /// Canonical event-tree generation that owns the business turn.
    /// 拥有业务回合的规范事件树代次。
    pub segment_generation: u64,
    /// Exact V1 business turn; auxiliary executions cannot capture a binding.
    /// 精确 V1 业务回合；辅助执行不能捕获绑定。
    pub turn_id: TurnId,
    /// Original turn-start event, independent of later projection positions.
    /// 原回合起始事件，独立于后续投影位置。
    pub start_seq: Seq,
    /// Actual immutable Hook correlation selected during initial admission.
    /// 首次准入实际选定的不可变 Hook 关联。
    pub hook_turn_id: String,
    /// Original Hook input after the existing host's prompt transformations.
    /// 经既有宿主提示转换后的原 Hook 输入。
    pub user_content: String,
    /// VMM readiness captured by the original Hook admission; recovery cannot upgrade it.
    /// 原 Hook 准入捕获的 VMM 就绪状态；恢复不能提升。
    pub vmm_runtime_ready: bool,
    /// Whether this business lifecycle may notify user observers; independent agent tasks disable it.
    /// 该业务生命周期是否可通知用户观察器；独立代理任务禁用此项。
    pub observes_user: bool,
}

impl BusinessHookMetadata {
    /// Validates the declared schema and owning generation before any binding is used.
    /// 使用任何绑定之前校验声明模式及所属代次。
    /// `generation` belongs to the containing document; returns explicit malformed-identity errors.
    /// `generation` 属于容器文档；返回明确的畸形身份错误。
    pub fn validate(&self, generation: u64) -> Result<(), DomainError> {
        if self.version != 1 {
            return Err(DomainError::CollaborationStateViolation {
                field: "business_hook.version",
            });
        }
        // Admission observations carry canonical ownership even when their Hook binding is not prepared yet.
        // 即使 Hook 绑定尚未准备，准入观察仍携带规范归属。
        let mut actions = BTreeSet::new();
        let mut inputs = BTreeSet::new();
        if self.observed_inputs.iter().any(|entry| {
            !actions.insert(&entry.client_action_id)
                || !inputs.insert(entry.input_id)
                || entry.observed_revision == 0
                || entry.observed_revision > JS_SAFE_INTEGER_MAX
                || self
                    .observed_inputs
                    .first()
                    .is_some_and(|first| first.turn_id != entry.turn_id)
        }) || self
            .observed_inputs
            .windows(2)
            // Composite admissions share one revision; their stored vector order preserves mutation order.
            // 组合准入共享一个修订；存储列表顺序保留修改顺序。
            .any(|pair| pair[0].observed_revision > pair[1].observed_revision)
        {
            return Err(DomainError::CollaborationStateViolation {
                field: "business_hook.timeline_identity",
            });
        }
        if let Some(binding) = &self.binding
            && (generation == 0
                || generation > JS_SAFE_INTEGER_MAX
                || binding.segment_generation != generation
                || binding.session_id.trim().is_empty()
                || binding.session_id.trim() != binding.session_id
                || binding.hook_turn_id.trim().is_empty()
                || binding.hook_turn_id.trim() != binding.hook_turn_id)
        {
            return Err(DomainError::CollaborationStateViolation {
                field: "business_hook.identity",
            });
        }
        Ok(())
    }
}
