# 原生 VMM 全能力对齐与定期清洗闭环计划

## 1. 文档状态

- 状态：已完成
- 创建日期：2026-09-05
- 范围：VulcanCode 原生 VMM、外置 gRPC VMM 的边界隔离、管理界面与验证
- 上位方案：`docs/completed/20260903/03-VMM_DUAL_INTEGRATION_REVISED.md`

## 2. 任务目标

1. 以当前外置 VulcanMemoryMesh 源码为行为基线，全面核对原生 VMM 的 PreCheck、PostAction、检索、画像、项目、迁移、清空删除、资源覆盖和维护能力。
2. 补齐定期清洗完整生命周期，包括画像到期、终态记忆回收、冷回合任务、闲置会话回收、回收站到期清除、向量一致性和失败隔离。
3. 保持原生与外置 gRPC 的配置、管理责任和界面严格分离；外置模式继续由远端 VMM 管理迁移、保留与查询策略。
4. 所有修复完成后执行定向测试、全量测试和连续五轮全范围零问题审核；任何一轮发现问题均修复并从第一轮重新计数。

## 3. 技术选择

- 原生维护继续使用 Core 的强类型保留计划，由 VulcanCode 宿主负责扫描事实、串行提交 SQLite/LanceDB 一致快照和运行工作器。
- 完整保留工作器保持启动立即运行并按 30 分钟周期执行；画像到期使用独立 30 秒工作器，避免被较慢的完整清洗周期阻塞。
- 回收站保留原始向量，不在恢复时重新嵌入；Embedding 路由变化后必须先显式重建活动区和回收站中的不兼容向量。
- 永久不可执行的待分析作业必须进入可审计终态并擦除原始会话载荷；可恢复运行错误继续按有界退避重试。
- 外置 gRPC 的维护操作只展示并调用远端能力，VulcanCode 不复制其内部迁移或清洗实现。

## 4. 执行步骤

### 阶段 1：源码事实矩阵

- [x] 对照外置 VMM 与原生实现的 PreCheck 历史、噪音输入和检索策略。
- [x] 对照 PostAction 排队、重试、失败越过、画像产出和状态投影。
- [x] 对照完整保留阶段、筛选条件、批次限制、幂等与崩溃恢复。
- [x] 对照项目绑定、迁移、清空删除、用户画像与项目画像。
- [x] 对照资源语言、PII、噪音规则、提示词覆盖与热更新。

### 阶段 2：差异修复

- [x] 修复确认的功能缺口，不以兼容性猜测或静默降级替代事实链路。
- [x] 为新增状态、边界和失败路径补充 Rust 与 WebUI 测试。
- [x] 更新管理界面，使周期状态、最近结果、项目失败、回收站和向量一致性可观察且可操作。

### 阶段 3：文档与资源同步

- [x] 在本文档记录逐项对照结论、源码依据和有意差异。
- [x] 如资源权威目录发生变化，执行 `make.ps1 sync` 并验证打包产物。
- [x] 如共享宿主 AI 能力契约发生变化，通知指定协作任务；仅原生 VMM 私有实现变化不发送误报。

### 阶段 4：验证与提交

- [x] 运行 Rust 格式、检查、相关单元与集成测试。
- [x] 运行 WebUI lint、类型检查与相关组件测试。
- [x] 只暂存本任务拥有的文件并使用中文结构化提交信息创建本地提交。

### 阶段 5：连续五轮审核

- [x] 建立逐文件覆盖清单并核对所有本地提交。
- [x] 连续完成五轮不同关注点的全范围代码审核。
- [x] 任一轮发现真实问题时先修复、验证并把连续计数清零。

## 5. 验收标准

