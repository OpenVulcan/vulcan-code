//! Host-native Vulcan Context Engine integration.
//! 宿主原生 Vulcan Context Engine 集成。

mod ai_port;
mod manager;
mod readiness;
mod services;
mod source_port;
mod watcher;

pub(crate) use ai_port::{
    VCE_QUERY_REWRITE_PURPOSE_ID, VceHostAiAdapter, VceHostAiRegistrationConfig,
    vce_embedding_physical_fingerprint,
};
pub(crate) use manager::{
    VCE_CONSUMER_PROFILE_ID, VCE_REFRESH_CANCELLED_ERROR, VceProjectLifecycle, VceRuntimeManager,
    VceRuntimeProjectStatus, VceRuntimeRetrieveResult,
};
pub(crate) use readiness::*;
pub(crate) use services::*;
pub(crate) use source_port::{VceProjectSourceAdapter, VceSourceLimits};
pub(crate) use watcher::VceWatcherService;
