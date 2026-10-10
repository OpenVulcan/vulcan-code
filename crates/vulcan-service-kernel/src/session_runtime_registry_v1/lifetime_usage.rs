//! Root-owned in-memory lifetime accounting, shared by every descendant handle.
//! 全部后代句柄共享的根会话内存生命周期统计。

use super::*;
#[cfg(test)]
use std::sync::OnceLock;
use vulcan_session_core::v1::lifetime_usage::{
    ChildSessionUsage, SessionLifetimeUsage, SessionUsageBucket,
};
use vulcan_workspace_store::workspace_store::{
    load_session_lifetime_usage, save_session_lifetime_usage,
};

/// One serialized aggregate owner; persistence never acquires a session document lock.
/// 一个串行累计所有者；持久化不会获取会话文档锁。
pub(super) struct SessionUsageOwner {
    /// Root identity remains available even when persistence fails.
    /// 即使持久化失败仍然可用的根身份。
    root_session_id: SessionId,
    /// Last usable aggregate or the explicit persistence error.
    /// 最近可用累计或明确的持久化错误。
    document: Mutex<Result<SessionLifetimeUsage, String>>,
    /// Root locator; absent only for in-memory test owners.
    /// 根定位器；仅内存测试所有者没有定位器。
    locator: Option<SessionV1StorageLocator>,
}

impl SessionUsageOwner {
    /// Applies a staged change under the root lock; publishes memory only after saving succeeds.
    /// 在根锁内应用暂存修改；保存成功后才发布内存。
    fn update(
        &self,
        change: impl FnOnce(&mut SessionLifetimeUsage) -> Result<bool, String>,
    ) -> Result<(), String> {
        let mut guard = self.document.lock().map_err(|error| error.to_string())?;
        let mut next = guard.as_ref().map_err(Clone::clone)?.clone();
        match change(&mut next) {
            Ok(false) => return Ok(()),
            Ok(true) => {}
            Err(error) => {
                *guard = Err(error.clone());
                return Err(error);
            }
        }
        if let Some(locator) = &self.locator
            && let Err(error) = save_session_lifetime_usage(locator, &next)
        {
            *guard = Err(error.clone());
            return Err(error);
        }
        *guard = Ok(next);
        Ok(())
    }
}

impl SessionHandle {
    /// Initializes a root at creation or cold load; repeated calls never read storage again.
    /// 在根创建或冷加载时初始化；重复调用绝不重读存储。
    /// Fresh roots start at zero, including forks; existing roots use the single saved aggregate.
    /// 新根从零开始，包含分叉；既有根使用唯一已存累计。
    pub(crate) fn initialize_root_usage(&self, fresh: bool, now_ms: i64) {
        let result = self.lifetime_usage.get_or_init(|| {
            let locator = self.storage_locator().map_err(|error| error.to_string())?;
            let document = if fresh {
                let document = SessionLifetimeUsage::new(self.session_id.to_string(), now_ms);
                save_session_lifetime_usage(&locator, &document)?;
                document
            } else {
                load_session_lifetime_usage(&locator)?
            };
            Ok(Arc::new(SessionUsageOwner {
                root_session_id: self.session_id,
                document: Mutex::new(Ok(document)),
                locator: Some(locator),
            }))
        });
        if let Err(error) = result {
            tracing::warn!("[session-usage] initialize: {error}");
        }
    }

    /// Binds a verified child to its root's sole owner and registers the stable child once.
    /// 将经验证的子会话绑定到根唯一所有者，并只登记一次稳定子会话。
    pub(crate) fn bind_child_usage(&self, root: &SessionHandle, parent_id: SessionId, now_ms: i64) {
        let result = self.lifetime_usage.get_or_init(|| root.usage_owner());
        if let Ok(owner) = result {
            let id = self.session_id.to_string();
            let parent = parent_id.to_string();
            let registered = owner.update(|usage| {
                if let Some(child) = usage.children_by_session_id.get(&id) {
                    if child.parent_session_id == parent {
                        return Ok(false);
                    }
                    return Err("Session usage child parent mismatch".to_owned());
                }
                usage.children_by_session_id.insert(
                    id,
                    ChildSessionUsage {
                        parent_session_id: parent,
                        usage: SessionUsageBucket::default(),
                    },
                );
                usage.revision = usage
                    .revision
                    .checked_add(1)
                    .ok_or("Session usage revision overflow")?;
                usage.updated_at_ms = usage.updated_at_ms.max(now_ms);
                Ok(true)
            });
            if let Err(error) = registered {
                tracing::warn!("[session-usage] register child: {error}");
            }
        } else if let Err(error) = result {
            tracing::warn!("[session-usage] bind child: {error}");
        }
    }

