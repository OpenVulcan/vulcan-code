use std::collections::BTreeMap;
use std::sync::Arc;

#[cfg(test)]
mod tests;

use crate::{
    HookError, HookHandler, HookPoint, HookPointDescriptor, HookRegistration,
    HookRegistrationSnapshot,
};

/// Runtime entry combining metadata, handler, and insertion order.
/// 组合元数据、处理器和插入顺序的运行时条目。
#[derive(Clone)]
pub(super) struct HookEntry {
    /// Registration metadata.
    /// 注册元数据。
    pub(super) registration: HookRegistration,
    /// Handler implementation.
    /// 处理器实现。
    pub(super) handler: Arc<dyn HookHandler>,
    /// Monotonic insertion sequence for stable ordering.
    /// 用于稳定排序的单调插入序号。
    pub(super) sequence: u64,
}

/// Registry that stores hook handlers by point.
/// 按点位存储 hook 处理器的注册表。
#[derive(Clone, Default)]
pub struct HookRegistry {
    /// Known hook point descriptors keyed by hook point.
    /// 按 hook 点位索引的已知 hook 点位描述。
    points: BTreeMap<HookPoint, HookPointDescriptor>,
    /// Registered hook entries grouped by hook point.
    /// 按 hook 点位分组的已注册条目。
    entries_by_point: BTreeMap<HookPoint, Vec<HookEntry>>,
    /// Next insertion sequence assigned to a registration.
    /// 下一个分配给注册项的插入序号。
    next_sequence: u64,
}

impl HookRegistry {
    /// Builds an empty hook registry.
    /// 构建一个空 hook 注册表。
    pub fn new() -> Self {
        Self::default()
    }

    /// Registers or replaces one hook point descriptor.
    /// 注册或替换一个 hook 点位描述。
    pub fn register_point(&mut self, descriptor: HookPointDescriptor) -> Result<(), HookError> {
        if descriptor.point.trim().is_empty() {
            return Err(HookError::invalid_registration(
                "hook point descriptor name cannot be empty",
            ));
        }
        if descriptor.modes.is_empty() {
            return Err(HookError::invalid_registration(
                "hook point descriptor must allow at least one mode",
            ));
        }
        self.points.insert(descriptor.point.clone(), descriptor);
        Ok(())
    }

    /// Registers or replaces many hook point descriptors.
    /// 注册或替换多个 hook 点位描述。
    pub fn register_points(
        &mut self,
        descriptors: impl IntoIterator<Item = HookPointDescriptor>,
    ) -> Result<(), HookError> {
        for descriptor in descriptors {
            self.register_point(descriptor)?;
        }
        Ok(())
    }

    /// Returns one hook point descriptor when known.
    /// 返回一个已知 hook 点位描述。
    pub fn point_descriptor(&self, point: &str) -> Option<&HookPointDescriptor> {
        self.points.get(point)
    }

    /// Returns every known hook point descriptor in stable order.
    /// 以稳定顺序返回所有已知 hook 点位描述。
    pub fn point_descriptors(&self) -> Vec<HookPointDescriptor> {
        self.points.values().cloned().collect()
    }

    /// Registers one hook handler.
    /// 注册一个 hook 处理器。
    pub fn register(
        &mut self,
        registration: HookRegistration,
        handler: Arc<dyn HookHandler>,
    ) -> Result<(), HookError> {
        if registration.id.trim().is_empty() {
            return Err(HookError::invalid_registration("hook id cannot be empty"));
        }
        if registration.point.trim().is_empty() {
            return Err(HookError::invalid_registration(
                "hook point cannot be empty",
            ));
        }
        if self
            .entries_by_point
            .values()
            .flat_map(|entries| entries.iter())
            .any(|entry| entry.registration.id == registration.id)
        {
            return Err(HookError::invalid_registration(format!(
                "hook id already registered: {}",
                registration.id
            )));
        }
        if let Some(descriptor) = self.points.get(&registration.point)
            && !descriptor.modes.contains(&registration.mode)
        {
            return Err(HookError::invalid_registration(format!(
                "hook mode {:?} is not allowed for point {}",
                registration.mode, registration.point
            )));
        }

        // Reserve an unambiguous insertion sequence before mutating the registry.
        // 在修改注册表前预留无歧义的插入序号。
        let next_sequence = self.next_sequence.checked_add(1).ok_or_else(|| {
            HookError::invalid_registration("hook registration sequence exhausted")
        })?;
        // The entry retains the sequence assigned by this registry.
        // 条目保留本注册表分配的序号。
        let entry = HookEntry {
            registration: registration.clone(),
            handler,
            sequence: self.next_sequence,
        };
        self.next_sequence = next_sequence;
        self.entries_by_point
            .entry(registration.point)
            .or_default()
            .push(entry);
        Ok(())
    }

