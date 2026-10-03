//! Existing native observation APIs exposed to the process-owned service collector.
//! 向进程拥有的服务采集器暴露既有原生观测接口。

pub use luaskills::runtime_logging::emit as emit_runtime_log;
pub use luaskills::{RuntimeLogCallback, RuntimeLogEvent, RuntimeLogLevel, set_log_callback};
pub use vulcan_plugin_runtime::{
    VulcanPluginNativeIdentityCallback, VulcanPluginNativeIdentityEvent,
    VulcanPluginNativeIdentitySubscription, install_native_identity_observer,
};
