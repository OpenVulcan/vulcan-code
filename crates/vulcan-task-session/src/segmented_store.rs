use crate::compaction::TaskSessionCompactionStore;
use crate::model::{
    TaskInvocationEvent, TaskInvocationEventKind, TaskSessionId, TaskSessionMetadata,
    TaskSessionStatus,
};
use crate::store::TaskSessionStore;
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::fs::{self, File, OpenOptions};
use std::io::{BufRead, BufReader, Write};
use std::path::{Path, PathBuf};
use vulcan_session_core::{
    ParentSessionRef, SessionCompactionCheckpoint, SessionContextDocument, SessionEventRecord,
    SessionNodeId, SessionNodeStore, SessionRuntimeCheckpoint, SessionSegmentSummary,
    SessionStoreError,
};

mod helpers;
mod invocations;
mod io;
mod mirror;
mod records;
mod store_impl;
#[cfg(test)]
mod tests;
mod traits;

use helpers::*;
use invocations::*;
use io::*;
use mirror::*;
use records::*;

const SEGMENTED_TASK_DIR: &str = "task";
const SEGMENTED_METADATA_FILE: &str = "metadata.json";
const SEGMENTED_STATE_FILE: &str = "state.json";
const DEFAULT_SEGMENT_MAX_LINES: u64 = 3000;
const DEFAULT_SEGMENT_MAX_BYTES: u64 = 8 * 1024 * 1024;

/// Segmented TaskSession store rooted at one main segmented session directory.
/// 以一个主分段会话目录为根的分段子任务会话存储。
#[derive(Debug, Clone)]
pub struct SegmentedTaskSessionStore {
    /// Root segmented main-session directory.
    /// 根分段主会话目录。
    root_session_dir: PathBuf,
}
