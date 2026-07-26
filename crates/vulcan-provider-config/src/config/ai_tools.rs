//! AI capability and hosted-tool provider configuration models.
//! AI 能力与 Hosted Tool 供应商配置模型。

use super::{
    default_true, default_web_search_max_results, is_default_ai_tool_policy, is_false, is_true,
    resolve_env_backed_secret,
};
use crate::ai_capability_provider_catalog;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::BTreeSet;
use std::fmt;
mod capability_configurations;
mod declarations;
mod persisted_configuration;
mod provider_entries;
mod provider_kinds;
mod route_targets;
mod router_tools;
mod tool_kind;
mod vendor_helpers;
mod vendor_models;

use vendor_helpers::*;

pub use capability_configurations::*;
pub use declarations::*;
pub use persisted_configuration::*;
pub use provider_entries::*;
pub use provider_kinds::*;
pub use route_targets::*;
pub use router_tools::*;
pub use tool_kind::*;
pub use vendor_models::*;
