# VMM 原生可靠性与资源配置审核记录

## 1. 审核目标

对真实桌面验证后新增的原生 VMM 修复进行累计审核：既有消息编辑身份、PostAction 重试、全用途关闭思考、结构化输出、资源覆盖、PII/Noise 测试、固定用户画像、项目画像和记忆来源维护。任何一轮发现问题并完成修复后，连续无问题轮次归零；只有连续五轮完整审核没有新增问题才结束任务。

## 2. 固定边界

- 累计范围：`6bfc5737..HEAD`，代码范围包含 `b9413dba`、`2bd9342d`、`c2e0422f`、`b7980c11`、`fb7d42d3` 五个本任务提交；最终文档提交只包含本审核记录与计划归档，并执行提交差异检查。
- 原生 VMM 与共享宿主 AI 文件属于本任务审核范围。
- 并行 VCE 私有文件不属于本任务修改范围；共享宿主 AI 契约只审核与 VMM 强制关闭思考、结构化正文和错误投影直接相关的部分。
- 本轮未修改 `crates/vulcan-runtime-assets/resources/`；Core Prompt、PII、Noise 由复制后的原生 Core crate 编译内置，用户覆盖写入 `.vulcan/code/vmm/config`，因此不触发 `make.ps1 sync`。
- 自动化环境不伪造真实 Provider 凭据；源码、确定性测试与用户提供的真实运行错误证据分别记录。

## 3. 文件覆盖台账

| 领域 | 文件 | 状态 |
| --- | --- | --- |
| 既有消息身份 | `crates/vulcan-agent-runtime/src/agent_runtime/runtime_stream_launch_payload.rs`、`workbench_prompt_turn.rs`、`workbench_prompt_turn/tests.rs`、`crates/vulcan-service-kernel/src/agent_stream/provider_v1_sink.rs`、`conversation_submission.rs`、`webui_prompt_lifecycle/pending_prepare.rs`、`precheck_continue.rs` | 待五轮审核 |
| Core 失败续体 | `crates/vulcan-memorymesh-core-engine/src/engine.rs`、`observation_tests.rs` | 待五轮审核 |
| Core 资源与 PII | `crates/vulcan-memorymesh-core-config/src/lib.rs`、`pii.rs`、`crates/vulcan-memorymesh-core/src/lib.rs` | 待五轮审核 |
| 宿主 AI 契约 | `apps/vulcan-code-web/src/types/public-ai.ts`、`crates/vulcan-provider-adapters/src/configuration.rs`、`crates/vulcan-service-kernel/src/configured_inference.rs`、`public_ai_settings/inference_settings.rs`、`inference_validation.rs`、`inference_validation/tests.rs` | 待五轮审核 |
| 原生运行时与维护 | `crates/vulcan-service-kernel/src/native_vmm/driver.rs`、`maintenance.rs`、`mod.rs`、`resources.rs`、`vmm/memory_actions.rs`、`crates/vulcan-service-api/src/vmm.rs` | 待五轮审核 |
| 原生设置界面 | `MemoryArea.tsx`、`vmm-advanced-page.tsx`、`vmm-inference-settings.tsx`、`vmm-native-resources-page.tsx`、`vmm-native-resources-page.test.tsx`、`settings-actions-service.ts`、`memory-vmm.ts`、`i18n-messages.ts` | 待五轮审核 |
| 画像与记忆来源界面 | `vmm-profile-page.tsx`、`vmm-profile-page.test.ts`、`vmm-content-page.tsx`、`vmm-native-memory-records.tsx`、`vmm-native-memory-records.test.ts` | 待五轮审核 |
| 项目级原生覆盖 | `project-settings-dialog.tsx`、`project-settings-utils.tsx`、`project-settings-utils.test.tsx` | 待五轮审核 |

## 4. 实现期新增发现

### F-R01 用户资源覆盖先无界读取再由 Core 拒绝