    /// Returns the already-loaded root owner; no disk access participates.
    /// 返回已经加载的根所有者；不访问磁盘。
    fn usage_owner(&self) -> Result<Arc<SessionUsageOwner>, String> {
        self.lifetime_usage
            .get()
            .ok_or("Session usage was not initialized")?
            .clone()
    }

    /// Reads one aggregate snapshot from memory for API projections and Goal inputs.
    /// 从内存读取一份累计快照，用于 API 投影与 Goal 输入。
    pub(crate) fn lifetime_usage_snapshot(&self) -> Result<SessionLifetimeUsage, String> {
        let owner = self.usage_owner()?;
        let guard = owner.document.lock().map_err(|error| error.to_string())?;
        guard.clone()
    }

    /// Reads this Goal owner's own and descendant counters, excluding ancestors and siblings.
    /// 读取当前 Goal 所有者自身与后代计数，排除祖先及兄弟会话。
    pub(crate) fn goal_usage_snapshot(
        &self,
    ) -> Result<vulcan_session_core::v1::lifetime_usage::GoalUsageSnapshot, String> {
        let usage = self.lifetime_usage_snapshot()?;
        let id = self.session_id.to_string();
        if id == usage.root_session_id {
            return Ok(usage.goal_snapshot());
        }
        let main = usage
            .children_by_session_id
            .get(&id)
            .ok_or("Goal usage child is not registered")?
            .usage
            .clone();
        let mut children = SessionUsageBucket::default();
        let mut visited = std::collections::BTreeSet::from([id.clone()]);
        let mut pending = vec![id];
        while let Some(parent) = pending.pop() {
            for (child_id, child) in &usage.children_by_session_id {
                if child.parent_session_id == parent && visited.insert(child_id.clone()) {
                    children.merge(&child.usage)?;
                    pending.push(child_id.clone());
                }
            }
        }
        Ok(vulcan_session_core::v1::lifetime_usage::GoalUsageSnapshot { main, children })
    }

    /// Returns the immutable root identity for usage notifications, without reading storage.
    /// 返回用量通知所需的不可变根身份，不读取存储。
    pub(crate) fn lifetime_usage_root_id(&self) -> Result<SessionId, String> {
        Ok(self.usage_owner()?.root_session_id)
    }

    /// Adds one actual terminal or tool increment; errors stay visible without failing the Provider.
    /// 加入一次实际终态或工具增量；错误明确可见，不使 Provider 失败。
    pub(crate) fn record_lifetime_usage(&self, increment: &SessionUsageBucket, now_ms: i64) {
        let result = self.usage_owner().and_then(|owner| {
            owner.update(|usage| {
                usage.apply(&self.session_id.to_string(), increment, now_ms)?;
                Ok(true)
            })
        });
        if let Err(error) = result {
            tracing::warn!("[session-usage] update: {error}");
        }
    }
}

/// Builds a storage-free owner for existing deterministic SessionHandle fixtures.
/// 为既有确定性 SessionHandle 测试构造无存储所有者。
#[cfg(test)]
pub(super) fn test_usage_owner(
    session_id: SessionId,
) -> OnceLock<Result<Arc<SessionUsageOwner>, String>> {
    OnceLock::from(Ok(Arc::new(SessionUsageOwner {
        root_session_id: session_id,
        document: Mutex::new(Ok(SessionLifetimeUsage::new(session_id.to_string(), 0))),
        locator: None,
    })))
}

#[cfg(test)]
mod tests {
    use super::*;
    use vulcan_workspace_store::workspace_store::{
        create_session_v1_storage_fixture, locate_embedded_task_session_v1,
    };