1. 原生完整清洗能自动、手动执行，并准确暴露两个工作器的运行状态、最近结果和项目级失败。
2. 所有保留阶段均有严格身份、状态、时间、版本与批次边界校验，回收和恢复不破坏向量及来源关系。
3. 永久损坏的 PostAction 作业不会无限消耗模型或永久占据活动队列，可恢复失败仍可自动重试。
4. PreCheck 只接收当前用户输入及规定的同会话历史，不引入 system/tool 噪音；PostAction 保留与外置 VMM 一致的历史语义。
5. 用户画像、项目画像、项目绑定、内容来源、迁移、清空删除和资源覆盖功能均有源码与测试证据。
6. 外置 gRPC 模式没有显示或调用原生专有控制，原生模式不依赖外置管理服务。
7. 所有本地提交均完成连续五轮零问题审核。

## 6. 对照记录

| 能力 | 外置 VMM 源码基线 | 原生实现结论 | 分类与依据 |
| --- | --- | --- | --- |
| PreCheck 当前输入 | 以当前用户输入提取意图并召回 | 只把规范用户提示词作为当前输入；不把 system/tool 当作噪音输入 | 完全一致；`NativeVmmClient::pre_check`、Core `initialize_precheck` |
| PreCheck 同会话历史 | 按同一主体、项目、会话加载近期回合，按数量与 token 预算裁剪，使用 DETAILS/RAW_TURN | 同样按固定用户、归属项目、会话加载，按相同配置限制裁剪并保持时间正序 | 完全一致；`recent_precheck_turns`、`precheck_turn_context` |
| Noise 门控 | 简单轮次执行正则与语义噪音；时间线输入跳过；区域规则与 common 组合 | 简单轮次只判定第一条 user prompt；助手输出不参与判定；语义原型也只使用 User 角色 | 原生按产品输入约束收紧；Core `initialize_intake` |
| PostAction 输入与历史 | 目标回合提供新事实，近期同会话回合作参考，工具直写只去重 | 保留目标回合、同会话历史、直接写入去重语义；提示词将 gRPC 专有字段改为宿主直接写入字段 | 完全一致并原生化命名；`post_action_timeline`、`validate_postaction_context`、六个有意修改的提示词文件 |
| PostAction 异步可靠性 | 会话队列、空闲扫描、失败退避、失败后继续处理 | 先耐久登记、原子绑定 Core 回合、后台有界重试；不可恢复失败进入 `analysis_bypassed/corrupted` 并擦除原始载荷 | 原生增强；`vmm_pending_analysis_jobs`、`NativeVmmClient::drain_due_pending_analysis_jobs` |
| 分析审计 | 外置管理面可观察会话与回合状态 | 页面按项目和会话展示回合状态、提取状态、等级、普通记忆和画像来源；成功/失败终态保留重试次数与最终诊断 | 已修复缺口；新增 `list_analysis_jobs_for_audit`，活动作业查询仍仅用于维护冲突检测 |
| 用户与项目画像 | 用户及项目画像分开维护，画像节点具有完整生命周期 | 固定原生用户 ID 1 维护唯一用户画像；每个一级项目维护独立项目画像，均支持自然语言修正及全状态查看 | 产品模型对齐；`profile_records`、`VmmProfilePage` |
| 项目模型 | team/space/project 三层 | 只有一级原生项目；ID 1 `default` 由 Schema 创建、禁止删除、允许改名；绑定按 ID 而非名称 | 有意原生差异；`build_vmm_schema_script` 及数据库触发器 |
| 默认绑定与自动创建 | 外置按可配置 team/space 前缀创建或绑定项目 | 原生全局默认可选择固定项目或按 VulcanCode 项目 tag 自动创建；已有项目在启用时补齐，项目级可覆盖归属与连带查询 | 产品模型对齐；项目目录、绑定与设置链路 |
| 召回共享与写入 | 外置共享范围由服务端配置决定 | 一个唯一归属项目负责写入，显式连带项目只扩展召回，非传递 | 原生增强控制；`ScopeAccess`、`replace_query_links` |
| 迁移 | 调用远端 VMM 自带迁移能力 | 原生迁移先预检冲突与数量，确认后移动记录并改写 scope；会话冲突按 `-1/-2` 改名；向量只改归属、不重算 | 按部署分责；`preview_migration`、`apply_migration_records` |
| 清空与删除 | 调用远端管理能力 | 清空删除项目级会话、回合、记忆、项目画像、回收站、任务和噪音缓存，保留全局用户画像；删除前列出所有归属/连带绑定，确认后解绑，ID 1 永不删除 | 产品模型对齐；`clear_project`、项目删除预检与确认 |
| 画像到期 | 外置队列维护周期调用 `ConvergeExpiredProfileNodes(256)` | 独立 30 秒工作器遍历项目，强校验主体、作用域、版本、状态与到期时间后收敛，单项目失败不阻断其他项目 | 完全一致并增强可观察性；`run_profile_expiry_worker` |
| 终态记忆回收 | 每轮先回收 superseded/deleted/expired 记忆，批次 128，保护 P1/L3 与共享项 | 同样按 128 条、有相同保护阈值和共享保护，连同情境证据移入回收批次 | 完全一致；`RetentionPolicy::v1`、`apply_retention_plan` |
| 冷回合回收 | 扫描 64、领取 16、保留最近 8 个回合，跳过仍被记忆或画像引用的回合 | 同样扫描 64、执行 16、保留热窗口 8；两阶段规划恢复外置“先扫描再领取”的次序 | 完全一致；`run_project_retention`、`recyclable_turn_ids` |
| 闲置会话回收 | 360 小时闲置阈值，批次 32，跳过 pending 和近期活动 | 使用相同阈值与批次，重新核验最后活动、pending、记忆到期和热窗口后才提交 | 完全一致；`idle_session_memory_ids`、`retention_session_latest_activity_ns` |
| 回收站 | 720 小时后按 64 批量永久清除 | 相同窗口与批次；支持列表、单批次恢复和人工永久清除；恢复前校验全部身份冲突 | 完全一致并增强人工维护；`recycle_bin`、`restore_recycle_batch`、`purge_recycle_batch` |
| 向量清理 | 热区记录回收后删除旁路向量，失败写入 128 条 GC 补偿队列 | 向量随回收批次保留以支持无重算恢复；永久清除时由同一快照提交计算陈旧向量并删除，失败由 SQLite/LanceDB 变更日志恢复 | 有意原生增强；不接受独立 `vector_gc_retry` 计划，避免重复 GC 状态机 |
| 定期调度 | 保留工作器启动立即执行，之后按配置周期；画像收敛跟随队列维护周期 | 完整清洗启动立即执行并每 30 分钟运行；画像到期独立每 30 秒运行；切出原生模式后两个工作器均停止变更 | 完全覆盖；`run_retention_worker`、`run_profile_expiry_worker` |
| 资源与语言 | 中英文五类提示词、common+区域 PII、common+区域 Noise | 49 个基线资源全部存在；六个提示词只做直接调用命名和字符串 ID 契约调整；源码权威目录同步到 output | 完全覆盖；哈希清单对比与 `make.ps1 sync` 验证 |
| 用户覆盖与热更新 | 外置 VMM 自行读取部署配置 | 原生资源在 `.vulcan/code/vmm/config` 保存用户层，不覆盖内置资源；读取时用户层优先，保存后立即激活 | 原生专有能力；`NativeVmmResourceStore` |
| 高风险编辑体验 | 外置管理端能力不由 VulcanCode 实现 | 提示词、PII、Noise 分页；当前有效文件单视图；默认锁定，解锁和保存均确认；代码高亮、行号、用途说明、PII/Noise 测试与校验 | 原生增强；`VmmNativeResourcesPage` |
| LLM/Embedding/Rerank | 外置服务自行配置 | 原生走统一宿主推理入口；所有 VMM LLM 强制关闭思考并使用结构化 JSON，Embedding 使用 ProviderDefault，Rerank 可选降级 | 原生宿主契约；`ConfiguredInferenceService` 与 Provider 请求投影 |
| 模式与管理责任 | gRPC 服务负责层级、查询、迁移、保留 | 原生页面只显示一级项目、资源与清洗；外置页面只显示外部用户/项目和远端能力，VulcanCode 不替外置实现迁移或清洗 | 严格分离；`MemoryArea` 的部署权威路由 |
| 交付形式 | 独立 Go 服务和 gRPC 协议 | 核心 Rust 源码作为 VulcanCode 独立 crates 维护，不含 FFI、Go SDK、sidecar 或托管进程 | 有意架构差异；五个 `vulcan-memorymesh-core-*` crates |

