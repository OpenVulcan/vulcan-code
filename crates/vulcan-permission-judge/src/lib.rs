//! Permission judge contract shared by the service and the local judge host process.
//! 服务与本机判定宿主进程共享的权限判定契约。
//!
//! The judge scores one gated tool call as allow, review or block. This crate owns everything both
//! sides must agree on without loading a model: package verification, the policy text, the
//! rendered call, the decision rule and the worker protocol. Inference lives in `vulcan-judge-host`.
//! 判定器把一次受管控工具调用评为 allow、review 或 block。本 crate 持有双方无需加载模型即须一致的
//! 全部内容：模型包验证、策略文本、调用渲染、判定规则与 worker 协议。推理位于 `vulcan-judge-host`。

pub mod cloud;
pub mod decision;
pub mod edge;
pub mod edge_route;
pub mod known;
pub mod package;
pub mod policy;
pub mod protocol;
pub mod render;
pub mod runtime;
pub mod system;

pub use cloud::{CLOUD_ALLOW_PROBABILITY, cloud_label, cloud_policy, cloud_state};
pub use decision::{JudgeLabel, NonFiniteLogits, decide};
pub use edge::{
    EDGE_JUDGE_PATH, EdgeAnswer, EdgeJudgeRequest, EdgeJudgeResponse, EdgeJudgeResult, EdgeOutcome,
    edge_outcomes,
};
pub use edge_route::{
    EDGE_SITE_HEADER, EDGE_STATUS_PATH, EDGE_SWITCH_PERCENT, EdgeCandidate, EdgeComputeMs,
    EdgeExclusion, EdgeStatus, choose_edge_endpoint, edge_score, median_ms,
};
pub use known::{
    FP16_MIN_CUDA_COMPUTE_CAPABILITY, KnownDownload, KnownPackage, KnownVariant, ModelFormat,
    find_known_package, known_package, known_packages,
};
pub use package::{
    DigestCache, JudgePackage, ManifestFile, MemoryDigestCache, PackageError, PackageSummary,
    RuntimeConfig, manifest_files,
};
pub use policy::{JudgePolicy, JudgeSettings, PolicyOption, SettingsError};
pub use render::render_call;
pub use runtime::{BundledRuntime, RuntimeError};
pub use system::JudgeBackend;
