//! Durable non-secret inventories precede secure-store writes and retain interrupted candidate ownership.
//! 持久非秘密清单先于安全存储写入，并保留被中断候选的所有权。

mod discovery;
mod operations;
#[cfg(test)]
mod tests;
mod writer;

use super::{
    VulcanPluginCredentialRevision, VulcanPluginGenerationIdentity, VulcanPluginGenerationStore,
};
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};
use std::fs::File;

/// One journal schema owns its immutable inventory and explicit progress state.
/// 单个日志模式持有不可变清单及显式进度状态。
const CREDENTIAL_RECORD_VERSION: u32 = 1;

/// Exact manifest-owned server and secret identifiers; neither component contains a credential value.
/// 清单所属的精确服务器及秘密标识；两个组件均不含凭据值。
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct VulcanPluginCredentialKey {
    /// Exact MCP server identifier within the plugin.
    /// 插件内精确 MCP 服务器标识。
    pub server_id: String,
    /// Declared secret-source key, preserved without normalization.
    /// 声明的秘密来源键，保持原样而不规范化。
    pub secret_key: String,
}

/// Complete candidate values; explicit absence supports disabled plugins with unconfigured declared slots.
/// 完整候选值；显式缺失支持具有未配置声明槽位的已停用插件。
/// This in-memory map must never be serialized or included in diagnostic output.
/// 此内存映射绝不能被序列化或写入诊断输出。
pub type VulcanPluginCredentialValues = BTreeMap<VulcanPluginCredentialKey, Option<String>>;

/// Durable credential progress, separate from plugin readiness or activation.
/// 持久凭据进度，独立于插件就绪或激活。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum VulcanPluginCredentialPhase {
    /// The complete cleanup inventory is durable; OS writes may be absent or partial.
    /// 完整清理清单已持久化；OS 写入可能尚未开始或只完成部分。
    Preparing,
    /// Every declared presence and value was verified before this checkpoint was persisted.
    /// 持久保存此检查点前，已验证每项声明存在性及值。
    Staged,
    /// Candidate deletion started; only the original cleanup may continue.
    /// 候选删除已开始；仅能继续原清理。
    Discarding,
    /// All declared OS entries were confirmed absent; the record prevents identity reuse.
    /// 全部声明 OS 条目已确认为缺失；记录防止身份复用。
    Discarded,
}

/// Strict non-secret journal snapshot; returning it does not grant a mutation lease.
/// 严格非秘密日志快照；返回快照不会授予变更租约。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct VulcanPluginCredentialRecord {
    /// Store validation rejects every unknown schema version before consuming the record.
    /// 存储校验在消费记录前拒绝所有未知模式版本。
    schema_version: u32,
    /// Exact package that supplied this inventory, including its inspected byte identity.
    /// 提供此清单的精确插件包，包含已检查字节身份。
    package: VulcanPluginGenerationIdentity,
    /// Stable secure-store namespace that never changes across recovery.
    /// 在恢复期间绝不改变的稳定安全存储命名空间。
    revision: VulcanPluginCredentialRevision,
    /// Complete canonical manifest key set, including explicitly unconfigured slots.
    /// 完整规范清单键集合，包含明确未配置槽位。
    keys: Vec<VulcanPluginCredentialKey>,
    /// Canonical subset expected to contain values; no value or value digest is persisted.
    /// 预期包含值的规范子集；不持久保存值或值摘要。
    configured_keys: Vec<VulcanPluginCredentialKey>,
    /// Explicit progress checkpoint, never inferred from an observer's lifetime.
    /// 显式进度检查点，绝不从观察者生命周期推断。
    phase: VulcanPluginCredentialPhase,
}

