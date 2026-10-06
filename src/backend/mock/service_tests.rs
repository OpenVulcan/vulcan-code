// The synthetic authority is tested through the same public API builders as the UI.
// 使用与界面相同的公开 API 构建器测试合成权威数据。

use super::*;
use crate::service_api::{
    knowledge_base as kb, memory_actions as vmm, settings_actions as settings, vce,
};
/// Managed browsing and reading round-trip exact identities, folder boundaries and pagination.
/// 托管浏览及阅读往返准确身份、目录边界和分页。
#[test]
fn managed_documents_round_trip_paging_folders_and_corpora() {
    // Synthetic service authority with root and nested managed documents.
    // 包含根目录及子目录托管文档的合成服务权威状态。
    let mut service = FakeService::new();
    // The caller requests one extra row to prove whether another page exists.
    // 调用方额外请求一行，确认是否存在下一页。
    let first = service
        .answer(&kb::list_managed_knowledge_documents(
            "demo-library",
            None,
            kb::DOCUMENT_PAGE_SIZE + 1,
            0,
        ))
        .unwrap();
    assert_eq!(first.as_array().unwrap().len(), 21);
    assert_eq!(first[0]["id"], "demo-document");
    // The next page overlaps only with the preceding sentinel, never the visible first page.
    // 下一页仅与前页哨兵重叠，不与可见第一页重叠。
    let next = service
        .answer(&kb::list_managed_knowledge_documents(
            "demo-library",
            None,
            kb::DOCUMENT_PAGE_SIZE + 1,
            kb::DOCUMENT_PAGE_SIZE,
        ))
        .unwrap();
    assert_eq!(next.as_array().unwrap().len(), 4);
    assert_eq!(next[0]["id"], first[20]["id"]);
    assert_eq!(
        service
            .answer(&kb::list_managed_knowledge_document_folders("demo-library"))
            .unwrap(),
        json!(["", "guide"])
    );
    assert_eq!(
        service
            .answer(&kb::list_managed_knowledge_documents(
                "demo-library",
                Some(""),
                21,
                0
            ))
            .unwrap()
            .as_array()
            .unwrap()
            .len(),
        1
    );
    assert_eq!(
        service
            .answer(&kb::list_managed_knowledge_documents(
                "demo-library",
                Some("guide"),
                21,
                0
            ))
            .unwrap()[0]["folderPath"],
        "guide"
    );
    // Raw reader content is unescaped Markdown and unavailable display corpus is explicit.
    // 原始阅读正文为未经转义 Markdown，缺失展示语料时显式失败。
    let raw = service
        .answer(&kb::read_managed_knowledge_document(
            "demo-library",
            "demo-document",
            "raw",
        ))
        .unwrap();
    assert!(
        raw["content"]
            .as_str()
            .unwrap()
            .contains("\n  indented line\n")
    );
    assert!(
        service
            .answer(&kb::read_managed_knowledge_document(
                "demo-library",
                "demo-document",
                "display"
            ))
            .is_err()
    );
    service
        .answer(&kb::create_knowledge_library("Empty", "", &[], json!({})))
        .unwrap();
    // A newly created library owns an independent empty managed-document store.
    // 新创建知识库持有独立空托管文档存储。
    let libraries = service.answer(&kb::list_knowledge_libraries()).unwrap();
    // Exact created library identity returned by its catalog.
    // 目录返回的准确创建知识库身份。
    let id = libraries.as_array().unwrap().last().unwrap()["id"]
        .as_str()
        .unwrap()
        .to_string();
    assert_eq!(
        service
            .answer(&kb::list_managed_knowledge_documents(&id, None, 21, 0))
            .unwrap(),
        json!([])
    );
    assert!(
        service
            .answer(&kb::read_managed_knowledge_document(
                &id,
                "demo-document",
                "raw"
            ))
            .is_err()
    );
}

