//! Explicit credential revision identities remain independent from physical package generations.
//! 显式凭据修订身份独立于物理插件包代次。

use serde::{Deserialize, Deserializer, Serialize, Serializer};

/// Canonical host-issued revision identifying one plugin's separately staged credential namespace.
/// 标识单个插件独立暂存凭据命名空间的规范宿主修订。
/// It contains no credential values and never grants access without the owning plugin/server/key tuple.
/// 不包含凭据值，也不能脱离所属插件、服务器及键授予访问。
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub struct VulcanPluginCredentialRevision(String);

impl VulcanPluginCredentialRevision {
    /// Allocate a new credential revision before staging; returns one canonical unique identity.
    /// 暂存前分配新凭据修订；返回单个规范唯一身份。
    pub fn allocate() -> Self {
        Self(uuid::Uuid::new_v4().to_string())
    }

    /// Parse `value` exactly, rejecting alternate UUID spellings and nil revisions; returns its validated identity.
    /// 精确解析 `value`，拒绝其他 UUID 拼写及空修订；返回已校验身份。
    pub fn parse(value: &str) -> Result<Self, String> {
        // Exact textual identity is also the durable namespace key, so aliases must never be normalized silently.
        // 精确文本身份也是持久命名空间键，所以绝不能静默规范化别名。
        let parsed = uuid::Uuid::parse_str(value).map_err(|_| {
            String::from("Plugin credential revision must be a canonical non-nil UUID.")
        })?;
        if parsed.is_nil() || parsed.to_string() != value {
            return Err(String::from(
                "Plugin credential revision must be a canonical non-nil UUID.",
            ));
        }
        Ok(Self(value.into()))
    }

    /// Borrow the canonical revision text for the exact secure-store namespace.
    /// 借用规范修订文本，用于精确安全存储命名空间。
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl Serialize for VulcanPluginCredentialRevision {
    /// Serialize this validated revision as one string through `serializer`; returns its encoding result.
    /// 通过 `serializer` 将已校验修订序列化为单个字符串；返回编码结果。
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.serialize_str(&self.0)
    }
}

impl<'de> Deserialize<'de> for VulcanPluginCredentialRevision {
    /// Decode and validate the exact string supplied by `deserializer`; malformed identities remain errors.
    /// 解码并校验 `deserializer` 提供的精确字符串；损坏身份保持为错误。
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        // Every persisted reference passes through the same canonical parser as explicitly supplied identities.
        // 每个持久引用均经过与明确提供身份相同的规范解析器。
        let value = String::deserialize(deserializer)?;
        Self::parse(&value).map_err(serde::de::Error::custom)
    }
}

/// Frozen credential authority captured with one MCP descriptor, never inferred from the latest catalog.
/// 随单个 MCP 描述符捕获的固定凭据权威，绝不从最新目录推断。
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum VulcanPluginCredentialBinding {
    /// Existing flat packages and activation formats one/two use their unchanged legacy namespace.
    /// 既有平铺包及第一／二版激活格式使用不变的旧命名空间。
    Legacy,
    /// A version-three selection explicitly names one isolated credential revision.
    /// 第三版选择明确指定单个隔离凭据修订。
    Revision(VulcanPluginCredentialRevision),
}
