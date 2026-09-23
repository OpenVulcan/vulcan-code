//! Seal every physical descendant before deleting one indexed session root.
//! 删除索引会话根前封存全部物理后代。

use super::*;
use vulcan_workspace_store::workspace_store::SessionV1StorageLocator;

/// Loads the exact `id` from `locator`, rejecting a replaced identity or a stale canonical location.
/// 从 `locator` 加载精确 `id`，拒绝被替换身份或过期规范位置。
/// `registry` owns publication; `indexed_root` selects root usage initialization.
/// `registry` 持有发布权；`indexed_root` 选择根用量初始化。
/// Returns the canonical full-history handle with ordinary cold-start interruption recovery, or an explicit error.
/// 返回保留普通冷启动中断恢复的规范完整历史句柄，失败时返回明确错误。
fn canonical_owner(
    registry: &SessionRuntimeRegistry,
    id: SessionId,
    locator: &SessionV1StorageLocator,
    indexed_root: bool,
) -> Result<Arc<SessionHandle>, SessionV1LifecycleError> {
    // Full loading includes obligations outside the ordinary resident tail.
    // 完整加载包含普通常驻尾段之外的义务。
    let handle = registry.get_or_load(id, || {
        // The physical document must match the registry identity before publication.
        // 物理文档必须在发布前匹配注册表身份。
        let loaded = load_session_v1_full(locator)?;
        if loaded.metadata.session_id != id.to_string() {
            return Err(SessionRuntimeError::LoadedSessionDocumentIdentityMismatch {
                expected: id,
                actual: loaded.metadata.session_id,
            });
        }
        // A refused deletion leaves the canonical owner usable, so retain ordinary cold-start recovery.
        // 删除被拒后规范所有者仍可使用，因此保留普通冷启动恢复。
        let mut document = loaded.document;
        // Persist recovered interruption facts before registry publication, as normal lifecycle loading does.
        // 与普通生命周期加载相同，在注册表发布前持久化恢复的中断事实。
        let projected = document.project_persisted_interruptions()?;
        // Root usage ownership must not be skipped merely because deletion first hydrated the session.
        // 不能因删除首先水合会话而跳过根用量归属。
        let handle = SessionHandle::new(id, loaded.locator, document);
        if projected > 0 {
            handle.flush_all()?;
        }
        if indexed_root {
            handle.initialize_root_usage(false, 0);
        }
        Ok(handle)
    })?;
    if handle.storage_locator()? != *locator {
        return Err(invalid(
            id,
            "Canonical owner belongs to another physical locator.",
        ));
    }
    Ok(handle)
}

/// Rolls back all `sealed` owners before physical deletion, preserving any cleanup errors with `primary`.
/// 物理删除前回滚全部 `sealed` 所有者，将清理错误与 `primary` 一同保留。
/// Returns the original error when every owner reopens; otherwise returns the complete diagnostic.
/// 全部所有者重新开放时返回原错误，否则返回完整诊断。
fn rollback(
    registry: &SessionRuntimeRegistry,
    root: SessionId,
    sealed: &[Arc<SessionHandle>],
    primary: SessionV1LifecycleError,
) -> SessionV1LifecycleError {
    // Every member is attempted independently so one poisoned owner cannot strand unrelated members.
    // 独立尝试每个成员，避免一个中毒所有者使其他成员滞留。
    let mut failures = Vec::new();
    for handle in sealed.iter().rev() {
        if let Err(error) =
            registry.abort_lifecycle_deletion(handle.session_id(), Arc::clone(handle))
        {
            failures.push(format!("{}: {error}", handle.session_id()));
        }
    }
    if failures.is_empty() {
        primary
    } else {
        invalid(
            root,
            format!(
                "{primary}; deletion rollback failed: {}",
                failures.join("; ")
            ),
        )
    }
}

