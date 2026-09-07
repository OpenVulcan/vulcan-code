//! Live profile policy saves verified through real natural-language instructions and durable lifetimes.
//! 通过真实自然语言指令及耐久生命周期验证联网画像策略保存。

use super::*;
use vulcan_memorymesh_core::{ProfileLevel, ProfileNodeState, StorageSnapshot};
use vulcan_service_api::{
    VmmNativeProfileLifetimeRule, VmmNativeProfileLifetimes, VmmNativeProfilePolicySummary,
};
use vulcan_vmm::{ApplyProfileInstructionRequest, ProfileTarget};

/// Provides profile-only L1/L2 output for the declared automatic lower-level extraction cases.
/// 为声明的低等级自动提炼用例提供仅含画像的 L1／L2 输出。
/// Takes a real rendered purpose and input; actual queue, history and profile persistence remain production-owned.
/// 接收真实渲染用途及输入；实际队列、历史及画像持久化仍由生产代码负责。
pub(in crate::native_vmm) fn queued_response(
    purpose: &str,
    input: &serde_json::Value,
) -> serde_json::Value {
    if purpose == "postaction_l1" {
        // The exact declared source contains its role and level as its final two tokens.
        // 准确声明来源以其最后两个词元携带角色及等级。
        let content = input["target_turn"]["raw_turn"]["user"]
            .as_str()
            .expect("HTTP profile source");
        assert!(content.starts_with("HTTP profile "));
        // Role is explicitly encoded by this test's source generator, not inferred from production ownership.
        // 角色由本测试来源生成器显式编码，不从生产归属推断。
        let role = content
            .split_whitespace()
            .rev()
            .nth(1)
            .expect("HTTP declared role");
        // Only the two actual profile types are admitted by this fixture.
        // 本夹具仅准入两个实际画像类型。
        let profile_type = match role {
            "user" => 0,
            "project" => 1,
            _ => panic!("unexpected HTTP profile role"),
        };
        return serde_json::json!({"user_input_kind":"statement","turn_id":input["target_turn"]["turn_id"],
            "details":content,"memory_nodes":[],"profile_nodes":[{"profile_type":profile_type,"content":content,
                "evidence_source":"user_asserted","admission":"keep","admission_reason":""}]});
    }
    assert_eq!(purpose, "postaction_l2");
    assert!(input.get("memory").is_none());
    // Production omits absent roles; one exact role must be present for this single-candidate fixture.
    // 生产代码省略缺失角色；本单候选夹具必须存在一个准确角色。
    let roles = ["user", "project"]
        .into_iter()
        .filter(|role| input.get(*role).is_some())
        .collect::<Vec<_>>();
    assert_eq!(roles.len(), 1);
    // The existing renderer owns this exact role partition.
    // 既有渲染器持有此准确角色分区。
    let section = &input[roles[0]];
    // Review the actual candidate index and current authorized active nodes only.
    // 仅评审实际候选编号及当前授权活动节点。
    let candidates = section["new_candidates"]
        .as_array()
        .expect("HTTP queued profile candidates");
    assert_eq!(candidates.len(), 1);
    // Candidate content carries the explicit level selected for automatic extraction.
    // 候选正文携带为自动提炼明确选择的等级。
    let content = candidates[0]["content"]
        .as_str()
        .expect("HTTP queued profile content");
    // Automatic extraction, unlike manual instructions, may retain L0 and L1.
    // 与手动指令不同，自动提炼可保留 L0 及 L1。
    let (_, level) = content.rsplit_once(' ').expect("HTTP queued profile level");
    assert!(["L0", "L1"].contains(&level));
    // Supersede only the exact matching active content supplied by the production renderer.
    // 仅替代生产渲染器提供的准确匹配活动正文。
    let active = section["active_nodes"]
        .as_array()
        .expect("HTTP queued active nodes")
        .iter()
        .filter(|node| node["content"] == content)
        .collect::<Vec<_>>();
    assert!(active.len() <= 1);
    serde_json::json!({(roles[0]):{"accepted_candidates":[{"candidate_index":candidates[0]["candidate_index"],
        "normalized_content":content,"priority":"P1","level":level,"level_reason":"Observed current profile fact.",
        "supersede_node_ids":active.iter().map(|node|node["id"].clone()).collect::<Vec<_>>() }],
        "invalid_candidate_indexes":[],"retire_only_node_ids":[],"reason":"Review the exact source-owned profile."}})
}