- 影响：单文件超过 Core 4 MiB 上限时，宿主会先把完整文件读入内存，边界校验发生得过晚。
- 修复：复用 Core 导出的单文件上限，在读取前通过元数据拒绝超限文件；同时校验配置根、最终文件和规范路径均留在安全配置根内。
- 验证：新增超限覆盖与非法配置根测试。

### F-R02 持久化成功前污染活动 Core 的配置注册名

- 影响：候选配置校验使用当前 Core 的稳定注册名；若随后磁盘写入失败，虽然活动快照未切换，但同名注册已指向未持久化候选，后续激活可能采用错误配置。
- 修复：候选配置先在隔离 Core 中编译校验；持久化成功后，才从磁盘权威状态重新构建并注册、激活活动 Core。
- 验证：资源定向测试与服务内核检查通过后纳入五轮审核。

### F-R03 排队后继用户回合未绑定 canonical message ID

- 影响：排队输入在前一回合结束后被提升为新的 `UserFollowup`，会执行 VMM 完成回合钩子；当会话已有待绑定或活动记忆状态时，因为该分支没有传入 canonical user-message ID，会触发与既有消息编辑相同的终态前置错误。
- 修复：Root 与 Queued 两种新用户回合都从各自经过复核的 V1 `input_seq` 解析 Provider 投影中的唯一 canonical ID，并以同一权威投影重建控制器状态；回合内中断、Goal 和子代理回流不进入该绑定路径。
- 验证：增加两类分发都暴露权威用户输入序号的直接测试，并执行服务内核定向测试。

### F-R04 记忆项目目录首次失败后无法原位重试

- 影响：首次目录请求返回无效结果时已写入上下文去重键，但没有项目 ID；页面刷新按钮又因缺少项目 ID 被禁用，用户在当前上下文无法重新发起目录请求。
- 修复：无效目录响应清除加载标记；刷新按钮在无项目 ID 时改为重载目录，在已有项目 ID 时仍刷新当前记录。
- 验证：纳入原生记忆页面测试、类型检查与五轮累计审核。

### F-R05 切换资源文件时短暂保留上一文件的保存权威

- 影响：资源下拉已显示新路径、异步读取尚未完成时，上一文件对象与解锁状态仍可用于保存，快速操作可能误写上一文件。
- 修复：选择新路径时同步清除上一文件、编辑文本、解锁状态和确认阶段，等待新路径的权威读取结果后才重新开放操作。
- 验证：纳入原生资源页面测试、类型检查与五轮累计审核。

### F-R06 资源覆盖操作未逐级拒绝中间重解析目录

- 影响：读取可以通过最终规范路径阻止越过配置根，但覆盖和删除只检查根与最终条目；若中间目录是 Windows Junction/重解析点，文件操作仍可能穿过非普通目录。
- 修复：创建目录时逐级创建并校验普通目录；读、覆盖、单文件删除与提示词 Bundle 删除均逐级拒绝链接/重解析目录，并复核最终规范路径仍位于配置根内。
- 验证：资源安全测试、服务内核检查及五轮累计审核。

### F-R07 全目标编译遗漏用途级思考字段

- 影响：Provider 适配器的一个测试构造仍使用旧 `InferenceProfile` 字段集合；常规库检查通过，但 `cargo clippy --all-targets` 编译测试目标时会因缺少 `llm_reasoning_efforts` 失败。
- 修复：补齐测试构造并让该用例同时覆盖已删除与保留用途的关闭思考配置。
- 验证：Provider 配置测试 55 项通过，相关库级严格 Clippy 通过。

### F-R08 删除 Provider 后残留孤立用途级思考配置