    /// Unregisters hook handlers by id and optional point filter.
    /// 按标识和可选点位过滤注销 hook 处理器。
    pub fn unregister(&mut self, hook_id: &str, point: Option<&str>) -> usize {
        let mut removed = 0;
        let target_points = if let Some(point) = point {
            vec![point.to_string()]
        } else {
            self.entries_by_point.keys().cloned().collect::<Vec<_>>()
        };
        for target_point in target_points {
            if let Some(entries) = self.entries_by_point.get_mut(&target_point) {
                let before = entries.len();
                entries.retain(|entry| entry.registration.id != hook_id);
                removed += before.saturating_sub(entries.len());
            }
        }
        self.entries_by_point
            .retain(|_, entries| !entries.is_empty());
        removed
    }

    /// Returns sorted entries for one hook point.
    /// 返回单个 hook 点位的已排序条目。
    pub(super) fn entries_for(&self, point: &str) -> Vec<HookEntry> {
        let mut entries = self
            .entries_by_point
            .get(point)
            .cloned()
            .unwrap_or_default();
        entries.sort_by_key(|entry| (entry.registration.priority, entry.sequence));
        entries
    }

    /// Returns the number of handlers registered for one hook point.
    /// 返回单个 hook 点位注册的处理器数量。
    pub fn count_for(&self, point: &str) -> usize {
        self.entries_by_point.get(point).map_or(0, Vec::len)
    }

    /// Projects one registration namespace from `source` without changing either registry.
    /// 从 `source` 投影一个注册命名空间，不修改任一注册表。
    /// `prefix` must be nonempty; host point policies and all other registrations stay local.
    /// `prefix` 必须非空；宿主点位策略与其他注册项保持本地定义。
    /// Returns a validated registry; equal-priority source entries follow local entries in source insertion order.
    /// 返回已校验注册表；同优先级来源条目按原插入顺序排在本地条目之后。
    /// Validation or sequence exhaustion returns an error without publishing a partial projection.
    /// 校验失败或序号耗尽时返回错误，不发布局部投影。
    pub fn with_registration_prefix_from(
        &self,
        prefix: &str,
        source: &Self,
    ) -> Result<Self, HookError> {
        if prefix.trim().is_empty() {
            return Err(HookError::invalid_registration(
                "hook registration prefix cannot be empty",
            ));
        }
        // Stage only registry state; composition must not emit dispatcher lifecycle events.
        // 仅暂存注册表状态；组合过程不得发送调度器生命周期事件。
        let mut projected = self.clone();
        for entries in projected.entries_by_point.values_mut() {
            entries.retain(|entry| !entry.registration.id.starts_with(prefix));
        }
        projected
            .entries_by_point
            .retain(|_, entries| !entries.is_empty());
        // Preserve insertion order across points instead of using map iteration order.
        // 保持跨点位插入顺序，不使用映射遍历顺序。
        let mut selected = source
            .entries_by_point
            .values()
            .flatten()
            .filter(|entry| entry.registration.id.starts_with(prefix))
            .collect::<Vec<_>>();
        selected.sort_by_key(|entry| entry.sequence);
        for entry in selected {
            projected.register(entry.registration.clone(), Arc::clone(&entry.handler))?;
        }
        Ok(projected)
    }

    /// Returns read-only registration snapshots for one point or all points.
    /// 返回单个点位或所有点位的只读注册快照。
    pub fn registration_snapshots(&self, point: Option<&str>) -> Vec<HookRegistrationSnapshot> {
        let mut snapshots = self
            .entries_by_point
            .iter()
            .filter(|(entry_point, _)| point.is_none_or(|point| point == entry_point.as_str()))
            .flat_map(|(_, entries)| entries.iter())
            .map(|entry| HookRegistrationSnapshot {
                registration: entry.registration.clone(),
                sequence: entry.sequence,
            })
            .collect::<Vec<_>>();
        snapshots.sort_by_key(|snapshot| {
            (
                snapshot.registration.point.clone(),
                snapshot.registration.priority,
                snapshot.sequence,
            )
        });
        snapshots
    }
}