/// Completes a real lower-level profile extraction and validates its durable queue receipt.
/// 完成真实低等级画像提炼并验证耐久队列回执。
/// Takes the exact host, project and source text; returns only after normal analysis commits successfully.
/// 接收准确宿主、项目及来源正文；仅在正常分析成功提交后返回。
async fn analyze_profile(test: &Acceptance, project: u64, host: &str, content: &str) {
    // The operation identity is fresh while the history session is explicitly stable.
    // 操作身份为新建，会话历史身份则显式保持稳定。
    let operation = uuid::Uuid::new_v4().to_string();
    test.client
        .post_action(PostActionRequest {
            host_project_id: host.to_owned(),
            project_id: project,
            user_id: 1,
            session_id: "http-profile-analysis".to_owned(),
            operation_id: operation.clone(),
            user_content: content.to_owned(),
            assistant_content: "Acknowledged.".to_owned(),
            ..Default::default()
        })
        .await
        .expect("HTTP profile intake");
    assert!(
        test.client
            .drain_due_pending_analysis_jobs()
            .await
            .expect("HTTP profile queue drain")
    );
    // Only the actual completed receipt proves that lower-level analysis reached persistence.
    // 只有实际完成回执能证明低等级分析到达持久化。
    let job = test
        .data_plane
        .load_pending_analysis_job(&operation)
        .await
        .expect("HTTP profile receipt read")
        .expect("HTTP profile receipt");
    assert_eq!(
        job.state,
        vulcan_workspace_store::vmm_data_plane::VMM_PENDING_ANALYSIS_COMPLETED,
        "{:?}",
        job.last_error
    );
    assert_eq!(job.attempt_count, 0);
    assert!(job.payload_json.is_empty());
}

/// Stable fixture host identities also own the two independent memory projects.
/// 稳定夹具宿主身份同时持有两个独立记忆项目。
const HOST_A: &str = "http-profile-owner-a";
/// A sibling owner must keep inheriting project defaults when A is overridden.
/// A 被覆盖时，同级归属方必须继续继承项目默认值。
const HOST_B: &str = "http-profile-owner-b";

/// Returns three distinct finite rules from an explicit fixture seed; no production resolver is reused.
/// 从明确夹具种子返回三个不同有限规则；不复用生产解析器。
fn lifetimes(seed: u64) -> VmmNativeProfileLifetimes {
    // Non-round milliseconds expose unintended duration-unit conversion or rounding.
    // 非整单位毫秒暴露非预期的时间单位转换或舍入。
    let rule = |value: u64| VmmNativeProfileLifetimeRule {
        base_ms: value * 86_400_000 + 11,
        step_ms: value * 3_600_000 + 13,
        max_ms: value * 90_000_000 + 41,
    };
    VmmNativeProfileLifetimes {
        transient: rule(seed),
        situational: rule(seed + 1),
        stable: rule(seed + 2),
    }
}

/// Returns the initial or final whole global expectation, independent of HTTP response contents.
/// 返回初始或最终完整全局预期，独立于 HTTP 响应内容。
pub(super) fn global(final_values: bool) -> VmmNativeProfilePolicySummary {
    VmmNativeProfilePolicySummary {
        user: lifetimes(if final_values { 7 } else { 1 }),
        project: lifetimes(if final_values { 10 } else { 4 }),
    }
}

