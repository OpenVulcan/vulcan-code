use std::collections::HashMap;
use std::sync::RwLock;

use crate::cache::invalidation::PromptInvalidationEvent;
use crate::types::PromptSegment;

/// Process-local prompt cache.
/// 进程内提示词缓存。
#[derive(Debug, Default)]
pub struct MemoryPromptCache {
    /// Cached node outputs keyed by node cache key.
    /// 按节点缓存键存储的节点输出缓存。
    node_entries: RwLock<HashMap<String, PromptSegment>>,
    /// Cached bundle outputs keyed by bundle cache key.
    /// 按组合缓存键存储的组合输出缓存。
    bundle_entries: RwLock<HashMap<String, PromptSegment>>,
}

impl MemoryPromptCache {
    /// Create one empty prompt cache.
    /// 创建一个空的提示词缓存。
    pub fn new() -> Self {
        Self::default()
    }

    /// Read one node-cached segment.
    /// 读取一个节点缓存片段。
    pub fn get_node(&self, cache_key: &str) -> Option<PromptSegment> {
        self.node_entries
            .read()
            .ok()
            .and_then(|entries| entries.get(cache_key).cloned())
            .map(|segment| segment.cached_copy())
    }

    /// Store one node-cached segment.
    /// 存储一个节点缓存片段。
    pub fn put_node(&self, segment: PromptSegment) {
        if let Ok(mut entries) = self.node_entries.write() {
            entries.insert(segment.cache_key.clone(), segment);
        }
    }

    /// Read one bundle-cached segment.
    /// 读取一个组合缓存片段。
    pub fn get_bundle(&self, cache_key: &str) -> Option<PromptSegment> {
        self.bundle_entries
            .read()
            .ok()
            .and_then(|entries| entries.get(cache_key).cloned())
            .map(|segment| segment.cached_copy())
    }

    /// Store one bundle-cached segment.
    /// 存储一个组合缓存片段。
    pub fn put_bundle(&self, segment: PromptSegment) {
        if let Ok(mut entries) = self.bundle_entries.write() {
            entries.insert(segment.cache_key.clone(), segment);
        }
    }

    /// Apply one invalidation event to the cache.
    /// 将一个失效事件应用到缓存。
    pub fn apply_invalidation(&self, event: &PromptInvalidationEvent) {
        if let Ok(mut node_entries) = self.node_entries.write() {
            if event.cache_key_prefixes.is_empty() {
                node_entries.clear();
            } else {
                node_entries.retain(|key, _| {
                    !event
                        .cache_key_prefixes
                        .iter()
                        .any(|prefix| key.starts_with(prefix))
                });
            }
        }
        if let Ok(mut bundle_entries) = self.bundle_entries.write() {
            if event.cache_key_prefixes.is_empty() {
                bundle_entries.clear();
            } else {
                bundle_entries.retain(|key, _| {
                    !event
                        .cache_key_prefixes
                        .iter()
                        .any(|prefix| key.starts_with(prefix))
                });
            }
        }
    }
}