补充边界：`vmm_pending_analysis_jobs` 的终态行已擦除 `payload_json`，保留的是请求指纹、回合 ID、状态、计数和有界诊断，属于幂等与审计账本，不属于待清洗的记忆正文。当前没有经过确认的幂等失效窗口，因此不能猜测一个期限并删除该账本；本轮仅修复其审计可见性。

## 7. 执行变更总结

### 7.1 核心修复与调整概述

1. 完成原生 VMM 与外置 VMM 的职责分离。原生链路以进程内 Rust crate 直接运行，不包含 FFI、Go SDK、sidecar 或托管 `vmm-local`；外置链路继续只通过 gRPC/管理 HTTP 使用远端能力。
2. 补齐 PreCheck 当前输入与同会话历史、PostAction 耐久排队与终态审计、直接记忆读写删除、ChatCompact、用户画像、项目画像、一级项目绑定及连带召回。
3. 修复原生推理失败链路。所有 VMM LLM 场景固定关闭思考并使用结构化输出；Provider 只支持 JSON Object 时由宿主做线协议投影，最终仍按原始 Core Schema 严格校验；Embedding 使用 Provider 默认任务，Rerank 保持可选。
4. 补齐原生管理体验。内容页可按项目和会话查看提取状态、各级记忆和准确来源；画像页同时支持固定用户画像与多个项目画像；项目页支持按稳定 ID 绑定、改名、迁移、清空和删除。
5. 完成资源外置与热更新。49 份外置 VMM 基线资源迁入权威资源目录，另加一份资源清单；用户覆盖保存到 `.vulcan/code/vmm/config`，提示词、PII、Noise 分页编辑，默认锁定，解锁和保存双确认，并提供语法高亮、PII 组合/公式测试和仅 User Prompt 的 Noise 测试。
6. 完成完整定期清洗。启动立即执行并每 30 分钟运行完整周期，另以 30 秒周期收敛画像到期；覆盖终态记忆、冷回合作业、闲置会话、回收站到期清除、向量一致性、回收恢复、项目失败隔离和孤儿终态审计清理。
7. 修复迁移静止边界。迁移预检与执行均同时检查源项目和目标项目的活动分析作业，防止确认后新结果改变冲突计数或会话改名方案。

