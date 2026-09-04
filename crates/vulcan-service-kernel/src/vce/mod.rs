//! Host-native Vulcan Context Engine integration.
//! 宿主原生 Vulcan Context Engine 集成。

mod ai_port;
mod manager;
mod services;
mod source_port;
mod watcher;

pub(crate) use ai_port::{VceHostAiAdapter, VceHostAiRegistrationConfig};
pub(crate) use manager::{
    VCE_REFRESH_CANCELLED_ERROR, VceProjectLifecycle, VceRuntimeManager, VceRuntimeProjectStatus,
    VceRuntimeRetrieveResult,
};
pub(crate) use services::*;
pub(crate) use source_port::{VceProjectSourceAdapter, VceSourceLimits};
pub(crate) use watcher::VceWatcherService;