/// Responds to one explicit profile instruction; only actual active IDs can be superseded.
/// 响应一个明确画像指令；仅能替代实际活动 ID。
/// Takes the rendered input and returns a declared level with no fabricated dates or persisted properties.
/// 接收渲染输入并返回声明等级，不伪造日期或持久属性。
pub(in crate::native_vmm) fn response(input: &serde_json::Value) -> serde_json::Value {
    // Each instruction is the exact canonical content owned by this closed fixture.
    // 每个指令是本封闭夹具持有的准确规范正文。
    let content = input["instruction"]
        .as_str()
        .expect("HTTP profile instruction");
    assert!(content.starts_with("HTTP profile "));
    // The final token is a deliberately declared model-level choice, not a guessed business field.
    // 最后词元是刻意声明的模型等级选项，不是猜测的业务字段。
    let (_, level) = content.rsplit_once(' ').expect("HTTP declared level");
    assert!(["L0", "L1", "L2", "L3"].contains(&level));
    // Production provides the current authorized scope's active nodes.
    // 生产代码提供当前授权作用域的活动节点。
    let active = input["active_nodes"]
        .as_array()
        .expect("HTTP actual profile nodes")
        .iter()
        .filter(|node| node["content"] == content)
        .collect::<Vec<_>>();
    assert!(active.len() <= 1);
    serde_json::json!({
        "accepted_nodes":[{"normalized_content":content,"priority":"P1","level":level,
            "level_reason":"Explicit authorized profile rule.",
            "supersede_nodes":active.iter().map(|node|serde_json::json!({"node_id":node["id"],"reason":"Explicit refresh."})).collect::<Vec<_>>() }],
        "retired_nodes":[],"reason":"Apply only this explicit profile instruction."
    })
}

/// Reads the real committed relational snapshot for the supplied private environment.
/// 为传入私有环境读取真实已提交关系快照。
async fn persisted(test: &Acceptance) -> StorageSnapshot {
    serde_json::from_str(
        &test
            .data_plane
            .load_core_snapshot_json()
            .await
            .expect("HTTP profile snapshot read")
            .expect("HTTP profile snapshot exists"),
    )
    .expect("HTTP profile snapshot schema")
}