### 7.2 文件变更清单

#### 新增

- `apps/vulcan-code-web/src/components/features/memory/vmm-native-maintenance-page.tsx`
- `apps/vulcan-code-web/src/components/features/memory/vmm-native-maintenance-page.test.tsx`

#### 修改

- `apps/vulcan-code-web/src/components/features/memory/MemoryArea.tsx`
- `apps/vulcan-code-web/src/components/features/memory/vmm-advanced-page.tsx`
- `apps/vulcan-code-web/src/components/features/memory/vmm-inference-settings.tsx`
- `apps/vulcan-code-web/src/components/features/memory/vmm-inference-settings.test.ts`
- `apps/vulcan-code-web/src/components/features/memory/vmm-memory-sections.tsx`
- `apps/vulcan-code-web/src/components/features/memory/vmm-native-memory-records.tsx`
- `apps/vulcan-code-web/src/components/features/memory/vmm-native-memory-records.test.ts`
- `apps/vulcan-code-web/src/components/features/memory/vmm-native-resources-page.tsx`
- `apps/vulcan-code-web/src/components/features/memory/vmm-native-resources-page.test.tsx`
- `apps/vulcan-code-web/src/components/features/memory/vmm-profile-page.tsx`
- `apps/vulcan-code-web/src/components/features/memory/vmm-profile-page.test.ts`
- `apps/vulcan-code-web/src/i18n-messages.ts`
- `apps/vulcan-code-web/src/types/memory-vmm.ts`
- `crates/vulcan-memorymesh-core-config/src/lib.rs`
- `crates/vulcan-memorymesh-core-config/tests/pii_semantics.rs`
- `crates/vulcan-memorymesh-core-contracts/src/canonical_command.rs`
- `crates/vulcan-memorymesh-core-contracts/src/command.rs`
- `crates/vulcan-memorymesh-core-contracts/src/discriminant.rs`
- `crates/vulcan-memorymesh-core-contracts/src/domain.rs`
- `crates/vulcan-memorymesh-core-contracts/src/effect.rs`
- `crates/vulcan-memorymesh-core-contracts/src/fingerprint.rs`
- `crates/vulcan-memorymesh-core-contracts/src/retention.rs`
- `crates/vulcan-memorymesh-core-contracts/tests/canonical_command_test.rs`
- `crates/vulcan-memorymesh-core-contracts/tests/fingerprint_golden.rs`
- `crates/vulcan-memorymesh-core-engine/src/engine.rs`
- `crates/vulcan-memorymesh-core-engine/src/lib.rs`
- `crates/vulcan-memorymesh-core-storage/src/lib.rs`
- `crates/vulcan-memorymesh-core-storage/tests/storage_contract.rs`
- `crates/vulcan-memorymesh-core/src/lib.rs`
- `crates/vulcan-memorymesh-core/tests/full_public_flow.rs`
- `crates/vulcan-memorymesh-core/tests/noise_intake.rs`
- `crates/vulcan-provider-adapters/src/execution/request_encoding.rs`
- `crates/vulcan-provider-adapters/src/execution/request_encoding/tests.rs`
- `crates/vulcan-service-api/src/vmm.rs`
- `crates/vulcan-service-kernel/src/configured_inference.rs`
- `crates/vulcan-service-kernel/src/native_vmm/client.rs`
- `crates/vulcan-service-kernel/src/native_vmm/driver.rs`
- `crates/vulcan-service-kernel/src/native_vmm/maintenance.rs`
- `crates/vulcan-service-kernel/src/native_vmm/mod.rs`
- `crates/vulcan-service-kernel/src/native_vmm/prompt.rs`
- `crates/vulcan-service-kernel/src/native_vmm/resources.rs`
- `crates/vulcan-service-kernel/src/public_ai_settings/inference_settings.rs`
- `crates/vulcan-service-kernel/src/public_ai_settings/inference_validation.rs`
- `crates/vulcan-service-kernel/src/public_ai_settings/inference_validation/tests.rs`
- `crates/vulcan-service-kernel/src/service_bootstrap.rs`
- `crates/vulcan-service-kernel/src/vmm/config.rs`
- `crates/vulcan-service-kernel/src/vmm/memory_actions.rs`
- `crates/vulcan-service-kernel/src/vmm/snapshot.rs`
- `crates/vulcan-service-kernel/src/vmm/tests.rs`
- `crates/vulcan-workspace-store/src/vmm_data_plane.rs`
- `docs/completed/20260903/04-VMM_HOST_AI_PORT_CONTRACT_HANDOFF.md`

