//! Real storage regressions for optional workspace session restoration.
//! 工作台可选会话恢复的真实存储回归。

use super::*;
use crate::session_model_projection_v1::read_automatic_session_model_projection_v1;
use vulcan_workbench_model::TaskProjectSummary;
use vulcan_workspace_store::workspace_store::{load_workspace_index_snapshot, persist_new_project};

/// Loads a real mixed active/archive index and removes every missing record without selecting it first.
/// 加载真实混合活动及归档索引，无需先选中便移除每个缺失记录。
/// Returns after checking durable cleanup, healthy membership and zero runtime hydration.
/// 检查持久化清理、正常成员与零运行态水合后返回。
#[tokio::test]
async fn workspace_index_cleans_all_missing_active_and_archived_sessions() {
    // Each fixture owns a private production schema and all session files.
    // 每个夹具独占生产表结构及全部会话文件。
    let fixture = LifecycleStorageFixture::start().await;
    vulcan_workspace_store::data_plane::prepare_application_schema_for_test(&fixture.controller)
        .await
        .unwrap();
    // Project membership must come from the real database loader.
    // 项目成员关系必须来自真实数据库加载器。
    let project = TaskProjectSummary::new(
        "project-explicit",
        "Cleanup",
        fixture.controller.data_root.to_string_lossy(),
        false,
        0,
        None,
        None,
        Vec::new(),
    );
    persist_new_project(fixture.controller.clone(), &project)
        .await
        .unwrap();
    // Creation owns live handles; the index loader must not depend on those handles.
    // 创建过程拥有活动句柄；索引加载器不能依赖这些句柄。
    let registry = SessionRuntimeRegistry::default();
    let healthy_id = SessionId::new_v4();
    create_fixture_session(&fixture, &registry, healthy_id).await;
    // Keep one parse-failed file to prove existence checks never delete corrupt-but-present data.
    // 保留一个解析失败文件，证明存在检查不会删除损坏但仍存在的数据。
    let corrupt_id = SessionId::new_v4();
    let corrupt = create_fixture_session(&fixture, &registry, corrupt_id).await;
    let corrupt_path = fixture
        .controller
        .data_root
        .join(SESSION_V1_STORAGE_PARTITION)
        .join(corrupt.storage_locator().unwrap().relative_path())
        .join("metadata.json");
    std::fs::write(&corrupt_path, b"{corrupt").unwrap();
    // Both active and archived rows must disappear in the same load.
    // 活动及归档行必须在同一次加载中消失。
    let mut missing_ids = Vec::new();
    for archived in [false, false, true] {
        let session_id = SessionId::new_v4();
        let handle = create_fixture_session(&fixture, &registry, session_id).await;
        let locator = handle.storage_locator().unwrap();
        if archived {
            archive_session_lifecycle_v1(
                &fixture.controller,
                &registry,
                session_id,
                1_787_788_800_300,
            )
            .await
            .unwrap();
        }
        // Remove only this fixture's exact metadata file, leaving other files untouched.
        // 仅移除该夹具的精确元数据文件，保持其他文件不变。
        let metadata_path = fixture
            .controller
            .data_root
            .join(SESSION_V1_STORAGE_PARTITION)
            .join(locator.relative_path())
            .join("metadata.json");
        std::fs::remove_file(metadata_path).unwrap();
        missing_ids.push(session_id);
    }
    for _ in 0..2 {
        // Repeated load proves the cleaned durable indexes cannot resurrect ghost sidebar rows.
        // 重复加载证明已清理的持久化索引不会复活幽灵侧栏行。
        let workspace = load_workspace_index_snapshot(fixture.controller.clone())
            .await
            .unwrap();
        assert!(
            workspace
                .sessions
                .iter()
                .any(|session| session.id == healthy_id)
        );
        assert!(
            workspace
                .sessions
                .iter()
                .any(|session| session.id == corrupt_id)
        );
        assert!(workspace.session_runtime_state.is_empty());
        assert!(workspace.hydrated_session_ids.is_empty());
        for session_id in &missing_ids {
            assert!(
                !workspace
                    .sessions
                    .iter()
                    .chain(&workspace.archived_sessions)
                    .any(|session| session.id == *session_id)
            );
            assert!(
                workspace
                    .task_projects
                    .iter()
                    .chain(&workspace.archived_task_projects)
                    .all(|project| !project.session_ids.contains(session_id))
            );
            assert!(
                locate_session_v1_storage(&fixture.controller, &session_id.to_string())
                    .await
                    .is_err()
            );
        }
        assert_eq!(std::fs::read(&corrupt_path).unwrap(), b"{corrupt");
    }
    fixture.shutdown().await;
}