/// Deletes the complete physical tree selected by `root_id` after all canonical owners are fenced.
/// 全部规范所有者被隔离后，删除 `root_id` 选中的完整物理树。
/// `controller` supplies SQLite authority; `registry` serializes loads, writes and lifecycle ownership.
/// `controller` 提供 SQLite 权威；`registry` 串行化加载、写入及生命周期归属。
/// Returns success only after exact physical removal and index deletion; ambiguous physical failures stay sealed.
/// 仅在精确物理移除及索引删除后返回成功；不确定物理失败继续保持封存。
pub(super) async fn delete_tree(
    controller: &HostRuntimeHandle,
    registry: &SessionRuntimeRegistry,
    root_id: SessionId,
) -> Result<(), SessionV1LifecycleError> {
    // Resolve SQLite before reading any directory, including when a root is already resident.
    // 即使根已驻留，也先通过 SQLite 定位，再读取任何目录。
    let root = locate_session_v1_storage(controller, &root_id.to_string()).await?;
    // First discovery determines the ordered lifecycle lock set without holding an out-of-order root lock.
    // 首次发现确定有序生命周期锁集合，不预先持有破坏顺序的根锁。
    let tree = deletion_members(&root, root_id)?;
    // Source and source-root gates preserve terminal receipts while checking their durable acknowledgement.
    // 来源及来源根锁在检查可靠确认期间保留终态回执。
    let peers = guards::peer_ids(registry, &tree)?;
    // Physical deletion members and read-only peers share one global lock order.
    // 物理删除成员与只读对端共享同一全局锁顺序。
    let identities = tree
        .keys()
        .copied()
        .chain(peers.iter().copied())
        .collect::<std::collections::BTreeSet<_>>();
    // Retain every guard through physical and SQLite deletion.
    // 跨物理及 SQLite 删除保留每个守卫。
    let mut lifecycles = Vec::with_capacity(identities.len());
    for id in &identities {
        lifecycles.push(registry.acquire_lifecycle_transition(*id).await?);
    }
    if deletion_members(&root, root_id)? != tree {
        return Err(invalid(
            root_id,
            "Embedded membership changed before lifecycle admission; retry deletion.",
        ));
    }
    if guards::peer_ids(registry, &tree)? != peers {
        return Err(invalid(
            root_id,
            "Settlement peer membership changed before admission; retry deletion.",
        ));
    }
    // Canonical handles are loaded only after all exact owners are held; stale warm aliases are rejected.
    // 仅在持有全部精确所有者后加载规范句柄；拒绝过期驻留别名。
    let mut handles = Vec::with_capacity(tree.len());
    for (id, locator) in &tree {
        // Hydrate the exact current contract without guessing a replacement location.
        // 水合精确当前契约，不猜测替代位置。
        let handle = canonical_owner(registry, *id, locator, *id == root_id)?;
        if handle.read(SessionDocument::has_unsettled_collaboration_for_edit_or_delete)? {
            return Err(SessionV1LifecycleError::UnsettledCollaboration(*id));
        }
        handles.push(handle);
    }
    guards::require_source_acknowledgements(controller, registry, &tree).await?;
    // Seal every owner before unregistering any member or deleting a byte.
    // 注销任何成员或删除任何字节之前，先封存每个所有者。
    let mut sealed = Vec::with_capacity(handles.len());
    for handle in &handles {
        if let Err(error) = registry.begin_lifecycle_deletion(handle.session_id(), handle) {
            // A late admitted collaboration mutation is distinguished from generic registry failure.
            // 将迟到的已准入协作修改与通用注册表失败区分。
            let error = match error {
                SessionRuntimeError::UnsettledCollaboration(id) => {
                    SessionV1LifecycleError::UnsettledCollaboration(id)
                }
                other => other.into(),
            };
            return Err(rollback(registry, root_id, &sealed, error));
        }
        sealed.push(Arc::clone(handle));
    }
    for handle in &handles {
        if let Err(error) = handle.flush_all() {
            return Err(rollback(registry, root_id, &sealed, error.into()));
        }
    }
    // Revalidate physical membership after sealing and flushing every admitted writer.
    // 封存并刷出全部已准入写入者后，再次校验物理成员集合。
    match deletion_members(&root, root_id) {
        Ok(current) if current == tree => {}
        Ok(_) => {
            return Err(rollback(
                registry,
                root_id,
                &sealed,
                invalid(
                    root_id,
                    "Embedded membership changed while sealing; retry deletion.",
                ),
            ));
        }
        Err(error) => return Err(rollback(registry, root_id, &sealed, error)),
    }
    for handle in &handles {
        if let Err(error) = registry.unregister_lifecycle_session(handle.session_id()) {
            return Err(rollback(registry, root_id, &sealed, error.into()));
        }
    }
    // From this boundary onward, cancellation or partial physical failure must never reopen stale writers.
    // 从此边界起，取消或部分物理失败绝不能重新开放过期写入者。
    delete_session_v1(controller, &root_id.to_string()).await?;
    drop(lifecycles);
    Ok(())
}