    /// Concurrent descendants share one serialized file and retain independent own counters.
    /// 并发后代共享一个串行文件，保留独立自身计数。
    #[test]
    fn concurrent_children_persist_without_lost_updates_or_child_files() {
        let temporary = tempfile::tempdir().unwrap();
        let root_id = SessionId::from_u128(100);
        let locator =
            create_session_v1_storage_fixture(temporary.path(), &root_id.to_string(), 1).unwrap();
        let root = Arc::new(SessionHandle::new(
            root_id,
            locator.clone(),
            load_session_v1_full(&locator).unwrap().document,
        ));
        root.initialize_root_usage(true, 1);
        let mut children = Vec::new();
        for index in 1..=4 {
            let child_id = SessionId::from_u128(index);
            let child_locator =
                locate_embedded_task_session_v1(&locator, &child_id.to_string()).unwrap();
            let child = Arc::new(SessionHandle::new(
                child_id,
                child_locator,
                SessionDocument::new(child_id.to_string(), locator.project_id(), "child", 1),
            ));
            let parent_id = if index == 4 {
                SessionId::from_u128(1)
            } else {
                root_id
            };
            child.bind_child_usage(&root, parent_id, 1);
            child.bind_child_usage(&root, parent_id, 1);
            children.push(child);
        }
        let increment = SessionUsageBucket::from_api(
            &vulcan_session_core::v1::UsageSnapshot {
                uncached_input_tokens: Some(10),
                output_tokens: Some(5),
                total_tokens: Some(15),
                ..Default::default()
            },
            &vulcan_session_core::v1::ApiTimings::default(),
        )
        .unwrap();
        std::thread::scope(|scope| {
            for child in &children {
                let increment = &increment;
                scope.spawn(move || {
                    for _ in 0..10 {
                        child.record_lifetime_usage(increment, 2);
                    }
                });
            }
            root.record_lifetime_usage(&increment, 2);
        });
        let stored = load_session_lifetime_usage(&locator).unwrap();
        assert_eq!(root.lifetime_usage_snapshot().unwrap(), stored);
        assert_eq!(stored.main.request_count, 1);
        assert_eq!(stored.children_total.request_count, 40);
        assert_eq!(stored.children_by_session_id.len(), 4);
        assert_eq!(stored.children_total.budget_tokens.sum, 600);
        assert_eq!(
            root.goal_usage_snapshot().unwrap().children.request_count,
            40
        );
        assert_eq!(
            children[0]
                .goal_usage_snapshot()
                .unwrap()
                .main
                .request_count,
            10
        );
        assert_eq!(
            children[0]
                .goal_usage_snapshot()
                .unwrap()
                .children
                .request_count,
            10
        );
        assert_eq!(
            children[1]
                .goal_usage_snapshot()
                .unwrap()
                .children
                .request_count,
            0
        );
        for child in children {
            assert_eq!(
                stored.children_by_session_id[&child.session_id().to_string()]
                    .usage
                    .request_count,
                10
            );
            assert!(
                !child
                    .storage_locator()
                    .unwrap()
                    .session_directory()
                    .join("usage.json")
                    .exists()
            );
        }
    }

    /// Once initialized, all reads use memory and every later change rewrites the root file.
    /// 初始化后全部读取使用内存，后续每次修改重写根文件。
    #[test]
    fn resident_owner_does_not_reload_external_file_changes() {
        let temporary = tempfile::tempdir().unwrap();
        let id = SessionId::from_u128(101);
        let locator =
            create_session_v1_storage_fixture(temporary.path(), &id.to_string(), 1).unwrap();
        let handle = SessionHandle::new(
            id,
            locator.clone(),
            load_session_v1_full(&locator).unwrap().document,
        );
        handle.initialize_root_usage(true, 1);
        std::fs::write(
            locator.session_directory().join("usage.json"),
            "invalid external content",
        )
        .unwrap();
        handle.initialize_root_usage(false, 1);
        assert_eq!(
            handle.lifetime_usage_snapshot().unwrap().main.request_count,
            0
        );
        handle.record_lifetime_usage(
            &SessionUsageBucket {
                tool_call_count: 1,
                ..Default::default()
            },
            2,
        );
        assert_eq!(
            load_session_lifetime_usage(&locator)
                .unwrap()
                .main
                .tool_call_count,
            1
        );
    }
}
