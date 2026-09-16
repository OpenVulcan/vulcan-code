//! Shared file-confirmation contracts for import, display generation and ignore management.
//! 导入、显示生成和忽略管理共用的文件确认契约。
use serde::{Deserialize, Serialize};

/// Default source-file size limit for confirmed operations.
/// 确认操作的默认源文件大小上限。
pub const KNOWLEDGE_SOURCE_BYTE_LIMIT: u64 = 64 * 1024;

/// Explains checkbox eligibility; legacy skip records default to oversized.
/// 说明复选框可用性，旧忽略记录默认为超大。
#[derive(Debug, Clone, Copy, Default, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum KnowledgeFileDisposition {
    /// Eligible for execution.
    /// 可以执行。
    Ready,
    /// Current content already completed.
    /// 当前内容已经完成。
    Completed,
    /// Source exceeds the size limit.
    /// 来源超过大小上限。
    #[default]
    Oversized,
    /// Explicit persistent ignore rule.
    /// 明确的持久忽略规则。
    Ignored,
    /// Unsupported format or filesystem link.
    /// 不支持的格式或文件系统链接。
    Unsupported,
    /// Source cannot be read or decoded.
    /// 来源无法读取或解码。
    Unreadable,
    /// Unchecked for this execution only.
    /// 仅本次未勾选。
    NotSelected,
}

/// Exact ignore identity scoped by the containing library settings.
/// 由外层知识库配置限定范围的精确忽略身份。
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct KnowledgeIgnoredFile {
    /// Canonical source directory; no glob matching.
    /// 规范来源目录，不使用通配匹配。
    pub source_root: String,
    /// Slash-separated relative file path.
    /// 斜杠分隔的相对文件路径。
    pub relative_path: String,
}

/// Immutable inspected file row, including disabled entries.
/// 不可变文件检查记录，包含不可选项。
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct KnowledgeFilePreviewEntry {
    /// Stable root-and-path selection key.
    /// 稳定的根目录及路径选择键。
    pub key: String,
    /// Canonical source identity for ignore management.
    /// 忽略管理使用的规范来源身份。
    pub source_root: String,
    /// User-visible relative path.
    /// 用户可见的相对路径。
    pub relative_path: String,
    /// Measured source-file bytes.
    /// 实测源文件字节数。
    pub byte_size: u64,
    /// Checkbox eligibility reason.
    /// 复选框可用性原因。
    pub disposition: KnowledgeFileDisposition,
    /// Readable error details, never treated as instructions.
    /// 可读错误详情，绝不作为指令执行。
    pub detail: Option<String>,
    /// Existing raw identity, absent before first import.
    /// 已有原始库身份，首次导入前为空。
    pub document_id: Option<String>,
    /// Inspected digest, absent for intentionally unread files.
    /// 已检查摘要，有意不读取的文件为空。
    pub content_sha256: Option<String>,
    /// Successful prior result that must be preserved even if ignored.
    /// 即使被忽略也必须保留的先前成功结果。
    pub already_completed: bool,
}

/// Read-only preflight snapshot; requesting it never starts an operation.
/// 只读执行前快照，请求快照不会启动操作。
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct KnowledgeFilePreview {
    /// Hash binding operation, source revision, rules and entries.
    /// 绑定操作、来源修订、规则和记录的哈希。
    pub fingerprint: String,
    /// Complete file list including unavailable rows.
    /// 包括不可用记录的完整文件列表。
    pub entries: Vec<KnowledgeFilePreviewEntry>,
}

/// Explicit selection bound to the exact preview the user confirmed.
/// 绑定用户实际确认的精确预览的明确选择。
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct KnowledgeFileConfirmation {
    /// Confirmed preview fingerprint.
    /// 已确认预览指纹。
    pub fingerprint: String,
    /// Eligible keys to execute; empty means explicitly select nothing.
    /// 待执行的可选键，空列表表示明确全部不选。
    pub selected_keys: Vec<String>,
}

impl KnowledgeFilePreview {
    /// Validates `confirmation` against this snapshot; returns selected rows or an explicit rescan error.
    /// 根据此快照验证 `confirmation`，返回选中记录或明确的重新扫描错误。
    pub fn confirmed_entries<'a>(
        &'a self,
        confirmation: &KnowledgeFileConfirmation,
    ) -> Result<Vec<&'a KnowledgeFilePreviewEntry>, String> {
        if self.fingerprint != confirmation.fingerprint {
            return Err(String::from("文件清单或忽略规则已变化，请重新扫描并确认。"));
        }
        // A repeated key must not dispatch duplicate work.
        // 重复键不得触发重复执行。
        let mut seen = std::collections::BTreeSet::new();
        confirmation
            .selected_keys
            .iter()
            .map(|key| {
                if !seen.insert(key) {
                    return Err(String::from("确认清单包含重复文件。"));
                }
                self.entries
                    .iter()
                    .find(|entry| {
                        &entry.key == key && entry.disposition == KnowledgeFileDisposition::Ready
                    })
                    .ok_or_else(|| String::from("确认清单包含不可选或已不存在的文件，请重新扫描。"))
            })
            .collect()
    }
}
