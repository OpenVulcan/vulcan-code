//! Canonical participant ownership across collaboration reads and blocking publications.
//! 跨协作读取与阻塞发布保留规范参与者所有权。

use super::*;

/// Runs `work` with lifecycle ownership of every canonical handle in `handles`.
/// 持有 `handles` 中每个规范句柄的生命周期所有权执行 `work`。
/// `registry` supplies the unique owners; the result is returned after blocking work finishes.
/// `registry` 提供唯一所有者；阻塞工作结束后返回其结果。
/// The callback must use synchronous handle operations and must not reacquire lifecycle gates.
/// 回调必须使用同步句柄操作，不得重新获取生命周期锁。
pub(crate) async fn with_canonical_handles<T, Work>(
    registry: &Arc<SessionRuntimeRegistry>,
    mut handles: Vec<Arc<SessionHandle>>,
    work: Work,
) -> Result<T, String>
where
    T: Send + 'static,
    Work: FnOnce() -> Result<T, String> + Send + 'static,
{
    // Global identity order prevents reverse-direction deliveries from taking opposite lock orders.
    // 全局身份顺序防止反向投递按相反顺序取锁。
    handles.sort_by_key(|handle| handle.session_id());
    // Acquire each identity once, but retain every supplied handle for stale-owner validation.
    // 每个身份只取锁一次，但保留每个传入句柄以校验过期所有者。
    let mut lifecycles = Vec::with_capacity(handles.len());
    let mut previous_id = None;
    for handle in &handles {
        if previous_id != Some(handle.session_id()) {
            lifecycles.push(
                registry
                    .acquire_lifecycle_transition(handle.session_id())
                    .await
                    .map_err(|error| {
                        format!("failed to acquire collaboration lifecycle gate: {error}")
                    })?,
            );
            previous_id = Some(handle.session_id());
        }
    }
    // Move guards into the actual writer: cancelling its async waiter cannot admit revocation early.
    // 将守卫移入实际写入器：取消异步等待者不能提前放行撤销。
    let registry = Arc::clone(registry);
    crate::spawn_service_blocking_task(move || {
        for handle in &handles {
            let canonical = registry
                .get(handle.session_id())
                .map_err(|error| format!("collaboration owner is unavailable: {error}"))?;
            if !Arc::ptr_eq(&canonical, handle) {
                return Err(String::from(
                    "collaboration handle is stale after lifecycle admission",
                ));
            }
        }
        let result = work();
        drop(lifecycles);
        result
    })
    .await
    .map_err(|error| format!("collaboration publication task failed: {error}"))?
}
