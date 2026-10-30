//! Provider catalog, persistence, and remote model helper logic.
//! 模型供应商目录、持久化与远端模型辅助逻辑。

use crate::runtime_assets;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};
use vulcan_vmm::VmmConfigLayer;

mod configuration_io;
pub use configuration_io::*;

mod git_commit;
pub use git_commit::*;

mod mcp_service;
pub use mcp_service::*;

mod permission_judge;
pub use permission_judge::*;

mod reasoning;
pub use reasoning::*;

mod retry;
pub use retry::*;

mod rtk;
pub use rtk::*;

mod runtime_persistence;
pub use runtime_persistence::*;

mod subagent_models;
pub use subagent_models::*;

mod system_prompt_files;
pub use system_prompt_files::*;