#### 资源迁移

- `crates/vulcan-runtime-assets/resources/memory/resource-manifest.toml`
- `crates/vulcan-runtime-assets/resources/memory/noise_rules/common.json`
- `crates/vulcan-runtime-assets/resources/memory/noise_rules/zh-CN.json`
- `crates/vulcan-runtime-assets/resources/memory/prompts/default_cn/postaction_l1_main.md`
- `crates/vulcan-runtime-assets/resources/memory/prompts/default_cn/postaction_l2_main.md`
- `crates/vulcan-runtime-assets/resources/memory/prompts/default_cn/precheck_l1_main.md`
- `crates/vulcan-runtime-assets/resources/memory/prompts/default_cn/precheck_l2_main.md`
- `crates/vulcan-runtime-assets/resources/memory/prompts/default_cn/profile_instruction_main.md`
- `crates/vulcan-runtime-assets/resources/memory/prompts/default_en/postaction_l1_main.md`
- `crates/vulcan-runtime-assets/resources/memory/prompts/default_en/postaction_l2_main.md`
- `crates/vulcan-runtime-assets/resources/memory/prompts/default_en/precheck_l1_main.md`
- `crates/vulcan-runtime-assets/resources/memory/prompts/default_en/precheck_l2_main.md`
- `crates/vulcan-runtime-assets/resources/memory/prompts/default_en/profile_instruction_main.md`
- `crates/vulcan-runtime-assets/resources/memory/pii_rules/ar-AE.json`
- `crates/vulcan-runtime-assets/resources/memory/pii_rules/common.json`
- `crates/vulcan-runtime-assets/resources/memory/pii_rules/da-DK.json`
- `crates/vulcan-runtime-assets/resources/memory/pii_rules/de-DE.json`
- `crates/vulcan-runtime-assets/resources/memory/pii_rules/en-AU.json`
- `crates/vulcan-runtime-assets/resources/memory/pii_rules/en-CA.json`
- `crates/vulcan-runtime-assets/resources/memory/pii_rules/en-GB.json`
- `crates/vulcan-runtime-assets/resources/memory/pii_rules/en-IE.json`
- `crates/vulcan-runtime-assets/resources/memory/pii_rules/en-IN.json`
- `crates/vulcan-runtime-assets/resources/memory/pii_rules/en-MY.json`
- `crates/vulcan-runtime-assets/resources/memory/pii_rules/en-NZ.json`
- `crates/vulcan-runtime-assets/resources/memory/pii_rules/en-PH.json`
- `crates/vulcan-runtime-assets/resources/memory/pii_rules/en-SG.json`
- `crates/vulcan-runtime-assets/resources/memory/pii_rules/en-US.json`
- `crates/vulcan-runtime-assets/resources/memory/pii_rules/es-AR.json`
- `crates/vulcan-runtime-assets/resources/memory/pii_rules/es-CL.json`
- `crates/vulcan-runtime-assets/resources/memory/pii_rules/es-ES.json`
- `crates/vulcan-runtime-assets/resources/memory/pii_rules/es-MX.json`
- `crates/vulcan-runtime-assets/resources/memory/pii_rules/es-PE.json`
- `crates/vulcan-runtime-assets/resources/memory/pii_rules/fi-FI.json`
- `crates/vulcan-runtime-assets/resources/memory/pii_rules/fr-FR.json`
- `crates/vulcan-runtime-assets/resources/memory/pii_rules/id-ID.json`
- `crates/vulcan-runtime-assets/resources/memory/pii_rules/it-IT.json`
- `crates/vulcan-runtime-assets/resources/memory/pii_rules/ja-JP.json`
- `crates/vulcan-runtime-assets/resources/memory/pii_rules/ko-KR.json`
- `crates/vulcan-runtime-assets/resources/memory/pii_rules/nb-NO.json`
- `crates/vulcan-runtime-assets/resources/memory/pii_rules/nl-NL.json`
- `crates/vulcan-runtime-assets/resources/memory/pii_rules/pl-PL.json`
- `crates/vulcan-runtime-assets/resources/memory/pii_rules/pt-BR.json`
- `crates/vulcan-runtime-assets/resources/memory/pii_rules/pt-PT.json`
- `crates/vulcan-runtime-assets/resources/memory/pii_rules/sv-SE.json`
- `crates/vulcan-runtime-assets/resources/memory/pii_rules/th-TH.json`
- `crates/vulcan-runtime-assets/resources/memory/pii_rules/tr-TR.json`
- `crates/vulcan-runtime-assets/resources/memory/pii_rules/vi-VN.json`
- `crates/vulcan-runtime-assets/resources/memory/pii_rules/zh-CN.json`
- `crates/vulcan-runtime-assets/resources/memory/pii_rules/zh-HK.json`
- `crates/vulcan-runtime-assets/resources/memory/pii_rules/zh-TW.json`