/// Verifies corrupt reads are ignored, missing files remove only their index, and healthy navigation survives.
/// 验证损坏读取被忽略、丢失文件仅删除对应索引、正常导航保持有效。
/// Uses an isolated SQLite host and real session files; returns after checking durable and projected state.
/// 使用隔离 SQLite 宿主与真实会话文件；检查持久化和投影状态后返回。
#[tokio::test]
async fn automatic_model_restore_ignores_failures_and_deletes_missing_sessions() {
    // The fixture exclusively owns every file and index changed by this test.
    // 夹具独占本测试修改的全部文件与索引。
    let fixture = LifecycleStorageFixture::start().await;
    // The lifecycle fixture creates session storage only; workspace reads also require the production project schema.
    // 生命周期夹具仅创建会话存储；工作台读取还需要生产项目表结构。
    vulcan_workspace_store::data_plane::prepare_application_schema_for_test(&fixture.controller)
        .await
        .unwrap();
    // Publish a real project so the production workspace loader includes both session indexes.
    // 发布真实项目，使生产工作台加载器包含两个会话索引。
    let project = TaskProjectSummary::new(
        "project-explicit",
        "Projection test",
        fixture.controller.data_root.to_string_lossy(),
        false,
        0,
        None,
        None,
        Vec::new(),
    );
    persist_new_project(fixture.controller.clone(), &project)
        .await
        .unwrap();
    // Creation and cold restoration deliberately use different runtime registries.
    // 创建与冷态恢复刻意使用不同运行时注册表。
    let creation_registry = SessionRuntimeRegistry::default();
    // This healthy sibling must remain usable through every failure case.
    // 该正常兄弟会话必须在每种失败场景中保持可用。
    let healthy_id = SessionId::new_v4();
    // All damage is restricted to this exact session's metadata path.
    // 所有损坏均限制在该精确会话的元数据路径内。
    let damaged_id = SessionId::new_v4();
    create_fixture_session(&fixture, &creation_registry, healthy_id).await;
    // Retain the locator from the real creation path rather than guessing a storage directory.
    // 保留真实创建路径提供的定位器，不猜测存储目录。
    let damaged = create_fixture_session(&fixture, &creation_registry, damaged_id).await;
    // This is the sole exact metadata path used by both production preflight and the test.
    // 这是生产预检与测试共同使用的唯一精确元数据路径。
    let metadata_path = fixture
        .controller
        .data_root
        .join(SESSION_V1_STORAGE_PARTITION)
        .join(damaged.storage_locator().unwrap().relative_path())
        .join("metadata.json");
    // Preserve valid bytes to prove corruption handling never silently repairs old data.
    // 保留有效字节，证明损坏处理绝不静默修复旧数据。
    let original = std::fs::read(&metadata_path).unwrap();
    // Reproduce the user's exact missing required collaboration field.
    // 复现用户缺少必填 collaboration 字段的精确问题。
    let mut old_metadata: serde_json::Value = serde_json::from_slice(&original).unwrap();
    assert!(
        old_metadata
            .as_object_mut()
            .unwrap()
            .remove("collaboration")
            .is_some()
    );
    // A cold registry forces actual file reads instead of resident-memory success.
    // 冷态注册表强制真实文件读取，避免常驻内存掩盖问题。
    let registry = SessionRuntimeRegistry::default();
    for bytes in [
        serde_json::to_vec(&old_metadata).unwrap(),
        b"{broken json".to_vec(),
    ] {
        std::fs::write(&metadata_path, &bytes).unwrap();
        // The production index loader still exposes both records before optional restoration.
        // 可选恢复前，生产索引加载器仍暴露两个记录。
        let mut workspace = load_workspace_index_snapshot(fixture.controller.clone())
            .await
            .unwrap();
        workspace.selected_session = damaged_id;
        // Capturing a damaged session must return a publishable result instead of a startup error.
        // 捕获损坏会话必须返回可发布结果，而不是启动错误。
        let projection =
            read_automatic_session_model_projection_v1(&fixture.controller, &registry, damaged_id)
                .await;
        projection.apply(&registry, &mut workspace);
        assert_eq!(workspace.selected_session, SessionId::nil());
        assert!(
            workspace
                .sessions
                .iter()
                .any(|session| session.id == damaged_id)
        );
        assert_eq!(std::fs::read(&metadata_path).unwrap(), bytes);
        assert!(
            locate_session_v1_storage(&fixture.controller, &damaged_id.to_string())
                .await
                .is_ok()
        );
        // A concurrent healthy selection wins when the same capture is applied at final publication.
        // 同一捕获在最终发布时再次应用，并发正常选择仍胜出。
        workspace.selected_session = healthy_id;
        projection.apply(&registry, &mut workspace);
        assert_eq!(workspace.selected_session, healthy_id);
    }
    // An existing non-file path is a read failure, never evidence authorizing index deletion.
    // 已存在的非文件路径属于读取失败，绝非允许删除索引的依据。
    std::fs::remove_file(&metadata_path).unwrap();
    std::fs::create_dir(&metadata_path).unwrap();
    let mut workspace = load_workspace_index_snapshot(fixture.controller.clone())
        .await
        .unwrap();
    workspace.selected_session = damaged_id;
    read_automatic_session_model_projection_v1(&fixture.controller, &registry, damaged_id)
        .await
        .apply(&registry, &mut workspace);
    assert_eq!(workspace.selected_session, SessionId::nil());
    assert!(
        locate_session_v1_storage(&fixture.controller, &damaged_id.to_string())
            .await
            .is_ok()
    );
    std::fs::remove_dir(&metadata_path).unwrap();

    // Only confirmed absence removes the durable index and every corresponding snapshot reference.
    // 只有确认缺失才删除持久化索引及快照中全部对应引用。
    workspace.selected_session = damaged_id;
    workspace.hydrated_session_ids.insert(damaged_id);
    workspace
        .session_runtime_state
        .entry(damaged_id)
        .or_default();
    let missing =
        read_automatic_session_model_projection_v1(&fixture.controller, &registry, damaged_id)
            .await;
    missing.apply(&registry, &mut workspace);
    assert_eq!(workspace.selected_session, SessionId::nil());
    assert!(
        !workspace
            .sessions
            .iter()
            .any(|session| session.id == damaged_id)
    );
    assert!(
        workspace
            .task_projects
            .iter()
            .all(|project| !project.session_ids.contains(&damaged_id))
    );
    assert!(!workspace.session_runtime_state.contains_key(&damaged_id));
    assert!(!workspace.session_run_phases.contains_key(&damaged_id));
    assert!(!workspace.hydrated_session_ids.contains(&damaged_id));
    assert!(
        locate_session_v1_storage(&fixture.controller, &damaged_id.to_string())
            .await
            .is_err()
    );
    assert!(metadata_path.parent().unwrap().exists());
    // Reloading the durable index cannot resurrect a removed session.
    // 重载持久化索引不能复活已删除会话。
    let mut reloaded = load_workspace_index_snapshot(fixture.controller.clone())
        .await
        .unwrap();
    assert!(
        !reloaded
            .sessions
            .iter()
            .any(|session| session.id == damaged_id)
    );
    reloaded.selected_session = healthy_id;
    missing.apply(&registry, &mut reloaded);
    assert_eq!(reloaded.selected_session, healthy_id);
    // A normal cold session still restores its request controls without hydrating a runtime.
    // 正常冷态会话仍恢复请求控件，且不水合运行时。
    let healthy =
        read_automatic_session_model_projection_v1(&fixture.controller, &registry, healthy_id)
            .await;
    healthy.apply(&registry, &mut reloaded);
    assert_eq!(reloaded.selected_session, healthy_id);
    assert!(reloaded.session_runtime_state.contains_key(&healthy_id));
    assert!(matches!(
        registry.get(healthy_id),
        Err(SessionRuntimeError::SessionNotRegistered(_))
    ));
    drop(healthy);
    fixture.shutdown().await;
}
