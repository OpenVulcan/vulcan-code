//! Explicit release acceptance alternates real retained package sources through the production service.
//! 显式发布验收经生产服务交替使用真实保留插件包来源。

use super::*;

/// The original release plan requires one hundred actual package publications, including forward updates and rollbacks.
/// 原发布方案要求一百次实际插件包发布，包含前向更新及回滚。
const PACKAGE_PUBLICATIONS: usize = 100;

/// Alternate unchanged version-one and version-two sources through real inspection, immutable preparation and Full publication.
/// 经真实检查、不可变准备及完整发布交替使用未改动第一版和第二版来源。
/// No parameters or return value; verifies every selected digest/revision, copied business bytes and terminal owner release.
/// 无参数或返回值；校验每次选定摘要／修订、已复制业务字节及终态所有者释放。
/// This content-only acceptance does not claim Lua/Node soak coverage, which uses separate real runtime consumers.
/// 此纯内容验收不宣称 Lua／Node 长时覆盖，后者使用独立真实运行时消费者。
#[tokio::test]
#[ignore = "Explicit release acceptance: one hundred actual service package updates and rollbacks."]
async fn release_acceptance_service_one_hundred_actual_updates_and_rollbacks() {
    // The existing fixture retains actual old-source bytes separately from its installed managed predecessor.
    // 既有夹具在已安装受管前驱之外保留实际旧来源字节。
    let fixture = Fixture::new(false);
    // This exact path is the old-source directory created by Fixture::new, not a searched recovery candidate.
    // 此精确路径为 Fixture::new 创建的旧来源目录，不是搜索的恢复候选。
    let original_source = fixture.root.path().join("old-source").join(PLUGIN);
    // Both immutable input snapshots stay unchanged for the complete acceptance loop.
    // 两份不可变输入快照在完整验收循环期间保持未改动。
    let source_manifests = [
        std::fs::read(original_source.join("plugin.yaml")).expect("retained original manifest"),
        std::fs::read(fixture.source.join("plugin.yaml")).expect("retained forward manifest"),
    ];
    // The two files below are the complete content-only business contribution created by write_package.
    // 下述两个文件为 write_package 创建的完整纯内容业务贡献。
    let source_skills = [
        std::fs::read(original_source.join("skills/review-helper/SKILL.md"))
            .expect("retained original skill"),
        std::fs::read(fixture.source.join("skills/review-helper/SKILL.md"))
            .expect("retained forward skill"),
    ];
    // Private policy storage covers every production management task in this acceptance.
    // 私有策略存储覆盖此验收中的每个生产管理任务。
    let destination = fixture
        .root
        .path()
        .join("provider-release-package-cycles.json");
    // Wall time is observed from the actual loop, without inferring a per-cycle performance guarantee.
    // 墙上时间从实际循环观测，不推断每轮性能保证。
    let started = std::time::Instant::now();
    PROVIDER_CONFIGURATION_PATH
        .scope(destination.clone(), async {
            // The managed initial predecessor anchors the revision sequence; all following revisions come from the actual writer.
            // 初始受管前驱锚定修订序列；此后全部修订来自实际写入器。
            let mut previous_revision =
                VulcanPluginGenerationStore::open_existing(fixture.runtime.user_plugin_root())
                    .expect("initial store")
                    .expect("initial store exists")
                    .activation(PLUGIN)
                    .expect("initial selection")
                    .expect("initial managed selection")
                    .revision;
            // Each ordinal counts one actual production package publication, not a close request or a simulated generation.
            // 每个序号计数一次实际生产插件包发布，不是关闭请求或模拟代次。
            for publication in 0..PACKAGE_PUBLICATIONS {
                // First publish the forward source, then alternate the retained original and forward sources.
                // 先发布前向来源，再交替使用保留原来源和前向来源。
                let source_index = if publication % 2 == 0 { 1 } else { 0 };
                // Selection is a declared two-source workload, never fallback recovery discovery.
                // 选择为已声明双来源负载，绝不是兜底恢复发现。
                let source = if source_index == 0 {
                    &original_source
                } else {
                    &fixture.source
                };
                // Inspection derives the exact current input digest and complete permission approval before effects.
                // 检查在副作用之前派生精确当前输入摘要及完整权限批准。
                let preview = fixture
                    .runtime
                    .inspect_local_user_plugin(source)
                    .expect("actual package-cycle preview");
                assert_ne!(
                    fixture.runtime.snapshot().plugins[PLUGIN].content_fingerprint,
                    preview.content_fingerprint,
                    "cycle must change actual selected package bytes"
                );
                service_update_local_plugin(
                    ServiceEventEmitter::new(Vec::new()),
                    Arc::clone(&fixture.state),
                    source.to_string_lossy().into_owned(),
                    preview.content_fingerprint.clone(),
                    preview.permissions,
                    false,
                    Vec::new(),
                )
                .await
                .expect("actual service package-cycle publication");
                assert!(
                    destination.is_file(),
                    "public management task must persist to its exact private test destination"
                );
                // Read-only opening observes the real writer's selected record and preserves no extra namespace owner.
                // 只读打开观测真实写入器选定记录，不保留额外命名空间所有者。
                let store =
                    VulcanPluginGenerationStore::open_existing(fixture.runtime.user_plugin_root())
                        .expect("actual cycle store")
                        .expect("published store exists");
                // The selected record, rather than UUID or UI response, proves the new physical generation choice.
                // 选定记录证明新物理代次选择，而非 UUID 或界面响应。
                let selected = store
                    .activation(PLUGIN)
                    .expect("actual cycle selection")
                    .expect("actual selected package");
                assert_eq!(selected.revision, previous_revision + 1);
                previous_revision = selected.revision;
                assert_eq!(
                    selected
                        .generation()
                        .expect("selected physical generation")
                        .content_fingerprint,
                    preview.content_fingerprint
                );
                // This short actual catalog borrow is dropped at the end of each iteration before the next update.
                // 此短实际目录借用在每轮结束时释放，先于下一次更新。
                let catalog = fixture.runtime.snapshot();
                // Locate the exact declared plugin namespace rather than an evolving catalog position.
                // 定位精确已声明插件命名空间，而非会演进的目录位置。
                let plugin = catalog.plugins.get(PLUGIN).expect("actual cycle plugin");
                assert_eq!(plugin.content_fingerprint, preview.content_fingerprint);
                assert_eq!(plugin.runtime_state, VulcanPluginRuntimeState::Disabled);
                assert!(
                    plugin
                        .managed_package()
                        .expect("actual managed cycle package")
                        .matches_activation(&selected)
                );
                assert_ne!(&plugin.package_root, source);
                assert_eq!(
                    std::fs::read(plugin.package_root.join("plugin.yaml"))
                        .expect("actual immutable manifest"),
                    source_manifests[source_index]
                );
                assert_eq!(
                    std::fs::read(plugin.package_root.join("skills/review-helper/SKILL.md"))
                        .expect("actual immutable skill"),
                    source_skills[source_index]
                );
                // The sole live service slot must publish exactly this same actual manager catalog allocation.
                // 唯一实时服务槽必须发布精确相同实际管理器目录分配。
                let service = fixture.state.lock().expect("actual cycle service");
                // The configured bridge owns the true Full pointer, without a test substitute generation.
                // 已配置桥拥有真实完整指针，不使用测试替代代次。
                let bridge = service.agent_runtime.as_ref().expect("actual cycle bridge");
                assert!(Arc::ptr_eq(
                    &catalog,
                    &bridge
                        .extension_generation
                        .plugin_catalog()
                        .expect("actual cycle Full catalog")
                ));
                assert!(
                    bridge
                        .provider_configuration
                        .luaskills
                        .disabled_vulcan_plugins
                        .contains(PLUGIN)
                );
                assert!(
                    !bridge
                        .provider_configuration
                        .luaskills
                        .enabled_vulcan_plugins
                        .contains(PLUGIN)
                );
                drop(service);
                if (publication + 1) % 10 == 0 {
                    eprintln!(
                        "actual service package publications: {}/{}; selected revision: {}",
                        publication + 1,
                        PACKAGE_PUBLICATIONS,
                        selected.revision
                    );
                }
            }
            // Query durable final records through the actual service path, independently from returned response counts.
            // 经实际服务路径查询持久最终记录，独立于已返回响应计数。
            let requests =
                local_plugin_update_records(Arc::clone(&fixture.state))
                    .await
                    .expect("actual terminal package-cycle records");
            assert_eq!(requests.len(), PACKAGE_PUBLICATIONS);
            assert!(
                requests
                    .iter()
                    .all(|record| record.phase == Some(Phase::Completed)
                        && !record.has_service_owner),
                "every actual publication must release its original service slot"
            );
            // Frozen source bytes still remain available; rollback never rewrites old selection journals.
            // 固定来源字节仍保持可用；回滚绝不改写旧选择日志。
            for (source_index, source) in
                [&original_source, &fixture.source].into_iter().enumerate()
            {
                assert_eq!(
                    std::fs::read(source.join("plugin.yaml")).expect("unchanged input manifest"),
                    source_manifests[source_index]
                );
                assert_eq!(
                    std::fs::read(source.join("skills/review-helper/SKILL.md"))
                        .expect("unchanged input skill"),
                    source_skills[source_index]
                );
            }
        })
        .await;
    eprintln!(
        "completed {} actual service package updates/rollbacks in {:.3}s",
        PACKAGE_PUBLICATIONS,
        started.elapsed().as_secs_f64()
    );
}