/// Creates and refreshes each finite level twice and creates one persistent node in the exact role/scope.
/// 在准确角色／作用域新增各有限等级并刷新两次，再新增一个持久节点。
/// Takes an independent policy and phase; verifies base, step, cap, immutable siblings and actual inference count.
/// 接收独立策略及阶段；验证基础值、步长、封顶、不可变同级节点及实际推理次数。
async fn exercise(
    test: &Acceptance,
    project: u64,
    host: &str,
    role: ProfileTarget,
    phase: &str,
    rules: &VmmNativeProfileLifetimes,
) {
    // Scope comes from the declared API target, never from an existing node's metadata.
    // 作用域来自声明接口目标，绝不从既有节点元数据推测。
    let (scope, role_name) = match role {
        ProfileTarget::ProfileTargetProject => (
            project_scope_id(project).expect("HTTP profile project scope"),
            "project",
        ),
        ProfileTarget::ProfileTargetUser => {
            (subject_scope_id(1).expect("HTTP fixed user scope"), "user")
        }
        _ => panic!("unexpected HTTP profile target"),
    };
    // Three editable levels and one deliberately unconfigurable persistent level use the real model protocol.
    // 三个可编辑等级及一个刻意不可配置的持久等级使用真实模型协议。
    let cases = [
        ("L0", ProfileLevel::Transient, Some(&rules.transient)),
        ("L1", ProfileLevel::Situational, Some(&rules.situational)),
        ("L2", ProfileLevel::Stable, Some(&rules.stable)),
        ("L3", ProfileLevel::Persistent, None),
    ];
    for (label, level, rule) in cases {
        // Content separates phases, roles and projects without depending on generated IDs.
        // 正文区分阶段、角色和项目，不依赖生成 ID。
        let content = format!("HTTP profile {phase} {host} {role_name} {label}");
        for weight in 0..if rule.is_some() { 3 } else { 1 } {
            // Every preceding node is compared after the real command except the exact superseded active node.
            // 真实命令后比较全部先前节点，唯独排除准确被替代的活动节点。
            let before = persisted(test).await;
            // Each operation must expose its exact production inference purposes.
            // 每个操作必须暴露其准确生产推理用途。
            let start = test
                .port
                .requests
                .lock()
                .expect("HTTP profile captures")
                .len();
            // Manual instructions enforce a P1/L2 authority floor; L0/L1 must use normal automatic extraction.
            // 手动指令强制 P1／L2 权限下限；L0／L1 必须使用正常自动提炼。
            let automatic = matches!(level, ProfileLevel::Transient | ProfileLevel::Situational);
            if automatic {
                analyze_profile(test, project, host, &content).await;
            } else {
                // Use the same public client entry as the actual application profile tool.
                // 使用与实际应用画像工具相同的公开客户端入口。
                let result = test
                    .client
                    .apply_profile_instruction(ApplyProfileInstructionRequest {
                        host_project_id: host.to_owned(),
                        project_id: project,
                        user_id: 1,
                        target: role,
                        instruction: content.clone(),
                    })
                    .await
                    .expect("HTTP actual profile instruction");
                assert_eq!(result.accepted_nodes.len(), 1);
            }
            {
                // Keep the synchronous capture lock out of all subsequent awaits.
                // 不让同步捕获锁跨过后续等待。
                let requests = test
                    .port
                    .requests
                    .lock()
                    .expect("HTTP profile capture result");
                assert_eq!(
                    requests[start..]
                        .iter()
                        .map(|request| request.purpose_id.as_str())
                        .collect::<Vec<_>>(),
                    if automatic {
                        vec!["postaction_l1", "postaction_l2"]
                    } else {
                        vec!["profile_instruction"]
                    }
                );
                if automatic {
                    assert_eq!(
                        analysis_input(&requests[start])["target_turn"]["raw_turn"]["user"],
                        content
                    );
                } else {
                    assert_eq!(analysis_input(&requests[start])["instruction"], content);
                }
            }
            // Only the committed physical representation proves lifetime persistence.
            // 只有已提交物理表示能证明期限持久化。
            let after = persisted(test).await;
            assert_eq!(
                after.dataset.profile_nodes.len(),
                before.dataset.profile_nodes.len() + 1
            );
            // The sole new active node is identified by exact content and independent scope.
            // 通过准确正文与独立作用域定位唯一新增活动节点。
            let nodes = after
                .dataset
                .profile_nodes
                .iter()
                .filter(|node| {
                    node.scope_id == scope
                        && node.content == content
                        && node.state == ProfileNodeState::Active
                })
                .collect::<Vec<_>>();
            assert_eq!(nodes.len(), 1);
            // This node must carry the actual increment, level and effective-date based expiry.
            // 此节点必须携带实际增量、等级及基于生效日期的过期时间。
            let node = nodes[0];
            assert_eq!(node.profile_level, level);
            assert_eq!(node.refresh_weight, weight);
            if let Some(rule) = rule {
                // Independent cases prove an uncapped base, an uncapped first step and a capped second step.
                // 独立分支证明未封顶基础值、未封顶首次递增以及封顶第二次递增。
                let duration = match weight {
                    0 => rule.base_ms,
                    1 => rule.base_ms + rule.step_ms,
                    2 => rule.max_ms,
                    _ => unreachable!(),
                };
                assert!(rule.base_ms + rule.step_ms < rule.max_ms);
                assert!(rule.base_ms + 2 * rule.step_ms > rule.max_ms);
                assert_eq!(
                    node.expires_at_ns,
                    Some(
                        node.effective_date_ns.expect("HTTP effective date")
                            + i64::try_from(duration).expect("HTTP finite duration") * 1_000_000
                    )
                );
            } else {
                assert!(node.expires_at_ns.is_none());
            }
            assert_eq!(after.dataset.memories, before.dataset.memories);
            assert_eq!(
                after.dataset.turns.len(),
                before.dataset.turns.len() + usize::from(automatic)
            );
            for old in &before.dataset.turns {
                assert_eq!(
                    after.dataset.turns.iter().find(|turn| turn.id == old.id),
                    Some(old)
                );
            }
            for old in &before.dataset.profile_nodes {
                if old.scope_id == scope
                    && old.content == content
                    && old.state == ProfileNodeState::Active
                {
                    continue;
                }
                assert_eq!(
                    after
                        .dataset
                        .profile_nodes
                        .iter()
                        .find(|current| current.id == old.id),
                    Some(old)
                );
            }
        }
    }
}