/// Display preflight freezes ready keys and source remapping retains the service-owned source id.
/// 展示预检冻结就绪键，来源重映射保留服务持有来源标识。
#[test]
fn display_confirmation_and_source_remap_preserve_identity() {
    // Stateful synthetic service; no paid inference or native directories are accessed.
    // 有状态合成服务；不访问付费推理或原生目录。
    let mut service = FakeService::new();
    // Exact display preview call.
    // 准确展示预检调用。
    let preview = kb::preview_knowledge_display("demo-library");
    // Service-issued ready documents and fingerprint.
    // 服务签发的就绪文档及指纹。
    let result = service.answer(&preview).unwrap();
    // Freeze the display-specific confirmation envelope and narrow its target set.
    // 冻结展示专用确认结构，并缩减其目标集。
    let mut build = kb::build_knowledge_display_index(&preview, &result).unwrap();
    build.params["confirmation"]["selectedFileKeys"] = json!(["outside"]);
    assert!(service.answer(&build).is_err());
    build.params["confirmation"]["selectedFileKeys"] = json!([result["entries"][0]["key"]]);
    service.answer(&build).unwrap();
    assert!(
        service
            .answer(&kb::read_managed_knowledge_document(
                "demo-library",
                "demo-document",
                "display"
            ))
            .is_ok()
    );
    assert!(
        service
            .answer(&kb::read_managed_knowledge_document(
                "demo-library",
                "demo-document-1",
                "display"
            ))
            .is_err()
    );
    // Rescan with the existing source id models the remap contract.
    // 使用已有来源标识重新扫描，模拟重映射契约。
    let preview = kb::preview_knowledge_import("demo-library", "C:/new-root", Some("demo-source"));
    // Confirm the actual preview rather than inventing a second-step payload.
    // 确认真实预检，不虚构第二步请求。
    let result = service.answer(&preview).unwrap();
    service
        .answer(&kb::import_knowledge_directory(&preview, &result).unwrap())
        .unwrap();
    // Exact source summaries after remapping.
    // 重映射后的准确来源摘要。
    let sources = service
        .answer(&kb::list_knowledge_sources("demo-library"))
        .unwrap();
    assert_eq!(sources.as_array().unwrap().len(), 1);
    assert_eq!(sources[0]["id"], "demo-source");
    assert_eq!(sources[0]["canonicalRoot"], "C:/new-root");
    assert_eq!(sources[0]["documentCount"], 24);
    assert_eq!(
        service
            .answer(&kb::read_managed_knowledge_document(
                "demo-library",
                "demo-document",
                "raw"
            ))
            .unwrap()["document"]["relativePath"],
        "readme.md"
    );
}

/// Full-save and metadata-only mutations are observable through their original loaders.
/// 完整保存与仅元数据变更均可通过其原始加载器观察。
#[test]
fn settings_writes_reload_authority_without_retaining_credentials() {
    // Service.
    // 合成服务权威状态。
    let mut service = FakeService::new();
    service
        .answer(&settings::save_git_commit_settings(
            json!({"branchPrefix":"feature/","prompt":"demo","modelId":"demo-model"}),
        ))
        .unwrap();
    assert_eq!(service.workbench["gitCommit"]["branchPrefix"], "feature/");
    service.answer(&settings::add_public_ai_connection(json!({"providerId":"demo-provider","serviceEntryId":"demo-entry",
        "credentialProfileId":"demo-profile","credentialMode":"token","displayName":"Second","secret":"discard-me"}))).unwrap();
    assert!(!service.loads.to_string().contains("discard-me"));
    // Public AI settings.
    // 公开 AI 设置。
    let ai = &service.loads["service_public_ai_settings_snapshot"];
    // Replacement.
    // 准确替代默认连接身份。
    let replacement = ai["connections"].as_array().unwrap().last().unwrap()["id"]
        .as_str()
        .unwrap()
        .to_string();
    assert!(
        service
            .answer(&settings::remove_public_ai_connection(
                "demo-connection",
                None
            ))
            .is_err()
    );
    service
        .answer(&settings::remove_public_ai_connection(
            "demo-connection",
            Some(&replacement),
        ))
        .unwrap();
    assert_eq!(
        service.loads["service_public_ai_settings_snapshot"]["providerDefaults"]["demo-provider"],
        replacement
    );
}

/// A deleted final library leaves reusable creation defaults and independent source state.
/// 删除最后一个知识库后仍保留可复用的创建默认值及独立来源状态。
#[test]
fn deleting_all_libraries_does_not_break_creation() {
    // Service.
    // 合成服务权威状态。
    let mut service = FakeService::new();
    service
        .answer(&kb::delete_knowledge_library("demo-library"))
        .unwrap();
    service
        .answer(&kb::create_knowledge_library(
            "New",
            "Description",
            &[],
            json!({"retrievalMode":"raw_bm25"}),
        ))
        .unwrap();
    // Catalog.
    // 服务发布的目录。
    let catalog = service.answer(&kb::list_knowledge_libraries()).unwrap();
    // Exact entity identity.
    // 准确实体身份。
    let id = catalog[0]["id"].as_str().unwrap();
    assert_eq!(catalog[0]["name"], "New");
    assert_eq!(
        service.answer(&kb::list_knowledge_sources(id)).unwrap(),
        json!([])
    );
}