impl VulcanPluginCredentialRecord {
    /// Borrow the exact package identity that supplied the inventory.
    /// 借用提供清单的精确插件包身份。
    pub fn package(&self) -> &VulcanPluginGenerationIdentity {
        &self.package
    }
    /// Borrow the original secure-store revision without searching for a newer selection.
    /// 借用原安全存储修订，不搜索更新选择。
    pub fn revision(&self) -> &VulcanPluginCredentialRevision {
        &self.revision
    }
    /// Borrow the complete cleanup inventory, including unconfigured slots.
    /// 借用完整清理清单，包含未配置槽位。
    pub fn keys(&self) -> &[VulcanPluginCredentialKey] {
        &self.keys
    }
    /// Borrow the declared configured subset, containing only names and no secret values.
    /// 借用声明的已配置子集，仅包含名称而不含秘密值。
    pub fn configured_keys(&self) -> &[VulcanPluginCredentialKey] {
        &self.configured_keys
    }
    /// Return durable progress; staged does not imply plugin readiness or route publication.
    /// 返回持久进度；已暂存不表示插件就绪或路由发布。
    pub fn phase(&self) -> VulcanPluginCredentialPhase {
        self.phase
    }
}

/// One original preparation owns an exclusive filesystem lease; dropping it never deletes credentials.
/// 单个原准备持有排他文件系统租约；丢弃准备绝不删除凭据。
#[derive(Debug)]
pub struct VulcanPluginCredentialStaging {
    /// Exact canonical store used for the original durable intent.
    /// 原持久意图使用的精确规范存储。
    store: VulcanPluginGenerationStore,
    /// Non-secret expected record; external record edits must not redirect this owner.
    /// 非秘密预期记录；外部记录编辑不得重定向此所有者。
    record: VulcanPluginCredentialRecord,
    /// Actual exclusive revision lease retained through every backend operation.
    /// 在每次后端操作期间保留的实际排他修订租约。
    _lease: File,
}

/// Secure backend operations address one explicit revision, never legacy or latest-selection storage.
/// 安全后端操作寻址单个显式修订，绝不寻址旧命名空间或最新选择存储。
/// Implementations must not reenter generation-store operations while called under a revision lease.
/// 实现在修订租约下被调用时，不得重入代次存储操作。
pub trait VulcanPluginCredentialBackend {
    /// Read the exact plugin/revision/key; return None only for an actually missing secure entry.
    /// 读取精确插件、修订及键；仅安全条目确实缺失时返回 None。
    fn read(
        &self,
        plugin_id: &str,
        revision: &VulcanPluginCredentialRevision,
        key: &VulcanPluginCredentialKey,
    ) -> Result<Option<String>, String>;
    /// Write `value` to the exact tuple; return the backend result without logging secret values.
    /// 将 `value` 写入精确元组；返回后端结果，不记录秘密值。
    fn write(
        &self,
        plugin_id: &str,
        revision: &VulcanPluginCredentialRevision,
        key: &VulcanPluginCredentialKey,
        value: &str,
    ) -> Result<(), String>;
    /// Delete the exact tuple; actual absence is idempotent success, all other failures remain errors.
    /// 删除精确元组；实际缺失为幂等成功，其他失败保持为错误。
    fn delete(
        &self,
        plugin_id: &str,
        revision: &VulcanPluginCredentialRevision,
        key: &VulcanPluginCredentialKey,
    ) -> Result<(), String>;
}

/// Derive a canonical configured subset from the caller's complete in-memory values without retaining values.
/// 从调用方完整内存值派生规范已配置子集，不保留值。
fn configured_keys(values: &VulcanPluginCredentialValues) -> BTreeSet<VulcanPluginCredentialKey> {
    values
        .iter()
        .filter(|(_, value)| value.is_some())
        .map(|(key, _)| key.clone())
        .collect()
}

/// Derive sorted unique declared slots from one verified `descriptor`; never consult mutable configuration or secrets.
/// 从单个已验证 `descriptor` 派生有序唯一声明槽位；绝不查询可变配置或秘密。
pub(super) fn declared_keys(
    descriptor: &crate::VulcanPluginDescriptor,
) -> Vec<VulcanPluginCredentialKey> {
    descriptor
        .mcp_servers
        .iter()
        .flat_map(|server| {
            server
                .manifest
                .declared_secret_keys()
                .into_iter()
                .map(|secret_key| VulcanPluginCredentialKey {
                    server_id: server.id.clone(),
                    secret_key,
                })
        })
        .collect::<BTreeSet<_>>()
        .into_iter()
        .collect()
}
