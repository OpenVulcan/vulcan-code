//! Permission judge service: system settings, model registry, local worker and CUDA runtime.
//! 权限判定服务：系统设置、模型登记表、本机 worker 与 CUDA 执行库。

mod cloud;
mod environment;
mod records;
mod review;
mod review_rules;
mod services;
mod unattended;
mod worker;

use std::sync::{Arc, Mutex};

use vulcan_permission_judge::MemoryDigestCache;
use vulcan_service_api::PermissionJudgeCudaInstall;

pub(crate) use cloud::PERMISSION_JUDGE_CONSUMER_PROFILE_ID;
pub(crate) use review::{
    PERMISSION_REVIEW_CONSUMER_PROFILE_ID, PERMISSION_REVIEW_PURPOSE_ID, review_call,
};
pub(crate) use review_rules::{
    service_project_review_rules, service_review_rules_system_default,
    service_save_project_review_rules, service_save_review_rules_system_default,
};
pub(crate) use services::*;
pub(crate) use unattended::*;

/// Process-wide judge state shared by settings commands and the tool permission gate.
/// 设置命令与工具权限闸门共享的进程级判定状态。
#[derive(Default)]
pub(crate) struct PermissionJudgeRuntime {
    /// The single local worker.
    /// 唯一的本机 worker。
    worker: worker::JudgeWorker,
    /// Package file digests reused until a file changes.
    /// 文件变化前复用的模型包文件摘要。
    digest_cache: Arc<MemoryDigestCache>,
    /// Latest CUDA installation.
    /// 最近一次 CUDA 安装。
    cuda_install: Mutex<Option<PermissionJudgeCudaInstall>>,
    /// Decision log of the judge and the reviewer.
    /// 判定器与复核器的判定记录。
    records: Arc<records::RecordStore>,
}
