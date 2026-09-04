//! Host-native Vulcan Context Engine integration.
//! 宿主原生 Vulcan Context Engine 集成。

mod ai_port;
mod manager;
mod source_port;
mod watcher;

pub(crate) use ai_port::{
    VCE_QUERY_REWRITE_PURPOSE_ID, VceHostAiAdapter, VceHostAiRegistrationConfig,
};
pub(crate) use manager::{VceProjectLifecycle, VceRuntimeManager, VceRuntimeProjectStatus};
pub(crate) use source_port::{
    VcePathRefresh, VceProjectSourceAdapter, VceSourceLimits, VceSourceManifest,
};
pub(crate) use watcher::VceWatcherService;
