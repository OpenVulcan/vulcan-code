//! Service-event publisher for plugin-scoped UI invalidation signals.
//! 插件作用域界面失效信号的服务事件发布器。

use vulcan_plugin_runtime::{SystemPluginUiEventPublisher, SystemPluginUiInvalidation};
use vulcan_service_api::{PluginUiInvalidatedEvent, ServiceEvent, ServiceEventEmitter};

/// Publishes plugin UI invalidations through the existing desktop and browser event sinks.
/// 通过现有桌面端与浏览器端事件接收器发布插件界面失效。
pub(crate) struct ServicePluginUiEventPublisher {
    /// Shared service event emitter retained by the application bootstrap.
    /// 由应用引导流程保留的共享服务事件发送器。
    event_emitter: ServiceEventEmitter,
}

impl ServicePluginUiEventPublisher {
    /// Creates one publisher over the shared service event emitter.
    /// 基于共享服务事件发送器创建一个发布器。
    ///
    /// # Parameters
    /// 参数
    /// `event_emitter` delivers events to every connected WebUI surface.
    /// `event_emitter` 将事件投递到每个已连接 WebUI 界面。
    ///
    /// # Returns
    /// 返回值
    /// Returns a synchronous lightweight publisher.
    /// 返回一个同步轻量发布器。
    pub(crate) fn new(event_emitter: ServiceEventEmitter) -> Self {
        Self { event_emitter }
    }
}

impl SystemPluginUiEventPublisher for ServicePluginUiEventPublisher {
    /// Publishes one validated plugin-scoped invalidation event.
    /// 发布一条已校验的插件作用域失效事件。
    ///
    /// # Parameters
    /// 参数
    /// `event` contains only plugin identity, topic, and monotonic revision.
    /// `event` 仅包含插件身份、主题与单调修订号。
    ///
    /// # Returns
    /// 返回值
    /// Returns unit after synchronous fan-out.
    /// 同步扇出后返回空值。
    fn publish(&self, event: SystemPluginUiInvalidation) -> Result<(), String> {
        self.event_emitter
            .emit(ServiceEvent::PluginUiInvalidated(Box::new(
                PluginUiInvalidatedEvent {
                    plugin_id: event.plugin_id,
                    topic: event.topic,
                    revision: event.revision,
                },
            )));
        Ok(())
    }
}