/// Per-project revisions, frozen maintenance actions and replay keys protect demo index jobs.
/// 逐项目修订、冻结的维护动作及重放键保护演示索引任务。
#[test]
fn vce_scope_versions_and_job_replay_match_the_service_boundary() {
    // Service.
    // 合成服务权威状态。
    let mut service = FakeService::new();
    service.register_project("new-project");
    // Empty.
    // 尚未初始化的项目概览。
    let empty = service
        .answer(&vce::load_vce_project_overview("new-project"))
        .unwrap();
    assert!(empty["status"]["checkpointVersion"].is_null());
    // Plan call.
    // 准确维护预检调用。
    let plan_call = vce::plan_vce_maintenance("p-tui", "repair", 1);
    // Plan.
    // 已审查操作计划。
    let plan = service.answer(&plan_call).unwrap();
    // Execute.
    // 由预检构建的执行请求。
    let mut execute = vce::execute_vce_maintenance(&plan_call, &plan, 1).unwrap();
    execute.params["request"]["actionIds"] = json!(["unreviewed"]);
    assert!(service.answer(&execute).is_err());
    execute.params["request"]["actionIds"] = json!(["demo-maintenance-action"]);
    // Job.
    // 持久任务回执。
    let job = service.answer(&execute).unwrap();
    assert_eq!(service.answer(&execute).unwrap(), job);
    assert_eq!(
        service.answer(&vce::load_vce_job("p-tui", None)).unwrap(),
        job
    );
    assert!(
        service
            .answer(&vce::load_vce_job("p-core", None))
            .unwrap()
            .is_null()
    );
    service
        .answer(&vce::update_vce_project_settings(
            "p-tui",
            1,
            json!({"enabled":false}),
        ))
        .unwrap();
    // Fresh plan.
    // 新观察的维护计划。
    let fresh_plan = service.answer(&plan_call).unwrap();
    // Stale.
    // 已过期的确认请求。
    let stale = vce::execute_vce_maintenance(&plan_call, &fresh_plan, 1).unwrap();
    assert!(service.answer(&stale).is_err());
    assert_eq!(
        service
            .answer(&vce::load_vce_project_overview("p-core"))
            .unwrap()["settings"]["revision"],
        1
    );
}

/// Native removal includes accurate impact and restores only moved records, preserving new content.
/// 原生移除包含准确影响，恢复时仅恢复已移动记录并保留新内容。
#[test]
fn removal_and_restore_preserve_independent_and_new_records() {
    // Service.
    // 合成服务权威状态。
    let mut service = FakeService::new();
    // Exact service call.
    // 准确服务调用。
    let call = vmm::native_removal_preview(101, "sessions", vec![json!("demo-session")]);
    // Plan.
    // 已审查操作计划。
    let plan = service.answer(&call).unwrap();
    assert_eq!(plan["data"]["value"]["impact"]["turnIds"], json!(["201"]));
    assert_eq!(plan["data"]["value"]["impact"]["memoryIds"], json!(["301"]));
    // Confirmation.
    // 明确预检确认请求。
    let confirmation = vmm::native_removal_recycle(&plan).unwrap();
    service.answer(&confirmation).unwrap();
    assert!(service.answer(&confirmation).is_err());
    // Removal changes active/index counts and retained counts through their original loaders.
    // 移除通过原始加载器改变活动、索引及保留计数。
    let vector = service.answer(&vmm::native_vector_status()).unwrap();
    assert_eq!(vector["data"]["value"]["activeMemoryCount"], 0);
    assert_eq!(vector["data"]["value"]["recycleBinMemoryCount"], 1);
    assert_eq!(
        service.answer(&vmm::native_lexical_status()).unwrap()["data"]["value"]["documentCount"],
        0
    );
    assert_eq!(
        service.answer(&vmm::native_retention_status()).unwrap()["data"]["value"]["recycleBatchCount"],
        1
    );
    assert!(
        service.records[&101]["memories"]
            .as_array()
            .unwrap()
            .is_empty()
    );
    assert_eq!(
        service.memory["native_profile_records"]["nodes"]
            .as_array()
            .unwrap()
            .len(),
        1
    );
    // New content is appended after removal to exercise restoration without rollback.
    // 移除后追加新内容，验证恢复不会回滚这些记录。
    service.records.get_mut(&101).unwrap()["memories"]
        .as_array_mut()
        .unwrap()
        .push(json!({"memoryId":302,"sourceTurnId":202,"sessionSourceId":"new-session"}));
    // Batch.
    // 回收批次身份。
    let batch = service.memory["native_recycle_bin"]["batches"][0]["batchId"]
        .as_str()
        .unwrap()
        .to_string();
    service
        .answer(&vmm::native_recycle_restore(&batch, service.version))
        .unwrap();
    assert_eq!(
        service.records[&101]["memories"].as_array().unwrap().len(),
        2
    );
    assert_eq!(
        service.records[&102]["memories"].as_array().unwrap().len(),
        0
    );
    // Restoration merges current and retained content, and its observations report both.
    // 恢复合并当前及保留内容，观察结果应同时统计两者。
    let vector = service.answer(&vmm::native_vector_status()).unwrap();
    assert_eq!(vector["data"]["value"]["activeMemoryCount"], 2);
    assert_eq!(vector["data"]["value"]["recycleBinMemoryCount"], 0);
    assert_eq!(
        service.answer(&vmm::native_lexical_status()).unwrap()["data"]["value"]["indexedRowCount"],
        2
    );
    assert_eq!(
        service.answer(&vmm::native_retention_status()).unwrap()["data"]["value"]["recycleBatchCount"],
        0
    );
    assert!(
        service.memory["native_recycle_bin"]["batches"]
            .as_array()
            .unwrap()
            .is_empty()
    );
}