- 影响：显式删除 Provider 会移除其 LLM 路由，但新增的用途级思考值原本不会随失去对应路由的用途一并删除，可能留下无法执行的孤立配置。
- 修复：Provider 路由清理后，以仍存续的 `llm_routes` 用途键为唯一权威同步裁剪 `llm_reasoning_efforts`；保留其他 Provider 路由的思考配置。
- 验证：增强 `removing_custom_provider_clears_all_owned_state_without_touching_peers`，确认已删除用途被清理、保留用途仍为 `None`；契约变化已同步到 VCE 任务。

## 5. 连续审核记录

- 当前连续无新增问题轮次：5/5。
- 第一轮：通过。覆盖累计差异编译、Provider 配置持久化、字段生命周期、库级严格 Clippy、格式与差异检查；F-R07/F-R08 修复后从零重新开始计数。
- 第二轮：通过。覆盖 VMM 六类 LLM 用途的关闭思考资格与执行强制、结构化协议投影、调用方 Schema 终验、reasoning/text 双流隔离，以及旧会话 Root/Queued canonical ID 复用。
- 第三轮：通过。覆盖资源目录白名单、大小边界、普通目录/重解析点防护、候选配置隔离校验、完整提示词 Bundle、PII 权威公式求值，以及项目迁移、清空和向量身份保持。
- 第四轮：通过。覆盖原生/外置页面分流、固定用户画像、多项目画像、项目/会话/回合/记忆来源投影、前后端动作与类型映射；前端全量 145 个测试文件共 875 项通过。
- 第五轮：通过。重新执行累计 Rust 测试、Provider 配置测试、库级严格 Clippy、前端生产构建、Rust 格式和累计差异检查；未发现新增问题。

## 6. 真实运行证据与问题结论

- 当前桌面调试数据位于 `.vulcan-debug/code`。Provider 请求轨迹证明此前连续失败的 VMM LLM 请求虽已关闭思考，但未投影 `json_object`/Schema 控制；本轮已在支持的 OpenAI、Gemini、Google Code Assist、Cohere 协议上投影 `JsonObject`，Anthropic 保留准确 JSON Schema，宿主最后仍按调用方 Schema 校验。
- 持久队列记录证明对应回合经历两次失败后完成；Core 快照中没有普通记忆，但存在一条“用户喜欢西瓜和葡萄”的固定用户画像候选。此前页面隐藏原生用户画像入口，造成“提取成功但没有关键信息”的表象；本轮恢复固定用户画像与项目画像切换。
- L2 提供方失败时，Core 保留 L1 画像候选为 `Pending`，不会把未经 L2 评审的候选伪装成活跃画像；修复后的 JSON 模式用于避免该异常路径成为常态。
- 旧会话编辑失败源于已持久用户输入没有把权威 V1 来源序号解析回 canonical user-message ID；本轮从同一 Provider 投影解析并复用，不生成替代 ID、不重复追加历史。

## 7. 验证结果

- `cargo test -p vulcan-memorymesh-core-config -p vulcan-memorymesh-core-engine -p vulcan-memorymesh-core -p vulcan-vmm -p vulcan-agent-runtime -p vulcan-service-kernel --lib -- --test-threads=4`：1825 项通过。
- `cargo test -p vulcan-provider-adapters configuration::tests:: --lib -- --test-threads=1`：55 项通过。
- `cargo clippy` 对七个相关库目标执行 `-D warnings`：零问题。
- WebUI `vitest run`：145 个测试文件、875 项测试通过；`tsc -b` 与 Vite 生产构建通过。
- `cargo fmt --all -- --check` 与 `git diff --check 6bfc5737..HEAD`：通过。
- Provider 适配器全量测试中的既有 Anthropic 模型发现 mock 在整套运行时发生一次第二页连接竞态，单项串行复跑通过；该用例及模型发现实现不在本任务差异范围，不作为 VMM 变更问题处理。
- 本轮未修改 `crates/vulcan-runtime-assets/resources/`，因此没有执行 `make.ps1 sync`；Core 资源由原生 Core crate 编译，用户覆盖由运行时写入 VMM 数据目录。
