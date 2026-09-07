//! Exact host conversation lineage for native and external memory cache projections.
//! 原生及外置记忆缓存投影使用的准确宿主会话沿革。

/// Identifies the authoritative host generation and its optional committed compaction.
/// 标识权威宿主代次及其可选已提交压缩。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct VmmContextEpoch {
    /// Host-owned segment generation; this value is never inferred from cache revisions.
    /// 宿主持有的分片代次；绝不从缓存修订号推断此值。
    generation: u64,
    /// Exact committed compaction identity, or explicit absence after creation or rewind.
    /// 准确已提交压缩身份，或创建及回退后的明确缺失。
    compaction_id: Option<String>,
}

impl VmmContextEpoch {
    /// Freezes host-provided generation and compaction; rejects an explicitly empty compaction ID.
    /// 冻结宿主提供的代次及压缩身份，拒绝显式空压缩 ID。
    /// Takes authoritative values and returns the validated immutable identity, without storage or inference.
    /// 接收权威值并返回已验证的不可变身份，不执行存储或推理。
    pub fn new(generation: u64, compaction_id: Option<String>) -> Result<Self, String> {
        if compaction_id
            .as_ref()
            .is_some_and(|id| id.trim().is_empty())
        {
            return Err(
                "VMM context epoch requires a non-empty committed compaction ID.".to_owned(),
            );
        }
        Ok(Self {
            generation,
            compaction_id,
        })
    }

    /// Returns the exact optional compaction identity independently of the cache epoch's encoding.
    /// 独立于缓存纪元编码，返回准确的可选压缩身份。
    pub fn compaction_id(&self) -> Option<&str> {
        self.compaction_id.as_deref()
    }

    /// Encodes the complete host lineage into the existing opaque cache epoch field.
    /// 将完整宿主沿革编码进既有不透明缓存纪元字段。
    /// Returns an unambiguous ID; absence and a literal compaction name can never collide.
    /// 返回无歧义 ID；缺失状态与字面压缩名称绝不会冲突。
    pub fn cache_epoch_id(&self) -> String {
        match &self.compaction_id {
            Some(id) => format!("v1:{}:compaction:{id}", self.generation),
            None => format!("v1:{}:uncompacted", self.generation),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Separates edits, compactions and explicit rewind without parsing cached message positions.
    /// 不解析缓存消息位置即可区分编辑、压缩及明确回退。
    #[test]
    fn epoch_encoding_keeps_generation_and_absence_distinct() {
        // The same compaction ID in another generation is not the same host history.
        // 另一代次中的同一压缩 ID 不属于同一宿主历史。
        let first = VmmContextEpoch::new(1, Some("9".to_owned())).expect("first");
        let edited = VmmContextEpoch::new(2, Some("9".to_owned())).expect("edited");
        let rewound = VmmContextEpoch::new(2, None).expect("rewound");
        let literal = VmmContextEpoch::new(2, Some("uncompacted".to_owned())).expect("literal");
        assert_ne!(first.cache_epoch_id(), edited.cache_epoch_id());
        assert_ne!(edited.cache_epoch_id(), rewound.cache_epoch_id());
        assert_ne!(rewound.cache_epoch_id(), literal.cache_epoch_id());
        assert_eq!(first.compaction_id(), Some("9"));
        assert_eq!(rewound.compaction_id(), None);
        assert!(VmmContextEpoch::new(1, Some(" ".to_owned())).is_err());
    }
}
