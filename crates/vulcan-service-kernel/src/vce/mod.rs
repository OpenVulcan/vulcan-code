//! Host-native Vulcan Context Engine integration.
//! 宿主原生 Vulcan Context Engine 集成。

mod ai_port;
mod embedding_dimensions;
mod embedding_policy;
mod manager;
mod readiness;
mod services;
mod source_port;
mod watcher;

pub(crate) use embedding_dimensions::validate_explicit_embedding_dimension;
pub(crate) use embedding_policy::{
    VceEmbeddingCandidate, VceEmbeddingProjectImpact, inspect_embedding_candidate,
    inspect_embedding_project_impact,
};

pub(crate) use ai_port::{
    VCE_QUERY_REWRITE_PURPOSE_ID, VceHostAiAdapter, VceHostAiRegistrationConfig,
    vce_embedding_physical_fingerprint, vce_rerank_truncation,
};
pub(crate) use manager::{
    VCE_CONSUMER_PROFILE_ID, VCE_REFRESH_CANCELLED_ERROR, VceJobAdmission, VceProjectLifecycle,
    VcePublicationAuthority, VcePublicationContract, VcePublicationError, VceRebuildProjectInput,
    VceRuntimeManager, VceRuntimeProjectStatus, VceRuntimeRetrieveResult,
};
pub(crate) use readiness::*;
pub(crate) use services::*;
pub(crate) use source_port::{VceProjectSourceAdapter, VceSourceLimits};
pub(crate) use watcher::VceWatcherService;