/// A changed content version invalidates destructive previews while derived lexical rebuilds keep it.
/// 内容版本变更使破坏性预检失效，而派生词法重建保持该版本。
#[test]
fn preview_versions_and_derived_rebuilds_are_distinct() {
    // Service.
    // 合成服务权威状态。
    let mut service = FakeService::new();
    // Exact service call.
    // 准确服务调用。
    let call = vmm::native_project_clear_preview(101);
    // Preview.
    // 只读预检。
    let preview = service.answer(&call).unwrap();
    assert_eq!(preview["data"]["kind"], "native_clear_preview");
    // Confirm.
    // 准确预检确认请求。
    let confirm = vmm::native_project_confirmation(&call, &preview).unwrap();
    service
        .answer(&vmm::native_lexical_rebuild(service.version))
        .unwrap();
    assert_eq!(service.version, 1);
    // Retention.
    // 保留策略组。
    let retention = service.answer(&vmm::native_retention_preview()).unwrap();
    service
        .answer(&vmm::native_project_rename(102, "Changed"))
        .unwrap();
    assert!(service.answer(&confirm).is_err());
    assert!(
        service
            .answer(&vmm::native_retention_run(&retention).unwrap())
            .is_err()
    );
    assert_eq!(
        service.records[&101]["memories"].as_array().unwrap().len(),
        1
    );
}

/// Migration reviews and applies the same session collision rename to every moved source record.
/// 迁移审查并将相同会话冲突改名应用到每条移动的来源记录。
#[test]
fn migration_keeps_reviewed_collision_renames_and_target_content() {
    // Service.
    // 合成服务权威状态。
    let mut service = FakeService::new();
    // A target-owned session collides by name, but its existing memory must stay untouched.
    // 目标持有的会话名称发生冲突，但其已有记忆必须保持不变。
    service.records.get_mut(&102).unwrap()["sessions"] =
        json!([{"sessionSourceId":"demo-session","displayTitle":"Target session"}]);
    service.records.get_mut(&102).unwrap()["memories"] =
        json!([{"memoryId":999,"summary":"Target memory","sessionSourceId":"demo-session"}]);
    // Exact service call.
    // 准确服务调用。
    let call = vmm::native_project_migration_preview(101, 102);
    // Preview.
    // 只读预检。
    let preview = service.answer(&call).unwrap();
    // Renamed.
    // 审查中已冻结的冲突新名称。
    let renamed = preview["data"]["value"]["sessionRenames"][0]["to"].clone();
    assert_eq!(preview["data"]["kind"], "native_migration_preview");
    assert_eq!(
        preview["data"]["value"]["records"]
            .as_array()
            .unwrap()
            .len(),
        3
    );
    // Confirm.
    // 准确预检确认请求。
    let mut confirm = vmm::native_project_confirmation(&call, &preview).unwrap();
    confirm.params["request"]["payload"]["acceptSessionRenames"] = json!(false);
    assert!(service.answer(&confirm).is_err());
    confirm.params["request"]["payload"]["acceptSessionRenames"] = json!(true);
    service.answer(&confirm).unwrap();
    assert_eq!(service.records[&102]["memories"][0]["memoryId"], 999);
    assert_eq!(
        service.records[&102]["memories"][1]["sessionSourceId"],
        renamed
    );
    assert_eq!(
        service.records[&102]["turns"][0]["sessionSourceId"],
        renamed
    );
    assert!(
        service.records[&101]["memories"]
            .as_array()
            .unwrap()
            .is_empty()
    );
}