/// Verifies four live UI stages using newly bound projects; returns A's stable ID for physical reload checks.
/// 使用新绑定项目验证四个联网界面阶段；返回 A 的稳定 ID 以检查物理重载。
/// Takes the existing private environment and phase directory; never rewrites old profile nodes on save.
/// 接收既有私有环境及阶段目录；保存时绝不改写旧画像节点。
pub(super) async fn verify(test: &Acceptance, root: &Path) -> u64 {
    // The already assembled service owns the real maintenance handle and operation gate.
    // 已装配服务持有真实维护句柄及操作锁。
    let maintenance = test
        .service
        .lock()
        .expect("HTTP profile service")
        .native_vmm_maintenance
        .clone()
        .expect("HTTP profile maintenance");
    // Both projects are obtained from actual creation/binding receipts.
    // 两个项目均来自实际创建／绑定回执。
    let a = maintenance
        .create_and_bind_project(HOST_A, "HTTP profile A")
        .await
        .expect("HTTP profile A");
    // A sibling's rules must stay inherited throughout A's override.
    // A 覆盖期间，同级规则必须始终保持继承。
    let b = maintenance
        .create_and_bind_project(HOST_B, "HTTP profile B")
        .await
        .expect("HTTP profile B");
    // Read unrelated policy and physical content before allowing the first page save.
    // 允许首次页面保存前读取无关策略及物理内容。
    let initial = test
        .runtime
        .runtime_policy()
        .await
        .expect("HTTP profile initial policy");
    // Saves alone must never rewrite any complete committed profile node.
    // 单独保存绝不能改写任何完整已提交画像节点。
    let mut previous = persisted(test).await;
    wait_phase(root, "profile-setup").await;
    acknowledge(root, "profile-setup");
    // Independent whole-group expectations encode global changes and explicit override removal.
    // 独立整组预期表达全局变化与显式覆盖移除。
    let old = global(false);
    // Final globals change every editable field in both roles.
    // 最终全局值改变两角色全部可编辑字段。
    let new = global(true);
    // All nine overridden project values differ from both global generations.
    // 全部九个项目覆盖值均不同于两代全局值。
    let override_rules = lifetimes(13);
    for (phase, global, local) in [
        ("profile-global", &old, None),
        ("profile-project", &old, Some(&override_rules)),
        ("profile-changed", &new, Some(&override_rules)),
        ("profile-inherit", &new, None),
    ] {
        wait_phase(root, phase).await;
        assert_eq!(
            persisted(test).await,
            previous,
            "policy saves must not rewrite records"
        );
        // Actual saved state must preserve every sibling group.
        // 实际保存状态必须保留每个相邻配置组。
        let saved = test
            .runtime
            .runtime_policy()
            .await
            .expect("HTTP saved profiles");
        assert_eq!(&saved.profiles, global);
        assert_eq!(saved.recall, initial.recall);
        assert_eq!(saved.retrieval, initial.retrieval);
        assert_eq!(saved.text, initial.text);
        assert_eq!(saved.retention, initial.retention);
        // A's memory-project rule is distinct from host recall/retrieval/text overrides.
        // A 的记忆项目规则区别于宿主召回／检索／文本覆盖。
        let policy_a = test
            .runtime
            .project_profile_policy_summary(a.id)
            .await
            .expect("HTTP project A rules");
        assert_eq!(policy_a.overrides.as_ref(), local);
        assert_eq!(&policy_a.effective, local.unwrap_or(&global.project));
        // B has no override and cannot inherit A's project rule.
        // B 没有覆盖，不能继承 A 的项目规则。
        let policy_b = test
            .runtime
            .project_profile_policy_summary(b.id)
            .await
            .expect("HTTP project B rules");
        assert!(policy_b.overrides.is_none());
        assert_eq!(policy_b.effective, global.project);
        exercise(
            test,
            a.id,
            HOST_A,
            ProfileTarget::ProfileTargetUser,
            phase,
            &global.user,
        )
        .await;
        exercise(
            test,
            a.id,
            HOST_A,
            ProfileTarget::ProfileTargetProject,
            phase,
            &policy_a.effective,
        )
        .await;
        if phase == "profile-changed" {
            exercise(
                test,
                b.id,
                HOST_B,
                ProfileTarget::ProfileTargetProject,
                phase,
                &global.project,
            )
            .await;
        }
        previous = persisted(test).await;
        acknowledge(root, phase);
    }
    a.id
}