原 `crates/vulcan-memorymesh-core-config/resources/` 下上述 50 份文件均以 Git rename 迁移到权威运行资源目录，没有丢失文件，也没有额外删除。

### 7.3 关键代码调整详情

- `ConfiguredInferenceService` 继续作为共享宿主 AI 入口，统一执行路由解析、鉴权、限额、并发、超时、取消、重试、用量与诊断；VMM 只提供目的、请求内容、结构化 Schema 与稳定消费方标识。
- `NativeVmmDriver` 负责将 Core Effect 直接映射到宿主 LLM、Embedding、Rerank 和 VLDB/LanceDB；不再做 FFI 注册或模型对象互转。
- `NativeVmmClient` 负责 gRPC 协议语义到原生 Core 命令的直接适配，并提供耐久 PostAction 队列、同会话历史、固定用户及项目作用域解析。
- `NativeVmmMaintenance` 负责项目管理、迁移/清空/删除预检、来源审计、画像生命周期、向量代次、回收站及两个周期工作器。
- `VmmDataPlaneHandle` 负责 ID 1 默认项目保护、稳定项目绑定、分析作业审计查询及失去回合引用的终态审计行清理。
- WebUI 根据服务端已保存部署模式选择原生或外置页面，未保存草稿不会提前改写当前管理路由。

