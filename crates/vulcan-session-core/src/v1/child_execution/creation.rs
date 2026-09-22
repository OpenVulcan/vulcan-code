//! Immutable source for publishing an initial or resumed embedded child execution.
//! 发布初始或续跑嵌入子执行的不可变来源。

use super::ChildExecutionProfile;
use serde::{Deserialize, Serialize};

/// Parent-owned execution creation source retained with the exact child reservation.
/// 与精确子预留一起保留的父侧执行创建来源。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ChildCreationIntent {
    /// Explicit initial allocation or exact existing-history resumption authority.
    /// 明确的初始分配或精确已有历史续跑权威。
    pub origin: ChildCreationOrigin,
    /// Profile bound to coordinates allocated by the initial or existing child document.
    /// 绑定到初始或已有子文档分配坐标的配置。
    pub profile: ChildExecutionProfile,
    /// Canonical project identity shared by the parent and child metadata.
    /// 父子元数据共同使用的规范项目身份。
    pub project_tag: String,
    /// Original execution admission timestamp, never replaced by the recovery attempt time.
    /// 原执行准入时间，不被恢复尝试时间替换。
    pub created_at_ms: i64,
}

/// Closed execution publication modes; resumption never authorizes initial file reconstruction.
/// 封闭的执行发布模式；续跑绝不授权重建初始文件。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum ChildCreationOrigin {
    /// Creates the initial child through the canonical allocator.
    /// 通过规范分配器创建初始子任务。
    Initial,
    /// Appends a successor only to the exact captured existing document revision.
    /// 仅向精确捕获的已有文档修订追加后继。
    Resumption {
        /// Existing child revision immediately before atomic successor publication.
        /// 原子发布后继之前的已有子文档修订。
        base_document_revision: u64,
    },
}
