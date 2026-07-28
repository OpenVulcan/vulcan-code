//! Shared single-owner storage for one streamed tool-request argument document.
//! 单条流式工具请求参数文档使用的共享唯一存储。

use std::fmt::{Debug, Formatter};
use std::sync::{Arc, Mutex, MutexGuard};

/// Owns the sole long-lived cumulative argument string shared by a provider adapter and request projection.
/// 持有由 Provider 适配器与请求投影共享的唯一长期累计参数字符串。
#[derive(Clone, Default)]
pub(crate) struct ToolCallDraftAssembler {
    /// Shared mutable argument storage; clones share ownership instead of cloning the accumulated text.
    /// 共享可变参数存储；克隆仅共享所有权，不复制累计文本。
    arguments_json: Arc<Mutex<String>>,
}

impl ToolCallDraftAssembler {
    /// Creates one assembler from the exact argument prefix already received.
    /// 使用已经收到的精确参数前缀创建一个 assembler。
    ///
    /// # Parameters
    /// 参数
    /// `initial_arguments` is moved into the sole cumulative allocation.
    /// `initial_arguments` 会被移动到唯一累计分配中。
    ///
    /// # Returns
    /// 返回值
    /// A new shared assembler owning the supplied prefix.
    /// 返回持有所提供前缀的新共享 assembler。
    pub(crate) fn new(initial_arguments: String) -> Self {
        Self {
            arguments_json: Arc::new(Mutex::new(initial_arguments)),
        }
    }

    /// Replaces the cumulative document with one authoritative provider completion snapshot.
    /// 使用一份权威 Provider 完成快照替换累计文档。
    ///
    /// # Parameters
    /// 参数
    /// `arguments_json` is moved in as the authoritative complete JSON document.
    /// `arguments_json` 会作为权威完整 JSON 文档被移动进来。
    pub(crate) fn replace(&self, arguments_json: String) {
        *self.lock_arguments() = arguments_json;
    }

    /// Returns the current UTF-8 byte length without copying argument content.
    /// 在不复制参数内容的情况下返回当前 UTF-8 字节长度。
    ///
    /// # Returns
    /// 返回值
    /// The exact current UTF-8 byte count.
    /// 返回当前精确 UTF-8 字节数。
    pub(crate) fn len(&self) -> usize {
        self.lock_arguments().len()
    }

    /// Creates a short-lived immutable snapshot for background tokenization or final JSON parsing.
    /// 为后台 Token 统计或最终 JSON 解析创建一份短生命周期不可变快照。
    ///
    /// # Returns
    /// 返回值
    /// An exact owned snapshot of the current cumulative document.
    /// 返回当前累计文档的精确自有快照。
    pub(crate) fn snapshot(&self) -> String {
        self.lock_arguments().clone()
    }

    /// Reads the cumulative document under the assembler lock without allocating a second string.
    /// 在 assembler 锁内读取累计文档，且不分配第二份字符串。
    ///
    /// # Parameters
    /// 参数
    /// `reader` consumes a borrowed view while the assembler lock is held.
    /// `reader` 在 assembler 锁持有期间消费借用视图。
    ///
    /// # Returns
    /// 返回值
    /// The value returned by `reader`.
    /// 返回 `reader` 产生的值。
    pub(crate) fn read<R>(&self, reader: impl FnOnce(&str) -> R) -> R {
        let arguments = self.lock_arguments();
        reader(arguments.as_str())
    }

    /// Returns whether two handles refer to the exact same cumulative argument allocation.
    /// 返回两个句柄是否指向完全相同的累计参数分配。
    ///
    /// # Parameters
    /// 参数
    /// `other` is the assembler handle whose storage identity is compared.
    /// `other` 是用于比较存储身份的 assembler 句柄。
    ///
    /// # Returns
    /// 返回值
    /// `true` when both handles share one long-lived cumulative buffer.
    /// 两个句柄共享同一个长期累计缓冲时返回 `true`。
    #[cfg(test)]
    pub(crate) fn shares_storage_with(&self, other: &Self) -> bool {
        Arc::ptr_eq(&self.arguments_json, &other.arguments_json)
    }

    /// Acquires the assembler mutex used by all exact buffer operations.
    /// 获取全部精确缓冲操作共用的 assembler 互斥锁。
    ///
    /// # Returns
    /// 返回值
    /// The mutex guard for the sole cumulative string.
    /// 返回唯一累计字符串的互斥锁守卫。
    fn lock_arguments(&self) -> MutexGuard<'_, String> {
        self.arguments_json
            .lock()
            .expect("tool request argument assembler mutex must remain available")
    }
}

impl Debug for ToolCallDraftAssembler {
    /// Redacts argument content while exposing only safe ownership and byte-count diagnostics.
    /// 在仅暴露安全所有权与字节计数诊断的同时脱敏参数内容。
    ///
    /// # Parameters
    /// 参数
    /// `formatter` receives the redacted debug fields.
    /// `formatter` 接收脱敏后的调试字段。
    ///
    /// # Returns
    /// 返回值
    /// The formatter result after writing safe diagnostics.
    /// 返回写入安全诊断后的格式化结果。
    fn fmt(&self, formatter: &mut Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("ToolCallDraftAssembler")
            .field("argument_bytes", &self.len())
            .field(
                "shared_owner_count",
                &Arc::strong_count(&self.arguments_json),
            )
            .finish()
    }
}

impl PartialEq for ToolCallDraftAssembler {
    /// Compares exact cumulative content for deterministic protocol tests.
    /// 为确定性协议测试比较精确累计内容。
    ///
    /// # Parameters
    /// 参数
    /// `other` is the assembler whose current content is compared.
    /// `other` 是比较当前内容的另一个 assembler。
    ///
    /// # Returns
    /// 返回值
    /// `true` when both current documents are byte-for-byte equal.
    /// 两份当前文档逐字节相等时返回 `true`。
    fn eq(&self, other: &Self) -> bool {
        self.snapshot() == other.snapshot()
    }
}

impl Eq for ToolCallDraftAssembler {}

impl From<String> for ToolCallDraftAssembler {
    /// Moves one owned string into a new assembler without copying its content.
    /// 将一个自有字符串移动到新 assembler 中，且不复制其内容。
    ///
    /// # Parameters
    /// 参数
    /// `arguments_json` is the owned cumulative document to move.
    /// `arguments_json` 是要移动的自有累计文档。
    ///
    /// # Returns
    /// 返回值
    /// A new assembler owning the provided allocation.
    /// 返回持有所提供分配的新 assembler。
    fn from(arguments_json: String) -> Self {
        Self::new(arguments_json)
    }
}
