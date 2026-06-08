use crate::contract::SessionBoundaryEvent;

/// Hook adapter used for Vulcan VMM, LuaSkills lifecycle hooks, and other post-processing.
/// 用于 Vulcan VMM、LuaSkills 生命周期 hook 和其他后处理的 hook 适配器。
pub trait SessionLifecycleHook: Send + Sync {
    /// Observes one event after it has been appended to session storage.
    /// 在事件已追加到会话存储后观察该事件。
    fn after_event_appended(&self, event: &SessionBoundaryEvent) -> Result<(), String>;
}
