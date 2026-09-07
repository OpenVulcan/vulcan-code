//! Exact VCE configuration compensation using the production serializer and activation transaction.
//! 使用生产序列化器及激活事务验证精确的 VCE 配置补偿。

use super::*;
use crate::tests::public_ai_persistence_scope::PUBLIC_AI_CONFIGURATION_ROOT;
use crate::tests::vce_configuration_readiness_tests::configured_state;

/// Verify late publication failure restores all configuration, or explicitly reports failed compensation.
/// 验证晚期发布失败会恢复全部配置，或明确报告补偿失败。
/// No inputs; both cases use private files and real registered models, never user credentials or storage.
/// 无输入；两个用例均使用私有文件及真实已注册模型，不使用用户凭据或存储。
#[tokio::test]
async fn vce_late_publication_failure_compensates_or_reports_exact_failure() {
    for fail_compensation in [false, true] {
        // The scoped root confines forward saves, journals and compensation to this one test case.
        // 作用域根将正向保存、日志及补偿限制在当前测试用例。
        let directory = tempfile::tempdir().expect("private configuration directory");
        // Reuse the real registered VCE route fixture, with credentials deliberately unavailable.
        // 复用真实已注册 VCE 路由夹具，凭据刻意不可用。
        let (state, runtime, mut before) = configured_state();
        // Seed exact peer consumers to detect any incidental memory or knowledge configuration mutation.
        // 播种精确的其他消费者，以发现任何附带的记忆或知识库配置修改。
        let peer_profile = before.inference_profiles[crate::vce::VCE_CONSUMER_PROFILE_ID].clone();
        before
            .inference_profiles
            .insert("vmm".into(), peer_profile.clone());
        before
            .inference_profiles
            .insert("knowledge_base:compensation-fixture".into(), peer_profile);
        runtime
            .replace_configuration(before.clone())
            .expect("install private peer profiles");
        PUBLIC_AI_CONFIGURATION_ROOT.scope(directory.path().to_path_buf(), async {
            // Match the production route-save lock order for both mutation and rollback.
            // 正向修改及回滚均遵循生产路由保存的加锁顺序。
            let _transaction = crate::begin_public_ai_configuration_transaction().await;
            vulcan_provider_config::config::save_public_ai_connections_with_custom_manifests(
                &before, directory.path(),
            ).expect("persist exact baseline");
            // These paths are solely inside the fresh private temporary directory.
            // 这些路径完全位于新建的私有临时目录中。
            let path = directory.path().join("ai-connections.toml");
            let retained = directory.path().join("candidate-before-failed-compensation.toml");
            // Capture whole-document and workbench baselines, not only the dimension being changed.
            // 捕获整份文档和工作台基线，不只比较被修改的维度。
            let before_bytes = std::fs::read(&path).expect("baseline bytes");
            let before_revision = state.lock().expect("state").workbench.revision;
            let mut candidate = before.clone();
            candidate.inference_profiles.get_mut(crate::vce::VCE_CONSUMER_PROFILE_ID)
                .expect("VCE profile").embedding_dimensions = Some(2048);
            // Inject failure only after the real durable save and runtime activation have succeeded.
            // 仅在真实持久保存和运行时激活成功之后注入失败。
            let error = mutate_public_ai_configuration_and_publish_using_in_transaction(
                &state, runtime.as_ref(), &before, "save private VCE candidate",
                |configuration| { *configuration = candidate.clone(); Ok(()) },
                |_| Ok(()),
                |published| {
                    assert_eq!(published, &candidate);
                    assert_eq!(runtime.configuration_snapshot().expect("activated candidate"), candidate);
                    let durable: vulcan_provider_config::config::PublicAiConnectionsFile =
                        toml::from_str(&std::fs::read_to_string(&path).expect("candidate bytes"))
                            .expect("candidate document");
                    assert_eq!(durable.configuration, candidate);
                    if fail_compensation {
                        // Preserve the committed candidate, then make only its target path unwritable as a file.
                        // 保留已提交候选，再使其目标路径无法作为文件写入。
                        std::fs::rename(&path, &retained).expect("retain private candidate");
                        std::fs::create_dir(&path).expect("inject private file-target obstruction");
                    }
                    Err("injected VCE workbench publication failure".into())
                },
            ).expect_err("late publication must not report success");
            assert!(error.contains("injected VCE workbench publication failure"), "{error}");
            assert_eq!(runtime.configuration_snapshot().expect("restored runtime"), before);
            assert!(crate::vce::vce_configuration_readiness(&state).status.ready);
            assert_eq!(state.lock().expect("state").workbench.revision, before_revision);
            if fail_compensation {
                assert!(error.contains("Compensation diagnostics"), "{error}");
                assert!(error.contains("restoration was incomplete"), "{error}");
                assert!(path.is_dir(), "failed disk restoration must remain observable");
                let durable: vulcan_provider_config::config::PublicAiConnectionsFile =
                    toml::from_str(&std::fs::read_to_string(&retained).expect("retained candidate"))
                        .expect("retained candidate document");
                assert_eq!(durable.configuration, candidate);
                // Remove only the empty directory created above and restore the owned test baseline explicitly.
                // 仅删除上方创建的空目录，并明确恢复本测试持有的基线。
                std::fs::remove_dir(&path).expect("remove private injected obstruction");
                vulcan_provider_config::config::save_public_ai_connections_with_custom_manifests(
                    &before, directory.path(),
                ).expect("repair isolated test baseline");
            } else {
                assert!(!error.contains("Compensation diagnostics"), "{error}");
            }
            assert_eq!(std::fs::read(&path).expect("final baseline bytes"), before_bytes);
        }).await;
    }
}
