//! Provider catalog, persistence, and remote model helper logic.
//! 模型供应商目录、持久化与远端模型辅助逻辑。

use crate::runtime_assets;
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::collections::{BTreeMap, BTreeSet};
use std::fmt;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::OnceLock;
use vulcan_vmm::VmmConfigLayer;

/// Provider identifiers that are intentionally excluded because their protocol or auth flow is not supported yet.
/// 因当前尚未支持其协议或认证流程而被刻意排除的供应商标识集合。
const UNSUPPORTED_PROVIDER_IDS: &[&str] = &[
    "amazon-bedrock",
    "cloudflare-ai-gateway",
    "github-copilot",
    "gitlab",
    "google-vertex",
    "google-vertex-anthropic",
    "kiro",
    "opencode",
    "opencode-go",
    "sap-ai-core",
    "v0",
    "vercel",
];

mod ai_tools;
pub use ai_tools::*;

mod configuration_io;
pub use configuration_io::*;

#[cfg(test)]
mod dynamic_models;

mod git_commit;
pub use git_commit::*;

mod helpers;

mod host_tools;
pub use host_tools::*;

mod model_catalog;
pub use model_catalog::*;

mod persisted_provider;
pub use persisted_provider::*;

mod provider_definition;
pub use provider_definition::*;

mod provider_types;
pub use provider_types::*;

mod reasoning;
pub use reasoning::*;

mod runtime_persistence;
pub use runtime_persistence::*;

use helpers::*;

#[cfg(test)]
mod tests;