### 7.4 验证记录

- Core 五个 crate：232 项测试通过。
- `vulcan-workspace-store`、`vulcan-provider-adapters`、`vulcan-service-api`：1021 项测试通过，8 项按既有配置忽略。
- `vulcan-service-kernel`：933 项测试通过。
- 原生 VMM 定向测试：57 项通过；维护模块 33 项通过；VMM 数据面 7 项通过；Core 存储契约 31 项通过；Core Engine 59 项通过。
- 宿主推理定向测试：`configured_inference` 35 项、Provider 请求编码 8 项通过。
- WebUI TypeScript lint 通过；记忆模块 10 个测试文件共 33 项通过。
- `cargo fmt --all -- --check` 通过；所涉库目标 `cargo clippy --lib -- -D warnings` 通过；`git diff --check` 通过。
- `make.ps1 sync` 成功；权威目录与 `output/resources/memory` 的 50 个文件路径和 SHA-256 全部一致。
- 外置 VMM `configs` 的 49 个基线文件均被覆盖；仅 6 个 PostAction/画像提示词存在有意的原生直接调用命名差异。

### 7.5 连续五轮审核

1. 架构与模式隔离：通过。确认进程内 Rust 依赖、无 FFI/sidecar，并验证原生/外置导航及动作门禁。
2. 宿主 AI 与流水线语义：通过。确认关闭思考、结构化输出终验、Embedding/Rerank、PreCheck 历史和 User-only Noise。
3. 数据归属与项目生命周期：通过。确认固定用户、默认项目保护、稳定 ID 绑定、画像、迁移、清空、删除及来源审计。
4. 定期清洗与恢复：通过。逐值对齐外置默认策略，核对全部计划族、时间/状态/版本校验、向量一致性、回收恢复及失败隔离。
5. UI、API、资源安全、测试与提交范围：通过。确认分类编辑、双确认、路径安全、打包同步、全量测试及全部本地提交。

审核期间曾发现终态审计清洗参数存在一个 Clippy 冗余闭包提示；修复并提交后将连续计数清零，再完成上述五轮零问题审核。

### 7.6 遗留问题与注意事项

- 外置 gRPC 的查询、迁移、保留策略与运行结果仍由用户部署的 VMM 服务自行管理；VulcanCode 不读取或伪造其内部维护状态。
- 原生向量恢复保留原向量，不在普通回收恢复时重复嵌入；更换 Embedding 代次后必须使用明确预检和确认的向量重建。
- 终态无正文分析收据仍作为幂等与审计账本保留；当前没有已确认的收据失效期限，因此未擅自增加按时间删除规则。
- 仓库既有测试代码在 `cargo clippy --all-targets -- -D warnings` 下仍有与本任务无关的 `unwrap_used` 等告警；本任务涉及的库目标使用 `--lib` 严格检查已零告警。
- 全量 Provider 测试首次与多个大测试并发运行时有一项模型发现测试出现瞬时错误分类差异；该测试单独复跑及最终完整聚合复跑均通过，未修改无关 Provider 模型发现代码。
