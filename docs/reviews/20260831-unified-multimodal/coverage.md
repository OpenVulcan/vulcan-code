# 统一多模态文件覆盖台账

“已定位”仅说明已读取源码事实，不代表实现或运行验收通过。逐个实际变更文件补充条目，不用目录检查冒充文件审核。

首表保留最初定位基线；当前实施与验证状态以下方各阶段的实际文件明细为准。适配层最近完整复跑为672项通过；当前增加V1锁边界与后台编辑器，完整审核仍为0/5。

| 文件或职责 | 初始事实与边界 | 实施状态 | 验证状态 |
| --- | --- | --- | --- |
| `crates/vulcan-provider-adapters/src/configuration.rs` | 配置版本、默认值、迁移和已有配置事务；无媒体阈值字段 | 待实施 | 已定位 |
| `crates/vulcan-ai-contract/src/resource.rs`、`request.rs` | 原始媒体类别及来源；请求语义唯一归属 | 待实施 | 已定位 |
| `crates/vulcan-llm-core/src/lib.rs`、`responses_to_*` | 标准 Responses 前置编码限制音频和视频 | 待实施 | 已定位 |
| `crates/vulcan-provider-adapters/src/execution/request_encoding.rs` | 协议选择与目标投影，不能自动切协议 | 待实施 | 已定位 |
| `crates/vulcan-provider-adapters/src/execution/prepared_media.rs` | 旧 512 KiB 阈值、上传通道、缓存键 | 待实施 | 已定位 |
| `crates/vulcan-provider-adapters/src/runtime/media.rs` | 旧视频必须上传策略、扩展准备 | 待实施 | 已定位 |
| `crates/vulcan-provider-adapters/src/upload_engine.rs` | 视频内联禁止、上传硬限制、操作预检 | 待实施 | 已定位 |
| `crates/vulcan-service-kernel/src/media_upload.rs` | 草稿投递、历史与当前附件混合准备 | 待实施 | 已定位 |
| `crates/vulcan-service-kernel/src/agent_stream/provider_v1_sink.rs` | V1 最终投影重新生成内联资源 | 待实施 | 已定位 |
| `crates/vulcan-service-kernel/src/session_runtime_registry_v1.rs` | 可靠准备回调持写入者锁 | 待实施 | 已定位 |
| `crates/vulcan-agent-runtime/src/agent_runtime/provider_transport_vulcan/input_resources.rs` | 重复策略、累计预算与引用投影 | 待实施 | 已定位 |
| `crates/vulcan-agent-runtime/src/provider_input_normalizer.rs` | 简单协议类别表会错误排除文件 | 待实施 | 已定位 |
| `crates/vulcan-service-kernel/src/public_ai_settings`、配置 RPC 与客户端消费者 | 后台读取、验证、保存、快照与鉴权闭环 | 待实施 | 待逐文件登记 |
| 资源目录、附件索引与历史投递来源 | 保持 A→B→A 及稳定安全地址；不能新增平行历史权威 | 待实施 | 待补齐唯一来源证据 |
| 协议、上传、配置、V1、客户端、生产清单与夹具测试 | 不删除有效覆盖，不把零测试计为通过 | 待实施 | 待建立基线 |

## 已实际修改文件

以下均为基础层局部检查，不计入完整审核轮次。类型和测试覆盖不能替代生产资源发送接入。

| 文件 | 已实现与本次检查 | 当前验证 |
| --- | --- | --- |
| `crates/vulcan-provider-adapters/src/media_delivery.rs` | 独立用途、16项阈值、默认值、版本、宿主上限、指纹、显式重置 | 定向测试与所属包生产严格 lint 通过 |
| `crates/vulcan-provider-adapters/src/media_delivery/decision.rs` | 纯投递决策、上传硬限制优先、历史分支、精确累计预算 | 9 项单元测试通过，调用链待接入 |
| `crates/vulcan-provider-adapters/src/media_delivery/tests.rs` | 16项独立边界及多种源状态，历史不可升级上传 | 未删除旧测试；新增 `unwrap` 已修正 |
| `crates/vulcan-provider-adapters/src/configuration.rs` | 可选字段、仅内存默认、迁移读取校验 | 修改前与修改后53项均通过 |
| `crates/vulcan-provider-adapters/src/lib.rs` | 注册并导出公共策略模块 | 跨 crate 检查通过 |
| `crates/vulcan-provider-adapters/src/runtime/selection.rs` | 冻结策略，不改变清单代际及原路由指纹 | 冻结测试通过 |
| `crates/vulcan-provider-adapters/src/runtime/model_execution.rs` | 从选择复制策略，不重新读取可变配置 | 跨 crate 检查通过，发送消费待接入 |
| `crates/vulcan-provider-adapters/src/runtime/activation.rs` | 激活修改前验证精确发布及运行时策略 | 无效赋值保持旧运行态测试通过 |
| `crates/vulcan-provider-adapters/src/runtime/selection/tests/accounts.rs` | 只增加策略冻结与拒绝无效激活测试 | 新增测试通过，旧函数未改写 |
| `crates/vulcan-provider-config/src/config/configuration_io.rs` | 精确 TOML 读取与写入边界校验 | 真实持久化测试通过 |
| `crates/vulcan-provider-config/src/config/configuration_io/media_delivery_tests.rs` | 不回写默认、显式独立值、无效文件不覆盖、恢复默认 | 最后联合复跑含重置断言，2项通过 |
| `crates/vulcan-service-kernel/src/public_ai_settings/media_delivery_settings.rs` | 完整保存、显式重置、同事务发布、后端快照上限 | 最后联合复跑通过；生产严格 lint 被未修改文件阻挡 |
| `crates/vulcan-service-kernel/src/public_ai_settings.rs` | 私有模块与服务动作导出 | 检查通过 |
| `crates/vulcan-service-kernel/src/public_ai_settings/snapshot.rs` | 实际全量快照包含生效媒体设置 | 实际构建测试通过 |
| `crates/vulcan-service-kernel/src/public_ai_settings/snapshot/tests.rs` | 保留旧测试，增加零值与指纹变更验证 | 新增测试通过 |
| `crates/vulcan-service-kernel/src/rpc_dispatch/configuration.rs` | 保存、重置的单领域 RPC 所有权 | 类型检查与唯一所有权定向测试通过 |
| `crates/vulcan-service-kernel/src/rpc_dispatch.rs` | 仅增加两个媒体设置名称的唯一所有权断言 | 1 项定向测试通过 |
| `apps/vulcan-code-web/src/types/media-delivery.ts` | 精确后端契约，未定义前端默认策略 | TypeScript 检查通过 |
| `apps/vulcan-code-web/src/types/public-ai.ts` | 必需媒体快照字段与导出 | TypeScript 检查通过 |
| `apps/vulcan-code-web/src/service/settings-actions-service.ts` | 完整设置原样提交与显式重置 | 3 项动作测试通过 |
| `apps/vulcan-code-web/src/service/settings-actions-service.test.ts` | 保存、重置、错误传递；钩子误返回已修复 | 3 项通过 |
| `apps/vulcan-code-web/src/test-fixtures/media-delivery.ts` | 仅测试使用的禁用策略，无生产默认或兜底 | 相关测试通过 |
| `apps/vulcan-code-web/src/components/composer/composer-capability-controls.test.ts` | 仅补齐必需快照字段，不删旧断言 | 与设置界面合计18项旧测试通过 |
| `apps/vulcan-code-web/src/components/features/settings/custom-provider-models-state.test.ts` | 仅补齐必需快照字段，不删旧断言 | 同上 |
| 本计划及专属审核总表、文件覆盖台账 | 明确实施未完成、旧测试 lint、历史来源缺口和0/5计数 | 持续维护，未归档 |

## 内部编码与目标投影实际变更

| 文件 | 已实现与局部检查 | 验证 |
| --- | --- | --- |
| `crates/vulcan-ai-protocols/src/openai/internal_input.rs` | 新增不可序列化内部所有者、强类型资源表、只含索引的消息封装；与标准入口共用原角色/工具/续传代码 | 核心148项、2项编译期文档测试通过 |
| `crates/vulcan-ai-protocols/src/openai.rs` | 标准编码器继续严格，内部可选不同资源编码器；没有添加标准 input_video 或 input_audio | 旧标准协议回归保留并通过 |
| `crates/vulcan-ai-protocols/src/anthropic.rs` | 仅将既有图片/视频块编码器开放给同crate内部目标投影；普通/视频变体没有混用 | 生产组合测试通过 |
| `crates/vulcan-ai-protocols/src/gemini.rs` | 复用既有 Gemini 原生资源编码函数，不复制字段形状 | 生产组合测试通过 |
| `crates/vulcan-llm-core/src/internal_media.rs` | 精确Profile与来源/MIME校验、类型保持、资源表索引解析、目标映射、标准身份隔离 | 核心与适配层组合测试、生产严格lint通过 |
| `crates/vulcan-llm-core/src/internal_media/tests.rs` | 公共入口拒绝私有资源、索引越界、角色位置、游标跨协议、真实内容位置护栏 | 新增4项通过 |
| `crates/vulcan-llm-core/src/media_wire_validation.rs` | 精确目标容器、私有媒体及禁用音视频标签护栏；区分工具输出数组和不透明参数 | 核心与生产组合测试通过；并非完整供应商JSON Schema验证器 |
| `crates/vulcan-llm-core/src/lib.rs` | 注册内部入口与出口护栏；标准公开Responses转换不再直接透传私有媒体 | 核心148项通过 |
| `crates/vulcan-llm-core/src/responses_to_chat.rs` | 公共转换不接受私有资源，内部转换显式携带资源所有者 | 原角色/工具测试与媒体测试通过 |
| `crates/vulcan-llm-core/src/responses_to_anthropic.rs` | 内部资源目标精确区分普通、视频及Claude Code；保留原有角色映射 | 同上 |
| `crates/vulcan-llm-core/src/responses_to_gemini.rs` | 内联与文件URI按精确Profile映射；Interactions仍拒绝未实现远程媒体形状 | 同上 |
| `crates/vulcan-provider-adapters/src/execution/request_encoding.rs` | 移除两次标准编码，只构造一次内部语义；所有目标独立投影，供应商映射后再检查媒体出口 | 54项定向、659项完整库测试通过 |
| `crates/vulcan-provider-adapters/src/execution/tests.rs` | 注册新增生产编码回归模块 | 完整库测试通过 |
| `crates/vulcan-provider-adapters/src/execution/tests/multimodal_projection.rs` | 四种媒体×9种协议配置、混合输入、上传ID/URI、矛盾MIME、标准身份与输出限制 | 新增5项通过；不代表在线供应商验收 |
| `docs/protocol-role-projection-rules.md` | 更新内部语义基线，明确请求局部索引不是历史恢复权威 | 对照当前源码检查 |
| `docs/provider-manifest-authoring.md`、`docs/新增系统供应商流程.md` | 明确Profile消费目标线路字段、不可注入内部媒体、必须验证生产编码入口 | 旧阈值说明尚待资源准备统一后更新 |

## 独立扩展准备实际变更

本节为第三阶段局部实施覆盖，不计入完整审核。完整主会话与历史恢复尚未接入。

| 文件 | 已实现与局部检查 | 验证 |
| --- | --- | --- |
| `crates/vulcan-ai-protocols/src/media_capabilities.rs` | 精确 Profile、媒体种类、MIME 与 InlineData／RemoteUrl／ProviderFileId 的编码能力；不猜测 HostResolved | 189种实际生产编码组合与注册表一致；不代表真实模型接受能力 |
| `crates/vulcan-ai-protocols/src/lib.rs` | 原本地能力查询委托精确来源注册表，补齐文件表示能力 | 核心148项及文档2项通过 |
| `crates/vulcan-provider-adapters/src/runtime/media/model_replacement_policy.rs` | 独立用途冻结策略、模型/MIME/引用/数量/硬上限交集、明确拒绝 | 阈值、无上传小视频、接口硬上限、无效引用、预算和路由边界通过 |
| `crates/vulcan-provider-adapters/src/runtime/media.rs` | 准备前策略、缓存及读取大小冻结；去除扩展视频全禁令；真实上传才报告进度；多绑定不使用标量缓存 | 适配层完整回归通过；完整来源修订与安全错误链仍待全方案审核 |
| `crates/vulcan-provider-adapters/src/runtime/media/tests.rs` | 将旧视频强制上传测试改成四类媒体独立阈值边界，保留进度与集合覆盖 | 5项通过，未删除有效测试 |
| `crates/vulcan-provider-adapters/src/runtime/tool_targets.rs` | 新工具调用从已发布代际冻结媒体策略；观察已有任务没有新准入策略 | 代际冻结新增回归通过 |
| `crates/vulcan-provider-adapters/src/runtime/tool_targets/tests.rs` | 未发布调用者配置不能覆盖目标策略，显式激活更新新快照且旧快照保持 | 新增1项，完整库测试包含该用例 |
| `crates/vulcan-provider-adapters/src/runtime/tool_execution.rs` | 使用已解析目标策略贯穿缓存、进度和操作物化，不从旧调用参数读取 | 类型检查及完整适配层回归通过 |
| `crates/vulcan-provider-adapters/src/runtime/async_tasks.rs` | 仅为恢复观察构造明确无新准入策略状态，不改任务记录或重新上传 | 完整适配层回归通过；没有迁移持久任务结构 |
| `crates/vulcan-provider-adapters/src/execution/prepared_media.rs` | 真实上传契约附带上限，明确物化动作，有界读取和已准入大小缓存查询，缓存键与真实上传一致 | 小视频真实内联物化、缺失上传不得回退、源文件增长拒绝及旧缓存回归通过 |
| `crates/vulcan-provider-adapters/src/execution/prepared_media/tests.rs` | 保留哈希/作用域/TTL覆盖，增加按预检字节数读取与精确动作回归 | 完整适配层回归通过；新增Clippy写法已修正 |
| `crates/vulcan-provider-adapters/src/execution.rs` | 导出精确物化动作，不新增第二套准备流程 | 跨模块检查通过 |
| `crates/vulcan-provider-adapters/src/upload_engine/delivery_policy.rs` | 注册原语分类、真实接口优先、无接口大小绕过、操作种类约束、显式直通与独立策略 | 生产操作策略测试通过 |
| `crates/vulcan-provider-adapters/src/upload_engine.rs` | 全绑定策略预检、延迟解码、有界无保留形态检查、跨绑定数量限制，去除全局视频禁令 | 原有测试继续执行；最新上传定向30项通过 |
| `crates/vulcan-provider-adapters/src/upload_engine/tests/policy.rs` | 上传优先与阈值、坏契约、累计预算、编码填充、坏后项先拒绝、小视频无虚假进度、跨绑定数量 | 新增7项生产路径回归通过 |
| `crates/vulcan-provider-adapters/src/provider_manifest_fixture.rs` | 离线夹具显式取得默认独立策略，不绕过生产准备 | 适配层库测试通过；独立发布夹具门禁尚待执行 |
| `crates/vulcan-provider-adapters/src/execution/tests/multimodal_projection.rs` | 新增189种编码与预检来源能力一致性组合，复用冻结夹具缩短运行 | 新增1项通过，原5项保留 |
| `crates/vulcan-agent-runtime/src/media_understanding.rs` | 将前置单纯内联能力判断移交给运行时联合判定，避免上传后才发现引用不可消费 | 模型理解11项通过；工具说明与能力暴露完整一致性仍待接入 |
| `crates/vulcan-service-kernel/src/media_upload.rs` | 仅机械适配已有真实上传调用为显式动作及已验证大小；没有重写历史逻辑 | 服务媒体上传12项通过 |
| `docs/provider-manifest-authoring.md`、`docs/新增系统供应商流程.md` | 增加已实现独立扩展阈值和真实上传规则，明确主会话旧链仍在迁移 | 与当前源码对照；未把全方案写成已完成 |

## V1边界与后台编辑器实际变更

本节为局部实现和复核，未开始五轮完整审核。没有新增原投递持久字段，也没有将旧历史直接按新阈值重放。

| 文件 | 已实现与局部检查 | 验证 |
| --- | --- | --- |
| `crates/vulcan-service-kernel/src/session_runtime_registry_v1.rs` | 同一会话异步单一准备门；锁内捕获、锁外构建、锁内核对前沿并可靠提交；既有参数规范化测试适配异步 | 50项运行时回归通过；活动API前置拒绝、提交前沿及无虚假起始已覆盖 |
| `crates/vulcan-service-kernel/src/session_runtime_registry_v1/tests/request_preparation.rs` | 真实临时存储、锁释放、并发修改、单一准备、取消、证明伪造、构建失败、磁盘重载断言 | 新增6项含在50项通过结果内；没有真实网络 |
| `crates/vulcan-service-kernel/src/agent_stream/provider_v1_sink.rs` | 异步闭包在锁外任务中执行现有读取与编码，并等待相同所有者可靠许可 | 17项回归通过；资源策略、读取边界及历史原方式仍待迁移 |
| `crates/vulcan-agent-runtime/src/agent_runtime/events.rs` | 既有请求变化结果说明扩展到会话输入变化，不新增第二重准备通道 | 跨模块类型检查通过 |
| `crates/vulcan-agent-runtime/src/agent_runtime/provider_transport_vulcan/streaming.rs` | 前沿变化提示准确涵盖输入及配置，继续使用原重准备错误路径 | 跨模块类型检查通过；循环收敛及取消完整路径待验收 |
| `crates/vulcan-service-kernel/src/public_ai_settings/media_delivery_settings.rs` | 保存、重置必须携带预期修订，在同一配置事务内比对；严格拒绝缺失修订 | 服务媒体设置4项通过；不取消既有回滚发布 |
| `apps/vulcan-code-web/src/service/settings-actions-service.ts` | 两个媒体动作提交精确修订及完整策略，补充参数说明 | 动作测试及类型检查通过 |
| `apps/vulcan-code-web/src/service/settings-actions-service.test.ts` | 保存和重置增加修订断言，错误传递及独立值覆盖保留 | 3项通过 |
| `apps/vulcan-code-web/src/components/features/settings/media-delivery-draft.ts` | 精确后端字段草稿、独立18值、KiB十进制整数字节验证、后端上限，无前端默认值 | 长小数舍入问题已复现并修复；有效末尾零保持合法 |
| `apps/vulcan-code-web/src/components/features/settings/media-delivery-draft.test.ts` | 字段独立、零及边界、无效形态、精度、累计预算、全部字节余数往返 | 16项通过，包含3072种余数组合；最终五文件联合39项通过 |
| `apps/vulcan-code-web/src/components/features/settings/media-delivery-policy-card.tsx` | 主会话与扩展双分组，16阈值及2预算，修订冲突、保存失败保留、显式重载与默认重置 | 两项结构测试及隔离浏览器交互通过；390像素英文无横向溢出 |
| `apps/vulcan-code-web/src/components/features/settings/media-delivery-policy-card.test.tsx` | 18输入、全部精确字段、后端值/上限、零副作用及中英文说明 | 2项通过 |
| `apps/vulcan-code-web/src/components/features/settings/context-basics-settings.tsx` | 现有基础设置中挂载媒体卡，发布服务器完整快照 | 类型检查通过；卡片交互独立验证 |
| `apps/vulcan-code-web/src/i18n-messages.ts` | 24项媒体设置中英文键及封闭类型 | 类型检查、中文默认视图和英文窄屏验证通过 |
| 本计划、审核总表及覆盖台账 | 更新带修订的设置契约、两阶段边界结果、原历史证据待确认、精度问题修复 | 当前仍为未完成计划；临时浏览器4文件已删除，不留下未执行夹具 |

## 用户提示词参数与时序边界实际变更

用户已确认原身份属于用户提示词参数，早先API事件归属提案已撤回。本节只覆盖来源载体及可靠边界，不表示真实媒体消费者和A→B→A全链已完成。

| 文件 | 已实现与局部检查 | 验证 |
| --- | --- | --- |
| `crates/vulcan-session-core/src/v1/user_prompt.rs` | 版本化参数、待准备/已冻结状态、精确原路线、有序资源原方式、不透明上传回执身份及严格验证 | 核心参数回归及生产严格检查通过；不保存私有URL或媒体字节 |
| `crates/vulcan-session-core/src/v1/event.rs` | `UserInputEvent.parameters`是原身份唯一归属；旧字段缺失保持未知 | 旧序列化金样保留，加载及来源未知回归通过 |
| `crates/vulcan-session-core/src/v1/mod.rs` | 导出强类型提示词参数与绑定 | 跨模块检查通过 |
| `crates/vulcan-session-core/src/v1/document.rs` | 注册同一文档所有者的原参数冻结实现 | 核心完整测试通过 |
| `crates/vulcan-session-core/src/v1/document/user_prompt.rs` | 严格事件顺序、写一次、禁止回填历史、拒绝封口事件、批次预检后原子更新 | 7项参数及原子性用例通过；未新增第二历史系统 |
| `crates/vulcan-session-core/src/v1/document/input.rs` | 根与排队直接调度构造待准备参数；恢复时验证参数与资源位置 | 原有输入/队列测试保留并通过 |
| `crates/vulcan-session-core/src/v1/document/api.rs` | 补充输入构造原参数；API提交可同时更新对应用户事件，精确请求路线校验及幂等保护 | 核心254项、服务真实原子落盘用例通过 |
| `crates/vulcan-session-core/src/v1/document/turns.rs` | 继续及队列后继输入按真实入流边界初始化参数 | 原有继续和后继测试通过 |
| `crates/vulcan-session-core/src/v1/document/generation.rs` | 显式替换提示词成为新意图，只重置被替换提示词参数，保留此前原身份 | 新编辑回归及旧代际测试通过 |
| `crates/vulcan-session-core/src/v1/document/persistence.rs` | 仅向同文档模块开放已有活动尾段边界查询，避免重复边界算法 | 核心及存储回归通过 |
| `crates/vulcan-session-core/src/v1/projection.rs` | 宿主来源投影携带本提示词参数；公开投影不携带，直接事件范围入口也校验 | 公开语义及规范指纹不变断言通过；非法参数投影拒绝测试已补充 |
| `crates/vulcan-session-core/src/v1/document/document_tests/user_prompt.rs` | 新增7项精确归属、时序、原子、重启、编辑、不可回填及公开投影隔离测试 | 未删除旧断言，254项核心完整回归包含新增用例 |
| `crates/vulcan-session-core/src/v1/document/document_tests.rs` | 注册新增参数测试模块 | 只增加模块注册 |
| `crates/vulcan-session-core/src/v1/document/document_tests/hydration.rs` | 8处旧测试输入显式保持无参数 | 仅机械补齐字段，旧错误及加载断言不变 |
| `crates/vulcan-session-core/src/v1/tests.rs` | 3处旧序列化夹具显式保持无参数 | 原金样及严格未知字段测试保持通过 |
| `crates/vulcan-service-kernel/src/session_runtime_registry_v1.rs` | 锁外准备结果可携带精确提示词绑定，同一锁内可靠提交调用原子核心入口 | 51项运行时回归通过；生产构建结果目前尚无真实媒体绑定 |
| `crates/vulcan-service-kernel/src/session_runtime_registry_v1/tests/request_preparation.rs` | 新增原参数和API共同可靠落盘及磁盘重载验证 | 7项准备用例通过 |
| `crates/vulcan-service-kernel/src/agent_stream/provider_v1_sink.rs` | 适配新的准备载体，当前媒体绑定显式为空 | 17项流入口回归通过；生成绑定及消费恢复策略仍待实现 |
| `crates/vulcan-workspace-store/src/workspace_store/session_v1/writer.rs` | 待准备提示词在首个API/回合结束前阻止提前封口；结束后不永久占用尾段 | 先复现问题，修复后2项针对性测试及63项存储回归通过 |
| `crates/vulcan-workspace-store/src/workspace_store/session_v1/tests/user_prompt.rs` | 两项真实目录、一行滚段、冻结和旧未解析边界测试 | 2项通过；没有真实供应商调用 |
| `crates/vulcan-workspace-store/src/workspace_store/session_v1/tests.rs` | 注册独立存储回归模块，不拆分无关旧测试 | 原有套件63项通过，3项旧忽略未计为通过 |
| 两份供应商手册、计划与本审核台账 | 明确用户提示词所有权、时序及未完成消费者，移除待确认的旧API所有者方案状态 | 持续维护，完整审核仍0/5，不归档 |

## 主会话策略与回执基础追加覆盖

以下为当前实施增量；历史章节保留当时结果，最新进展以本节为准，尚未进入完整审核。

| 文件 | 本阶段变更 | 验证与边界 |
| --- | --- | --- |
| `crates/vulcan-service-kernel/src/media_upload/composer_policy.rs` | 新主会话批次规划器，冻结独立主会话策略、逐资源顺序、数量及编码预算，无预览替代 | 生产检查及媒体测试通过 |
| `crates/vulcan-service-kernel/src/media_upload/composer_policy/tests.rs` | 3项旧数量/预算/Opus测试迁移，新增四种阈值、上传偏好、硬大小、数量语义及不改标测试 | 7项，包含于最新服务媒体16项通过中 |
| `crates/vulcan-service-kernel/src/media_upload.rs` | 草稿与显式提交接入新规划器；移除旧固定阈值和已被替代函数；历史不进入新上传，当前意图独立，有界身份及缓存查询 | 16项通过；旧准备结果交接到V1原提示词仍待实现 |
| `crates/vulcan-provider-adapters/src/media_delivery/target.rs` | 主会话和模型替代共用精确模型、协议、MIME及真实上传检查，区分消息数量和单次上传数量 | 适配层完整674项及服务媒体回归通过 |
| `crates/vulcan-provider-adapters/src/media_delivery.rs` | 导出共用目标事实类型 | 类型与生产严格检查通过 |
| `crates/vulcan-provider-adapters/src/media_delivery/decision.rs`、`tests.rs` | 历史内联/上传同样检查当前真实接口硬大小 | 新回归及完整适配层测试通过 |
| `crates/vulcan-provider-adapters/src/runtime/media/model_replacement_policy.rs` | 移除重复目标判断，消费共用目标事实，保留独立用途 | 原有独立扩展测试保留并通过 |
| `crates/vulcan-provider-adapters/src/execution/prepared_media.rs`、`tests.rs` | 新公开有界源身份入口，持久引用补水和上传后校验按已知大小读取 | 新取消/增长测试及完整674项通过 |
| `crates/vulcan-workspace-store/src/workspace_store/resource_catalog.rs`、`mod.rs` | 既有绑定可选回执，目录验证回执；普通资源读取改用有界来源 | 资源目录17项通过，存储生产严格检查通过 |
| `crates/vulcan-workspace-store/src/workspace_store/resource_catalog/source.rs`、`source/tests.rs` | 元数据先行、包含关系、普通文件、精确大小及摘要，固定缓冲区与读取上限 | 3项新测试通过 |
| `crates/vulcan-workspace-store/src/workspace_store/resource_catalog/upload_receipt.rs`、`upload_receipt/tests.rs` | 既有项目所有者原子追加，不覆盖更早回执；源匹配、指纹、旧记录和错误隐私 | 4项新测试通过；真实上传生产尚未写入此回执 |
| `crates/vulcan-session-core/src/v1/user_prompt.rs` | 契约指纹严格使用真实适配器的`sha256:`格式 | 核心参数8项及完整255项通过 |
| `crates/vulcan-session-core/src/v1/document/user_prompt.rs`、`document/document_tests/user_prompt.rs` | 核心首次准备资格，拒绝历史及非开放回合，严格保持序号顺序 | 新资格测试及旧参数测试通过 |
| `crates/vulcan-service-kernel/src/session_runtime_registry_v1.rs` | 同一快照捕获首次准备资格 | 跨模块检查通过 |
| `crates/vulcan-service-kernel/src/agent_stream/provider_v1_sink.rs` | 资源前核对精确来源资格，核对冻结媒体执行路线与会话；未增加第二数据所有者 | 流入口18项通过；真实媒体绑定仍为空 |
| `crates/vulcan-agent-runtime/src/agent_runtime/events.rs`、`provider_transport_vulcan.rs`、`provider_transport_vulcan/streaming.rs` | 实际适配器把同一冻结执行传给V1候选，调试输出不打印秘密 | 生产跨模块检查通过 |
| `crates/vulcan-agent-runtime/src/agent_runtime/provider_transport_stream_runtime.rs`、`runtime_stream_delta.rs` | 仅状态测试夹具明确无媒体上下文 | 类型检查通过，未删除旧断言 |

## 真实媒体消费、宿主地址与逐出现路线追加覆盖

此节覆盖最新实施增量；旧章节保留当时结果，不代表当前仍为空绑定。完整审核尚未开始。

| 文件或明确文件组 | 本阶段变更 | 验证与边界 |
| --- | --- | --- |
| `crates/vulcan-service-kernel/src/agent_stream/provider_v1_sink/media.rs` | 按原事件及资源顺序规划、统一预算、真实物化、原回执恢复、逐提示词绑定及安全附件说明 | 7项真实准备测试通过；草稿来源交接尚未完成 |
| `crates/vulcan-service-kernel/src/agent_stream/provider_v1_sink/media/tests.rs` | 内联切换、原上传/过期恢复、同文件多prompt、同prompt不同原模型、累计预算、损坏源及新上传失败 | 无真实上传；凭据失败在网络前注入，不删除旧测试 |
| `crates/vulcan-service-kernel/src/agent_stream/provider_v1_sink.rs` | 在同一两阶段准备中调用真实媒体消费者；原绑定不再为空；预算控制器接收实际附件事实 | 最后完整入口25项通过，包含7项真实媒体测试 |
| `crates/vulcan-session-core/src/v1/user_prompt.rs` | 区分首次请求路线及`resources[].original_route`，逐资源严格验证 | 完整核心256项通过；参数仍只属于用户prompt |
| `crates/vulcan-session-core/src/v1/document/document_tests/user_prompt.rs` | 同prompt多原模型按位置持久化，重载不变，单项改写也原子拒绝 | 9项提示词测试包含于完整核心256项 |
| `crates/vulcan-service-kernel/src/session_runtime_registry_v1/tests/request_preparation.rs`、`crates/vulcan-workspace-store/src/workspace_store/session_v1/tests/user_prompt.rs` | 原有真实可靠落盘及分片夹具显式填入逐资源原路线 | 7项真实准备、2项分片测试通过，原断言保留 |
| `crates/vulcan-workspace-store/src/workspace_store/resource_catalog/source.rs`、`source/tests.rs`及导出模块 | 精确项目定位符格式化/解析；严格ID边界，不接受别名 | 资源目录18项通过；地址不含物理项目路径 |
| `crates/vulcan-service-kernel/src/session_media_resources_v1.rs`、`tests.rs`及服务`lib.rs` | 宿主端口按调用会话已注册定位器解析项目资源，返回原名称/MIME/大小/摘要；安全错误 | 2项真实目录测试通过；相同ID跨项目隔离 |
| `crates/vulcan-agent-runtime/src/media_resource_runtime.rs`及`lib.rs` | 只读、仅宿主持有资源端口，元数据先行，不接受模型选择项目 | 跨模块检查及真实服务解析测试通过 |
| `crates/vulcan-agent-runtime/src/agent_runtime/execution_models.rs`、`execution_preparation.rs`、`provider_tool_bridge.rs`及服务`webui_prompt_lifecycle/precheck_continue.rs` | 冻结准备请求携带资源端口；固定专用/模型代替工具消费；既有子执行克隆继承 | 类型检查与固定媒体理解测试通过；动态模型扩展尚未接通 |
| `execution_preparation/tests.rs`、`prompt_compaction.rs`、`runtime_stream_launch_payload.rs`、`task_runtime/tests/preparation_config.rs`、`task_runtime/tests/runtime_flow.rs`、`workbench_prompt_turn.rs`（均在代理运行时）及服务`git/commit_summary.rs` | 无媒体的既有构造显式不安装端口；主执行由服务注入 | 机械补齐新字段，未改断言及工具用途 |
| `crates/vulcan-agent-runtime/src/media_understanding.rs`、`media_understanding/tests/project_resources.rs` | 精确项目地址解析、原名称校验、强类型私有元数据旁路、不可变源证明，更新AI输入说明 | 固定媒体理解13项通过，其中2项新桥接测试 |
| `crates/vulcan-provider-adapters/src/operation_media_source.rs`及`lib.rs` | 宿主持有来源及资源上下文类型，不实现JSON反序列化 | 适配层严格检查通过；不把完整宿主来源记录塞入工具参数 |
| `crates/vulcan-provider-adapters/src/execution/prepared_media.rs`、`tests.rs`及`execution.rs` | 显式已准入来源携带原文件名/MIME/摘要；缓存核对摘要；上传前后精确验证 | 完整适配层676项通过；新增同大小内容漂移断言 |
| `crates/vulcan-provider-adapters/src/runtime/media.rs`、`tool_execution.rs`、`upload_engine.rs` | 专用操作在宿主侧消费来源证明，全部预检后才读取/上传，实际内联及Multipart流校验摘要 | 数量误用按单次上传语义修正后，上传引擎32项、适配层676项及生产严格检查通过 |
| `crates/vulcan-provider-adapters/src/upload_engine/tests/policy.rs`、`provider_manifest_fixture.rs` | 显式无宿主来源的旧夹具保留；新增真实无扩展名来源、错误证明和独立数量语义 | 旧错误/策略断言保留或按权威Schema修正；不伪造上传进度 |
| `crates/vulcan-service-kernel/src/media_upload.rs` | 上传及缓存调用交付已确认大小/摘要/原名称，不重新从变化文件推断身份 | 服务媒体16项通过 |
| `crates/vulcan-provider-adapters/src/media_delivery/decision.rs` | 区分新上传准备失败和历史原引用不可用，给出准确安全未发送原因 | 新V1失败注入回归通过，无Base64回退 |
| 计划第19.4节、审核总表与两份供应商手册 | 更新真实消费者、逐出现路线、项目地址支持及明确未完成入口 | 完整审核仍0/5，不归档，不改用户配置或运行时产物 |

## 草稿交接与持久身份追加覆盖

本节为最新实现增量；上节草稿移交未实现的描述保留为历史阶段记录。当前仍有纯项目草稿、重试及其他入口未完成，不能视作端到端验收。

| 文件或明确文件组 | 本阶段变更 | 验证与剩余边界 |
| --- | --- | --- |
| `crates/vulcan-agent-runtime/src/attachment_asset_index/draft_media.rs`、`draft_media/tests.rs` | 活动租约首次意图，单次原回执，精确源/路线/契约预检，迟到完成拒绝 | 4项新增，含于索引27项通过 |
| 同目录上层`attachment_asset_index.rs`、`types.rs`、`lifecycle.rs` | 原意图字段及导出；租约释放、协调、消息提升清理对应原意图，不修改其他来源 | 资产索引27项通过 |
| `crates/vulcan-session-core/src/v1/user_prompt.rs`、`document/user_prompt.rs` | 有序可选已准备项，首次冻结不覆盖，初始证明供幂等重试比较 | 13项提示词测试含于完整260项 |
| `crates/vulcan-session-core/src/v1/config.rs`、`event.rs` | 草稿/排队/补充输入保留参数；草稿增加逐出现暂存身份；排队决议带原证明 | 元数据真实序列化/重载、旧字段缺失不回填 |
| `crates/vulcan-session-core/src/v1/document/input.rs`、`turns.rs`、`api.rs` | 准入及幂等校验、队列生命周期参数传递、仅正文编辑保留、参数前置验证 | 完整260项通过，非法待处理证明失败原子性回归 |
| `crates/vulcan-session-core/src/v1/document/document_tests/user_prompt.rs`、`input.rs` | 新准入/排队/重复资源/文本编辑/失败原子性覆盖，旧夹具显式缺省字段 | 未删除有效原断言 |
| `crates/vulcan-service-kernel/src/media_upload.rs`、`media_upload/composer_policy.rs` | 草稿槽及意图绑定，A→B地址→A恢复、原内联可逆；原待上传固定方式；移除被替代提交入口及死代码 | 媒体16项及V1附带输入区路线矩阵通过，真实上传未执行 |
| `crates/vulcan-service-kernel/src/rpc_dispatch/conversation_runtime.rs` | 规划RPC要求精确草稿槽 | 类型检查；准备RPC使用服务签发计划内保存的槽，不信任临时新槽 |
| `crates/vulcan-service-kernel/src/conversation_submission.rs`、`webui_prompt_lifecycle/pending_prepare.rs` | 不再旧预上传；托管草稿来源注册到提示词；排队传递参数 | 生产检查；已释放租约重试及纯项目恢复仍未闭环 |
| `crates/vulcan-service-kernel/src/session_v1_lifecycle.rs` | 完整批次证明验证、资源注册、私有回执追加、有序参数交接 | 新真实控制器集成1项通过，独立重载验证原用户事件 |
| `crates/vulcan-service-kernel/src/agent_stream/provider_v1_sink/media.rs`、`media/tests.rs` | 待准备提示词消费较早逐项原事实并保留；新增真实输入区矩阵调用 | V1入口27项通过，不伪造上传或凭据读取 |
| `crates/vulcan-service-kernel/src/conversation_drafts_v1.rs`、`session_runtime_registry_v1.rs` | 规范暂存ID随草稿可靠保存、响应按位置输出，仍使用既有单一文档所有者 | 服务检查及物理准备7项通过；完整草稿RPC浏览器验收仍待补 |
| `crates/vulcan-service-api/src/session_drafts_v1.rs` | 明确有序可空暂存ID响应，不含私有回执 | 类型/生产严格检查通过 |
| `crates/vulcan-service-kernel/src/pending_prompts.rs`、`session_goal_lifecycle/state_cache.rs`、`session_runtime_registry_v1/tests/request_preparation.rs` | 既有构造及调用补全明确空参数，不猜旧来源 | 定向测试及检查通过 |
| `apps/vulcan-code-web/src/service/conversation-actions-service.ts`、`components/composer/composer-panel/use-composer-attachment-preparation.ts` | 规划携带草稿槽，仅执行真正待上传且阻塞的计划，不重新执行已恢复回执 | 类型检查及既有附件测试通过 |
| `apps/vulcan-code-web/src/service/conversation-session-drafts-v1.ts`、对应测试文件 | 严格解析逐出现暂存ID，恢复显式托管身份，拒绝无效数量/标识及缺失权威元数据 | 与附件/串行写入共19项通过；不以相同ResourceId合并原意图 |
| 计划19.5、审核总表、供应商作者手册、角色来源约定 | 更新实际实现、验证和未完成边界 | 完整审核0/5，不归档 |

## 精确重试与未知继承追加覆盖

| 文件或明确文件组 | 当前增量 | 验证与边界 |
| --- | --- | --- |
| `crates/vulcan-session-core/src/v1/document/input.rs` | 精确客户端动作的不可变输入查询，跨已声明生命周期校验唯一所有者/原证明；精确资源出现解析；陈旧待准备不变成新意图 | 提示词15项、完整262项及生产严格检查通过 |
| `crates/vulcan-session-core/src/v1/user_prompt.rs`、`mod.rs` | 精确来源引用和宿主准备结果；未知继承位置、构造及验证、冻结仅地址约束和初始身份比较 | 新已知/未知/新意图组合与矛盾位置覆盖；WebUI生产来源传输未完成 |
| `crates/vulcan-session-core/src/v1/document/document_tests/user_prompt.rs` | 查询跨队列/补充/冻结/草稿修改；重复所有者拒绝；同ResourceId不同原模型；未知继承和旧API前沿 | 新测试及原有效断言全部保留 |
| `crates/vulcan-service-kernel/src/conversation_submission.rs` | 既有会话先找精确原输入；已接纳重试不访问活动草稿租约；正文冲突拒绝 | 编译及生产检查，浏览器确认丢失整链仍待验收 |
| `crates/vulcan-service-kernel/src/session_v1_lifecycle.rs` | 重试沿原输入参数，精确资源顺序校验，不追加新回执；真实落盘冻结后的幂等集成 | 集成1项通过；不是无会话ID新会话重试或源已清理的证明 |
| `crates/vulcan-service-kernel/src/agent_stream/provider_v1_sink/media.rs`、`media/tests.rs` | 未知继承在新提示词中也仅地址；原仅附件/未知/跨模型上传不查询新上传契约或来源 | V1入口28项通过，新测试无凭据读取或供应商调用 |
| `crates/vulcan-service-kernel/src/session_runtime_registry_v1/tests/request_preparation.rs` | 既有夹具显式无未知位置，未改变准备原子性断言 | 随服务测试编译通过 |
| 计划19.6及审核记录 | 区分已实现核心/既有会话重试和未接通的纯项目输入区、新会话确认丢失等边界 | 完整审核仍0/5 |

## 纯项目草稿与混合附件追加覆盖

| 文件或明确文件组 | 当前增量 | 验证与边界 |
| --- | --- | --- |
| 核心`v1/config.rs`、`document/input.rs`及`document_tests/user_prompt.rs`、`input.rs` | 持久有序来源引用、排队恢复锚点、重排/增删重建、双重身份拒绝及原子预检 | 完整263项通过，新重排用例含实际序列化重载 |
| 服务API`attachments.rs`、`session_drafts_v1.rs` | 项目附件来源及草稿有序来源契约，不暴露原回执 | 前后端类型检查通过 |
| 服务`conversation_drafts_v1.rs`、`conversation_resources_v1.rs`、`attachments/conversion.rs`、`session_runtime_registry_v1.rs`及`tests/request_preparation.rs` | 元数据重建、来源传递、混合资源注册及草稿保存 | 服务定向测试；完整浏览器RPC流程待验收 |
| 服务`session_v1_lifecycle.rs`、`conversation_submission.rs` | 区分暂存和项目来源；仅注册新暂存，保留既有ID及各自原证明 | 真实交接1项通过，缺失实体故障注入仍保留元数据；联合体检查修正待复跑 |
| 服务`media_upload.rs`、`media_upload/composer_policy.rs`及`provider_v1_sink/media.rs`、`media/tests.rs` | 纯项目规划、未知保护、精确回执恢复、共用完整回执校验和真实状态投影 | V1入口28项通过，含扩展输入区A/B矩阵 |
| 服务`attachments/staging.rs`、`conversation_submission.rs` | 跳过纯项目暂存租约；三个入口共用精确所选槽位校验 | 新所有者正反测试待复跑 |
| 前端`service/composer-attachment-reference.ts`及测试、`conversation-session-drafts-v1.ts`及测试 | 统一视图身份与严格来源解析，源位置独立于当前显示位置 | 前端4文件22项及类型检查通过 |
| 前端`composer-panel/use-composer-attachment-preparation.ts`及测试、`composer-panel/helpers.ts`、`composer-panel.tsx`、`composer-attachment-controls.tsx`、`types/composer-skills.ts` | 规划签名、本地草稿、删除、重复资源身份；仅真正阻塞待上传计划可取消 | 不伪造项目资源上传，完整浏览器流程待验收 |
| 计划19.7及审核总表 | 记录实际实现、局部验证和剩余闭环 | 未开始完整审核，0/5 |

## 动态声明项目资源追加覆盖

| 文件或明确文件组 | 当前增量 | 验证与边界 |
| --- | --- | --- |
| 适配层`operation_media_source.rs`、`operation_media_source/tests.rs`及`lib.rs` | 宿主只读解析端口、声明绑定解析、精确身份拒绝、去重元数据但不去重预算 | 新4项通过；不递归扫描未声明字段 |
| 适配层`upload_engine/inputs.rs`及`upload_engine.rs` | 解析和物化共用声明遍历器；所有形态预检；权威MIME同样受绑定格式约束 | 完整681项通过，保持数组/条目输出及旧失败断言 |
| 适配层`upload_engine/tests/policy.rs` | 不存在源上的MIME前检、Data URL不能扩大格式、固定MIME原优先级 | 新1项及完整适配层通过 |
| 适配层`runtime/tool_execution.rs` | 同一冻结Operation内解析，前置于凭据并受共同截止时间；已解析与即时解析两种显式宿主模式互斥 | 编译、完整适配层及生产严格检查通过；真实供应商未调用 |
| 代理`media_resource_runtime.rs`及服务`session_media_resources_v1.rs`、对应测试 | 通用目录元数据解析、固定工具额外模态检查、调用会话绑定适配 | 固定桥14项、真实端口3项通过，五类MIME不重新标记 |
| 代理`provider_tool_bridge.rs`、`media_understanding/tests/project_resources.rs` | 动态执行传递端口、模型描述及参数发现说明；规范ID/原MIME/原摘要桥接 | 扩展桥15项及代理生产严格检查通过 |
| 两份供应商手册、方案19.8、审核总表 | 更新已实现项目地址和原有精确来源；记录旧地址、压缩/捕获/非会话等剩余边界 | 完整审核0/5，无配置、运行产物或Git提交变更 |

## 旧托管地址与元数据前置追加覆盖

| 文件或明确文件组 | 当前增量 | 验证与边界 |
| --- | --- | --- |
| 代理`attachment_asset_index/replay.rs`及导出模块 | 共同所有权/路径/大小元数据解析；新增严格ID和固定模态前置入口；旧完整校验仍核对摘要 | 索引28项，同大小损坏前置不读、后续完整校验拒绝 |
| 代理`media_resource_runtime.rs`、`provider_tool_bridge.rs`、`media_understanding.rs`及`tests/project_resources.rs` | 动态两种定位符分权解析，托管输入要求准确消息所有权；固定理解元数据先行 | 理解及桥接15项，原项目范围和模态边界保留 |
| 适配层`operation_media_source.rs`、对应测试及`runtime/tool_execution.rs` | 同一声明遍历支持精确项目/托管定位符，统一解析方法及准确截止时间诊断 | 4项声明解析、完整适配层681项及生产严格检查通过 |
| 两份手册、方案19.9及审核总表 | 更新已实现旧地址支持和未完成消费者 | 完整审核0/5 |

## 纯预构建、只读压缩与捕获追加覆盖

| 文件或明确文件组 | 当前增量 | 验证与边界 |
| --- | --- | --- |
| 代理`provider_transport_vulcan/input_resources.rs`及测试 | 纯安全元数据、五类附件及重复出现；去掉旧文件读取、最新缓存、逆序及固定预算 | 10项；Base64大小及来源格式断言使用真实共同策略；完整传输40项通过 |
| 代理`provider_transport_vulcan.rs`、`request.rs`、`streaming.rs`、`message_projection.rs`及测试 | 移除预构建旧资源约束字段；请求组装准确身份/位置映射；远程压缩消费原prompt只读投影 | 传输40项及生产严格检查通过；完整压缩计划集成仍待验收 |
| 代理`prompt_media_projection.rs`、`lib.rs` | 宿主冻结有序来源端口，只读重放与无上传捕获两种明确模式 | 非序列化权威，不增加用户可伪造字段 |
| 服务`provider_v1_sink/media.rs`、`media/readonly.rs`、`provider_v1_sink.rs` | 共用准备器显式分派/只读/捕获模式；相同来源和阈值；禁止新上传及原身份写入 | V1入口33项通过；新增口径不改变用户事件参数权威 |
| 服务`provider_v1_sink/media/tests/readonly.rs`及上级测试 | 原上传A/B恢复、原内联、未知Pending、窗口预算、逐消息数量、源顺序、会话隔离、新内联捕获与新上传拒绝 | 5项；无额外凭据读取或新增目录回执；追加历史捕获和矛盾快照断言后的只读3项复跑通过 |
| 服务`context_compaction/auto_compaction.rs`、`streaming.rs`、`conversation_run_services.rs` | 手动/自动计划与实际来源映射同快照，冻结既有目录所有者，启动参数传递 | 服务测试编译通过；不复读较新会话或最新文件缓存 |
| 代理`prompt_compaction.rs`；适配层`runtime/model_execution.rs` | 媒体与远程压缩执行共用同一已解析执行引用，兼容原拥有值调用者 | 代理及适配层生产严格检查通过；任务入口来源仍待接入 |
| 代理`runtime_request_capture.rs`、`provider_runtime_dispatch.rs` | 可选来源端口沿实际捕获调用链传递，普通调用兼容 | 生产检查；仓库内尚无业务捕获消费者 |
| 代理`provider_transport_vulcan/capture_media.rs`及测试 | 位置/角色/未知/遗漏原子预检；含附件缺少来源时明确失败，纯文本仍可捕获 | 3项含6种失败注入、系统前缀与字典序差异，含于传输40项 |
| 两份供应商手册、方案19.10及审核总表 | 记录实际能力、原测试迁移和未完成消费者 | 完整审核0/5，未归档 |

## V1上传生命周期追加覆盖

| 文件或明确文件组 | 当前增量 | 验证与边界 |
| --- | --- | --- |
| 适配层`runtime/conversation_media.rs`及测试（新增） | 精确冻结执行的会话上传、原契约比较、共用期限、局部取消及安全错误 | 9项；含跨阶段总时限、不让出最终轮询、同次轮询取消、丢弃后台取消；真实HTTP状态分类仍待补 |
| 适配层`runtime/model_execution.rs`、`runtime.rs` | 冻结既有观察器及准确选择代际/路线修订，注册会话上传模块 | 适配层/代理生产严格检查，代理传输40项通过 |
| 适配层`runtime/media.rs`、`media.rs`及旧测试夹具 | 共同终态守卫的局部取消；串行进度队列、终态封闭、锁外可重入回调及连续字节合并 | 旧测试保留；迟到和可重入用例含于新9项 |
| 适配层`execution/prepared_media.rs`及两处上传契约夹具 | 精确通道总超时纳入强类型契约，不猜默认值 | 实际Manifest字段来源；已有硬限额和原回执指纹不变 |
| 服务`provider_v1_sink/media.rs`及`media/tests.rs` | 新上传走冻结运行时方法；既有缓存查询、原顺序和原用户参数保持不变 | 完整V1入口35项通过 |
| 服务`media/tests/upload_lifecycle.rs`（新增） | 实际V1凭据失败发出Preparing/Failed且保留附件；新意图缓存和历史恢复无事件及无凭据读取 | 2项通过，使用真实目录/规划器/观察器桥，未访问供应商 |
| 前端`workspace/media-upload-progress-state.ts`及测试、`workbench-layout.tsx`、`i18n-messages.ts` | 按明确操作与失败终态提示附件保留；取消不报失败；工作台版本及通知ID去重 | 进度3项与类型检查通过；整链浏览器验收待补 |
| 两份供应商手册、方案19.11及审核总表 | 区分已实现上传局部期限、未完成整回合收敛和HTTP分类 | 完整审核0/5，不归档 |

## 上传强类型失败和HTTP链路追加覆盖

| 文件或明确文件组 | 当前增量 | 验证与边界 |
| --- | --- | --- |
| 适配层`media_failure.rs`（新增）、`lib.rs` | 封闭错误事实、真实HTTP状态/阶段/时限/来源/引用及限额，安全显示边界 | 不保留响应正文、秘密或本地路径；完整695项通过 |
| 适配层`upload_engine.rs` | 各上传原语使用同一状态先行响应读取器，非成功响应不先读正文，声明引用和硬限制强类型返回 | 新回环5项，旧测试改为准确枚举及数值断言 |
| 适配层`upload_engine/tests/http_failures.rs`（新增） | 实际Multipart/文件创建状态矩阵、坏/停滞正文、正常引用、签名401及成功正文超时 | 5项通过，仅严格回环；规范路径夹具已修正，不放宽生产检查 |
| 适配层`execution/prepared_media.rs`及对应测试 | 增加强类型入口；原字符串入口只在外层显示转换，原来源变化及无回退行为保留 | 实际来源变化断言、完整适配层与生产严格检查通过 |
| 适配层`runtime/conversation_media.rs`及测试 | 不压平上传失败，超时保留准确通道，进度显示由类型生成 | 生命周期9项通过，原取消/迟到/总期限覆盖保留 |
| 服务`provider_v1_sink/media.rs` | 当前资源出现的失败说明随安全元数据给模型；原用户参数、资源顺序及历史投递不变 | V1入口37项通过 |
| 服务媒体测试上下文及`upload_lifecycle/http.rs`（新增） | 复用既有受限客户端注入和TLS/DNS夹具，真实V1注册→上传→回执/附件/进度验证 | HTTPS2项通过；成功保存真实引用，失败不产生成功回执 |
| 两份供应商手册、方案19.12及审核总表 | 更新已接通HTTP事实，记录仍需修复的文件创建内存放大及剩余完整目标 | 完整审核0/5 |

## 文件创建流式正文追加覆盖

| 文件或明确文件组 | 当前增量 | 验证与边界 |
| --- | --- | --- |
| 适配层`upload_engine/file_create_body.rs`（新增） | 48KiB增量编码、精确JSON长度、原字段覆盖、源长度/摘要、取消及独立强类型完成 | 本地文件无完整字节/Base64/JSON分配；原已有内联来源只保留必要拥有值副本 |
| 适配层`upload_engine/file_create_body/tests.rs`（新增） | 多种填充/分块边界、实际字节进度、转义、同长度损坏、增长/截短、取消、丢弃和超长前置拒绝 | 6项通过；不删除或降低原完整性门槛 |
| 适配层`upload_engine.rs` | 文件创建消费流式正文和准确长度，真实源失败从完成通道传回；HTTP成功仍要求来源完成 | 适配层完整703项及生产严格检查通过；HTTP状态优先语义保留 |
| 适配层`upload_engine/tests/http_failures.rs` | 新增实际JSON/长度/进度、多块及600001字节上传、来源失败和提前200拒绝；修正夹具残留内联来源 | 新增2项通过，原5项保留；合计7项真实HTTP回归 |
| 服务`provider_v1_sink/media/tests/upload_lifecycle/http.rs` | 真实HTTPS成功增加Uploading原始字节断言，不只依赖完成态 | V1入口37项通过；原目录回执和失败附件验证保留 |
| 两份供应商手册、方案19.13及审核总表 | 更新实际流式上传能力，区分接口Base64封装与上下文内联，保留整个准备流程未完成边界 | 完整审核0/5，不归档、不提交 |

## 准备取消及有限重启追加覆盖

| 文件或明确文件组 | 当前增量 | 验证与边界 |
| --- | --- | --- |
| 服务`agent_stream/stream_task.rs`、`stream_task/prepared.rs` | 统一重启状态沿真实模型/策略/自动压缩入口传递，可靠API许可后重置 | 服务编译与严格检查仅余旧基线；无新的会话所有者或持久字段 |
| 服务`stream_task/restart_state.rs`及测试（新增） | 可靠许可前最多自动重启3次，耗尽使用原精确失败所有权，不重置压缩守卫 | 6项类型状态机回归；既有准备失败所有权3项通过，浏览器热修改整链未计通过 |
| 服务`provider_v1_sink/media.rs`、媒体测试模块与`media/tests/cancellation.rs`（新增） | 规划前建立取消，真实规划器/物化入口/排队回执检查；内联工作线程协作取消 | 新增2项，实际V1入口39项通过；未声明整个准备已具有统一绝对期限 |
| 存储`resource_catalog/source.rs`及测试 | 保留旧API，新增有界分块可取消读取，增量摘要及取消丢弃部分内容 | 新增2项；项目资源20项及存储生产严格检查通过 |
| 适配层`execution/prepared_media.rs`及测试 | 每个来源I/O阶段响应等待中取消，准入长度加一字节限制，同次轮询取消与安全错误 | 新增4项；适配层完整707项，适配层/代理生产严格检查通过 |
| 两份供应商手册、方案19.14及审核总表 | 记录实现、局部证据及尚未完成的预算/期限/整链边界 | 完整审核0/5，不归档、不提交 |

## 实际前缀预算追加覆盖

| 文件或明确文件组 | 当前增量 | 验证与边界 |
| --- | --- | --- |
| 服务`provider_v1_sink/media.rs` | 完整预检后逐实际结果推进预算/数量；资格与原历史归属独立；原回执再次检查；捕获逐项禁止新上传 | V1入口46项通过；保留同一冻结目标和精确选中通道 |
| 服务`media/tests/budget.rs`（新增）、`media/tests.rs` | 失败内联/上传后的编码及数量释放，跨提示词/逐提示词边界，原证明保护，捕获正/反权限，回执有效性 | 新增7项通过；真实读取/凭据失败注入，捕获与只读端口对照正式内容 |
| 服务`media/tests/cancellation.rs` | 明确仅地址夹具没有原生重规划资格 | 原取消测试保留于V1入口46项 |
| `session_runtime_registry_v1.rs`、对应`tests/request_preparation.rs`、核心`v1/user_prompt.rs`与`document/user_prompt.rs`（本轮只读） | 核实前沿失效早于prompt原身份提交；现有两阶段7项没有覆盖已上传后丢弃的原证明保留 | 新发现列为下一优先修复，未据源码分析冒充真实并发验收 |
| 两份供应商手册、方案19.15及审核总表 | 更新实际预算与捕获语义，明确首次准备失效/取消时原证明保留仍未完成 | 完整审核0/5，不归档、不提交 |

## 逐资源完成参数追加覆盖

| 文件或明确文件组 | 当前增量 | 验证与边界 |
| --- | --- | --- |
| 核心`v1/user_prompt.rs`、`v1/mod.rs` | 部分完成状态、逐资源绑定、严格形状/不可变初始证明校验及状态精确读取 | 核心269项及生产严格检查通过 |
| 核心`v1/document/user_prompt.rs`、`input.rs` | 精确代际/出现写入、幂等、最终冻结保留、动作查询优先真实运行时结果、来源继承 | 未放宽旧历史或新上传资格；真实上传生产者尚未接入 |
| 核心`document_tests/user_prompt.rs`与`user_prompt/partial_completion.rs`（新增） | 6项回归：位置/重试/重载、替换拒绝、旧历史边界、非法补水、队列提升、取消后继承 | 6项及全部269项通过，原有效测试保留 |
| 存储`session_v1/writer.rs`及`tests/user_prompt.rs` | 部分状态开放窗口阻止封片，真实落盘重载后冻结并恢复滚动 | 新增1项，相关存储3项通过 |
| 服务`provider_v1_sink.rs`、`media.rs`及`media/tests.rs` | 准备前沿接受未冻结部分状态，统一消费Core逐出现原结果 | V1入口48项通过，服务生产编译通过 |
| 服务`media/tests/partial_completion.rs`（新增） | A上传→B地址→A精确原引用，A内联→受支持B内联；无新凭据读取 | 新增2项通过；使用已保存部分状态夹具，不冒充真实并发上传验收 |
| 两份供应商手册、方案19.16及审核总表 | 明确完成范围与取消安全生产交接仍未接通 | 完整审核0/5，原完整目标保持活动 |

本节最终核心269项、V1入口48项复跑通过，核心/存储严格检查及格式/差异空白检查通过；服务严格检查仍仅有未修改`workbench_refresh.rs:400`旧诊断，不计通过。未修改适配层及代理运行时生产逻辑，未重跑其旧全量门禁。

## 已知完成生产交接追加覆盖

| 文件或明确文件组 | 当前增量 | 验证与边界 |
| --- | --- | --- |
| 服务`session_runtime_registry_v1.rs`及新增`request_preparation.rs` | 原单飞门自有租约、逐出现可靠完成、自身/外部修订分离、正式快照重捕获及严格最终门 | 准备11项和思考参数提升1项通过，不手改旧证明、不放宽外部修订拒绝 |
| 服务`provider_v1_sink.rs`、`media.rs` | 真实媒体逐项提交，已知上传先原证明后私有回执，租约跨全部提交；编码用真实重捕获 | V1入口49项通过，默认线程栈；未知上传选择尚未前置持久声明 |
| 服务`conversation_run_services.rs`、`stream_task.rs` | Stop及失败终态先等待同一准备门，让完成工作线程先结算 | 真实所有者取消/停止顺序回归；完整RPC/浏览器停止矩阵未计通过 |
| 服务`session_runtime_registry_v1/tests/request_preparation.rs`及新增`request_preparation/completion.rs` | 保留旧7项，增加自身完成、四类外部修订时点、取消后结算、伪造重捕获拒绝 | 共11项通过，实际磁盘重载验证，无删除有效测试 |
| 服务`media/tests/upload_lifecycle/http.rs`及新增`http/durable_completion.rs` | 实际HTTPS上传、成功时真实切模型、prompt原证明落盘、A/B/A恢复和凭据计数 | 新增1项，默认栈通过；临时大栈仅作诊断，不作验收 |
| 通用构建和媒体外围Future | 修复新增整链暴露的有限大状态默认栈溢出；保持父拥有和丢弃取消 | 真实整链增加外围帧小于4KiB断言，未改运行时线程栈配置 |
| 两份供应商手册、方案19.17及审核总表 | 更新真实接通范围，记录未获回执上传选择及其余全目标边界 | 完整审核0/5，不归档、不提交 |

## 上传选择及无回执取消追加覆盖

| 文件或明确文件组 | 当前增量 | 验证与边界 |
| --- | --- | --- |
| Core`v1/user_prompt.rs`、`document/user_prompt.rs` | 原上传待回执状态、通道/契约校验、原运行时唯一合法前向完成 | 完整272项、生产严格检查通过；导入证明不由新生产者猜测补齐 |
| Core`document_tests/user_prompt.rs`及新增`user_prompt/upload_selection.rs` | 3项选择/重载/取消/完整性/不可替换/历史及导入边界回归 | 原269项保留；含无效引用格式原子拒绝 |
| 服务`provider_v1_sink/media.rs`、`request_preparation.rs` | 上传前可靠记录选择；失效不启动新上传；失败保留原选择；历史待回执不新上传 | V1入口50项、准备11项；实际预算保持按内容而非原方式计数 |
| 服务`media_upload/composer_policy.rs` | 输入区准确区分外模型原上传和原引用不可用 | 真实取消回归调用实际规划器验证，不上报就绪或重新上传 |
| 服务媒体既有失败/预算/进度测试 | 当前仅地址与原上传选择分别断言，保留失败及安全内容检查 | 无删除有效测试 |
| 服务`media/tests/upload_lifecycle/http.rs`及新增`http/cancelled_selection.rs` | 回复前闸门、完整正文到达后取消、原选择已落盘、Stop、模型切换及无关回执保护 | 新增1项真实HTTPS回归；生产准备被中止且服务器确定结束，无外部供应商 |
| 代理`attachment_asset_index/draft_media.rs`及测试 | 无回执草稿转prompt保留原上传选择，保留原只写一次及移除重加保护 | 附件索引28项、项目交接1项及代理严格检查通过；晚到回执跨提交仍待闭环 |
| 两份供应商手册、方案19.18及审核总表 | 区分V1取消已验证与草稿晚到回执/共同期限/浏览器未验收 | 完整审核0/5，不归档、不提交 |

## 草稿晚到回执交接追加覆盖

| 文件或明确文件组 | 当前增量 | 验证与边界 |
| --- | --- | --- |
| Core`v1/user_prompt.rs`、`document_tests/user_prompt/upload_selection.rs` | 可选原草稿意图关联、UUID校验、拒绝新V1生产者改写导入选择；紧凑装箱文本不改变线路字符串 | 新增1项；首次严格检查发现枚举体积问题后修复，Core严格检查通过 |
| 存储`resource_catalog.rs`、`resource_catalog/upload_receipt.rs`及测试 | 私有回执按意图或原指纹精确匹配；同意图首次回执唯一，冲突发布和补水拒绝，旧缺省字段不变 | 新增1项；目录回执5项通过，存储严格检查通过 |
| 代理`attachment_asset_index.rs`、`types.rs`、`lifecycle.rs`、`draft_media.rs`及新增`draft_media/handoff.rs` | 同一资产索引锁登记项目交接，活动原意图或已登记目标接纳晚到完成，资产锁与项目锁不嵌套 | 生产严格检查通过；不增加全局会话权威，不放宽原活动租约接口 |
| 代理`draft_media/tests.rs`及新增`draft_media/tests/handoff.rs` | 三种完成顺序、幂等、替换拒绝、移除重加有无交接、错误模型及真实磁盘重载 | 新增3项，附件索引31项通过；保留正常无引用清理 |
| 服务`session_v1_lifecycle.rs`及新增`session_v1_lifecycle/tests/draft_handoff.rs` | 实际prompt注册登记真实项目目标；真正创建根会话、释放租约、晚到完成及独立注册表重载 | 新增1项，项目prompt2项通过；没有在上传规划猜测默认项目 |
| 服务`media_upload.rs`、`media_upload/composer_policy.rs`、`provider_v1_sink/media.rs` | 草稿生产完成自有工作线程、请求及输入区共用精确回执匹配、关联成功算恢复而非新上传 | V1入口51项通过；完整RPC取消及故障矩阵未计通过 |
| 服务`media/tests/upload_lifecycle/http.rs`及新增`http/draft_handoff.rs` | 真实HTTPS回复跨准入/租约提升/B首次API冻结；A精确恢复、历史不变、无关较新引用及凭据计数 | 新增1项通过，默认线程栈、真实隔离TLS，无外部供应商或推理 |
| 两份供应商手册、方案19.19、审核总表及覆盖表 | 记录新增生产路径、局部审核修复、真实验证范围及未完成验收 | 完整审核0/5；紧凑表示变更后的受影响测试已复跑 |

本阶段最终复跑：Core及存储完整测试合计470项通过、5项忽略，附件索引31项、V1入口51项、项目prompt2项、物理准备11项通过；Core/存储/代理生产严格检查、格式、差异空白检查通过。服务严格检查仅有未修改`workbench_refresh.rs:400`既有问题，不计通过。总体工作量估计85%，其余完整目标不变；没有重跑前端全套、适配层707项或浏览器完整矩阵。

## 上传准备共同期限追加覆盖

| 文件或明确文件组 | 当前增量 | 验证与边界 |
| --- | --- | --- |
| 适配层`lib.rs`及新增`media_upload_deadline.rs`、`media_upload_deadline/tests.rs` | 冻结通道/时长、同一绝对终点、取消优先、不取消父请求、紧凑堆所有阶段 | 新增3项，适配层严格检查通过 |
| 适配层`runtime/conversation_media.rs`及测试 | 接收已有期限的生产入口；原进度所有者保留，来源阶段耗时不会被网络阶段重置 | 新增2项，原9项保留，适配层完整712项通过 |
| 服务`provider_v1_sink/media.rs` | 来源身份、选择等待、缓存及真实上传共用期限；已确认完成发布在等待之外 | V1完整53项及准备并发11项通过；默认线程栈 |
| 服务`media_upload.rs` | 草稿路径/索引有界等待、来源/缓存/凭据/网络共用期限；取消由父令牌决定 | 媒体相关16项通过；全部RPC取消时点仍待验收 |
| 服务`http.rs`、`http/draft_handoff.rs`及新增`http/shared_deadline.rs` | 更新实际调用签名，真实HTTPS区分总期限和错误重置，确认后超期不丢回执 | 新增2项，真实A/B/A交接旧回归仍保留 |
| 两份供应商手册、方案19.20、审核总表及覆盖表 | 记录有效范围、默认栈修复及未完成的全局生命周期矩阵 | 工程估计86%，完整审核0/5，不归档 |

最终712/53/16/11项均为本阶段实跑。服务生产编译通过、严格检查仅有无差异的既有`workbench_refresh.rs:400`诊断；格式及差异空白检查通过。未声称本轮重新运行全部Core、存储或前端验收。

## 取消登记及结算确认追加覆盖

| 文件或明确文件组 | 当前增量 | 验证与边界 |
| --- | --- | --- |
| 适配层`media.rs` | 取消令牌精确操作身份比较，区别共享信号与相同布尔值 | 新增1项，适配层完整713项及生产严格检查通过 |
| 服务`media_upload.rs`及新增`media_upload/registration.rs` | 唯一登记守卫、退出清理、已知发布持有、同ID替换保护；请求取消与实际结算分离 | 服务生产编译通过，正常/异常退出不依赖分散手工清理 |
| 新增`media_upload/registration/tests.rs` | 中止、取消等待、替换、已知结果、实际RPC分发、发布错误/异常六项回归 | 新增6项；媒体相关合计22项通过，V153项通过 |
| 服务`rpc_dispatch/conversation_runtime.rs`及前端`conversation-actions-service.ts` | 原取消RPC等待真实结算；布尔响应和请求形态不变，同步说明语义 | 实际RPC处理端已验证，不宣称完整浏览器/传输层通过 |
| 前端`use-composer-attachment-preparation.ts`（本轮只读） | 确认路线变化取消原上传、忽略旧代际完成，以及取消失败后仍继续租约队列 | 两项未完成边界写入方案19.21，不能计作已验收 |
| 两份供应商手册、方案19.21及审核记录 | 区分后端已修复与前端仍未闭环的取消/代际事实 | 仍约86%、完整审核0/5，不归档、不扩大拆分 |

最终713/22/53项为本轮实跑；适配层严格检查、格式及差异空白检查通过。服务严格检查仍仅无差异既有`workbench_refresh.rs:400`诊断，未放宽规则或计为通过。

## 前端取消失败及显式移除追加覆盖

| 文件或明确文件组 | 当前增量 | 验证与边界 |
| --- | --- | --- |
| `composer-panel/use-composer-attachment-preparation.ts`及原测试 | 原操作取消观察、失败保留、真实确认屏障、跨路线错误投影、租约布尔确认 | 新增1项队列回归；原7项保留，整体原在途上传保留尚未实现 |
| `composer-attachments.ts`、`composer-panel.tsx`及新增`use-composer-attachment-removal.ts` | 等取消及租约后按对象移除，保护并发新增/替换和草稿槽 | 显式移除已测；待发送提示词编辑整组替换未计通过 |
| 新增`composer-attachment-cancellation.hook.test.tsx` | 真React状态/effect、生产附件准备及移除Hook，服务边界替身 | 12项通过，包含多附件失败隔离与跨路线错误保留；不是完整浏览器或真实网络 |
| `conversation-actions-service.ts`及新增`conversation-cancellation-service.test.ts` | 区分真实后端无登记、服务不可用与确认丢失 | 新增3项通过，调用参数及布尔RPC形态不变 |
| 前端`package.json`与`pnpm-lock.yaml` | 测试专用`jsdom 27.4.0`，版本要求与现有构建环境一致 | 不执行依赖安装脚本，不新增生产依赖 |
| 两份供应商手册、方案19.22及审核记录 | 区分显式移除已验证与A/B/A、整组替换等未验收 | 约86%，完整审核0/5，不归档 |

本阶段实跑前端全量126个测试文件、768项通过，类型检查及差异空白检查通过。无Rust源码变更、无Rust复跑；不把历史严格检查既有诊断计作已修复。

## 原上传单次执行及视图观察追加覆盖

| 文件或明确文件组 | 当前增量 | 验证与边界 |
| --- | --- | --- |
| `media_upload.rs`及新增`media_upload/draft_execution.rs` | 原意图共享一次许可、首个实体启动、后续观察、明确重试、原执行与当前视图约束分层 | 生产编译通过；未取得全部真实准备RPC与网络整链证据 |
| `media_upload/registration.rs` | 实体所有者退出清理共享令牌别名，观察者退出不取消原操作 | 既有媒体22项保留并通过 |
| `media_upload/composer_policy.rs` | 原上传不可用降级后退还原生数量额度，不影响内联字节预算 | 新生命周期入口覆盖退还后后续数量上限 |
| 新增`draft_execution/tests.rs`及V1媒体测试入口 | 多视图归并、一次启动、晚到观察、丢弃RPC、取消等待、权限取消、原所有者隔离 | 新增1项多分支真实登记测试；V1入口最终54项通过，无真实外部供应商 |
| `rpc_dispatch/conversation_runtime.rs`及服务附件DTO | 独立丢弃API、明确重试参数、服务生成原实体进度标识 | 实际丢弃RPC已验证，不把它等同完整规划/准备RPC |
| 前端附件准备Hook、附件类型及动作包装 | 路线切换不取消原操作、B地址/A观察、进度身份分层、陈旧计划只丢弃 | 新增A/B/A真实Hook；服务为隔离替身 |
| 前端Hook及服务边界测试 | 更正幂等租约布尔语义，覆盖无服务错误、丢弃与取消区分、明确重试权限 | 全量126文件/772项及类型检查通过 |
| 两份供应商手册、方案19.23及审核记录 | 纠正旧语义，区分已测单飞/Hook与待验收实际来源和整链 | 约87%，完整审核0/5，不归档、不提交 |

当前格式、差异检查与后端生产编译通过；服务严格检查仍只有无差异的既有`workbench_refresh.rs:400`诊断，不计通过。本阶段没有重跑适配层713项或宣称完整浏览器验收。

## 实体来源寿命追加覆盖

| 文件或明确文件组 | 当前增量 | 验证与边界 |
| --- | --- | --- |
| 新增`attachment_asset_index/source_hold.rs`及模块导出 | 进程内逐资产读取计数，最后读取者退出后复用原安全删除 | 不持久化为草稿/回执，不是媒体历史账本 |
| `attachment_asset_index/storage.rs` | 已存在根规范化为同一物理索引锁身份 | 新增Windows实际别名回归；不存在根无可持有来源，保留原路径行为 |
| `draft_media.rs`及新增`draft_media/tests/source_hold.rs` | 活动草稿/原UUID/完整来源原子校验，双读取、替换、错误输入及并发竞争 | 新增5项，资产索引完整37项通过 |
| `lifecycle.rs`、`cleanup.rs`及索引测试 | 草稿协调、孤立删除、预览/执行清理识别实体读取；持久消息仍保留 | 不新增删除路径或放宽托管路径校验 |
| 服务`media_upload/registration.rs`、`media_upload.rs`及新增来源测试 | 实体持有跨网络/取消/Future丢弃/已知发布，先释放再确认结算 | 新增2项；服务媒体24项、V1入口54项通过 |
| 两份供应商手册、方案19.24及审核记录 | 区分取消登记、持久草稿引用和实体来源持有 | 约88%，完整审核0/5，不归档、不提交 |

代理生产严格检查、后端生产编译、格式及差异空白检查通过。服务严格检查仅旧`workbench_refresh.rs:400`诊断且该文件无差异，不计通过。本阶段未改前端，未把历史前端结果计为新验证。

## 待发送提示词事务及跨存储发布恢复追加覆盖

| 文件或明确文件组 | 当前增量 | 验证与边界 |
| --- | --- | --- |
| 服务API`subagents.rs`、服务`pending_prompt_services.rs` | 强类型已提交编辑结果；提交前完成可失败准备，提交后面板/事件故障只形成警告 | 新增强类型故障单测；快照投影失败不再冒充持久移动失败 |
| 前端`session-actions.ts`、动作服务与类型 | RPC提交结果和后续工作台刷新分层，刷新失败仍返回精确条目 | 类型检查及完整前端回归通过 |
| 前端`use-composer-session-draft.ts`、发送控制器、输入区及队列控件 | 较早草稿写入刷新、owner绑定、同步互斥门、完整替换、中途会话切换隔离及确认丢失权威恢复 | 新增真实Hook时序与恢复测试；不合并正文或附件 |
| 前端`composer-attachments.ts`及原取消Hook测试 | 异步附件暂存代际失效，完整替换后晚到结果不得追加 | 原测试扩展1项，前端最终127文件/775项通过 |
| 代理`draft_media/handoff.rs`测试 | 项目目录发布失败后保留资产意图已接纳回执，并以相同身份恢复精确发布 | 新增1项确定性磁盘故障回归；交接13项通过 |
| 方案19.25与审核记录 | 记录本轮精确范围、验证及仍未完成整链 | 约91%，完整审核0/5，不归档、不提交 |

本阶段本地验证不包含真实浏览器服务和外部供应商网络。压缩窗口媒体投影、无会话来源边界、确认丢失生产重试和完整规划/准备RPC仍按方案继续；不把React Hook、服务单测与本地目录故障合并声称为整链通过。

## 压缩窗口及非会话出口门追加覆盖

| 文件或明确文件组 | 当前增量 | 验证与边界 |
| --- | --- | --- |
| 代理`message_projection.rs`、`prompt_compaction.rs`与服务上下文压缩入口 | 精确会话范围、选定消息窗口、冻结只读媒体投影及原路线恢复 | 只读媒体3项、消息投影3项、上下文压缩9项通过；不产生新上传 |
| 代理`events.rs` | 非会话许可拒绝`HostResolved`及任何本地路径，只放行已准备标准来源 | 新增1项来源门测试；最终协议出口仍负责格式与硬限制 |
| 服务普通流、子会话与提交摘要非会话分支 | 传播强类型来源门错误，不再无条件原样分派候选 | 服务生产编译通过；文本专用调用行为不变 |
| 方案19.26及审核记录 | 区分本地生产链验证与未执行在线验收 | 约94%，完整审核0/5，不归档、不提交 |

独立AI理解/生成扩展继续使用专用用途策略，不借用非会话文本许可。真实供应商、付费请求和无Core服务的浏览器整链保留为发布条件。

## 发布矩阵及默认栈修复追加覆盖

| 文件或明确文件组 | 当前增量 | 验证与边界 |
| --- | --- | --- |
| Manifest CLI`main.rs`、适配器`provider_manifest_fixture.rs` | 逐层堆化完整夹具、上传及异步工具Future，修复默认主线程栈溢出 | 17供应商、101目标、37可执行夹具、黄金快照和发布脚本通过 |
| 服务测试`capability_metadata_tests.rs` | 堆化完整重开场景及RPC外围，保持生产和断言不变 | 精确用例及服务完整865项通过 |
| 六个受影响Rust包及全测试矩阵 | 全目标检查；契约、转译、适配、Core、存储、代理、服务、Host、配置/API回归 | 32/148/713/273/197+5忽略/806/865/3/149项通过 |
| WebUI生产构建、架构、lint及完整测试 | 构建与交互逻辑回归 | 127文件/775项通过；Workbench CSS旧预算超限单独记录 |
| 严格检查、格式与差异 | 生产Clippy及源码整洁 | 适配器/代理无问题；服务仅旧文件诊断；格式和差异空白通过 |
| 方案19.27及审核记录 | 记录真实发现、修复、基线和发布条件 | 约97%，完整审核0/5，进入清洁审核 |

Workbench CSS预算、真实供应商/付费推理和无Core服务浏览器整链不是本任务可伪造的通过项；前者属于未修改基线，后两者保留为在线发布条件。

## 连续五轮完整审核收口

| 轮次 | 全范围复核重点 | 结果 |
| --- | --- | --- |
| 1 | 内部契约、16项配置、全部协议出口、公共字段及非会话来源门 | 契约/转译180项、适配器713项通过，零问题 |
| 2 | 会话时序、历史恢复、A→B→A、压缩窗口、项目目录与资产所有权 | Core273项、存储197项通过/5项忽略、V1入口54项通过，零问题 |
| 3 | 并发、取消、来源寿命、登记结算、整组草稿替换及确认恢复 | Agent Runtime806项、服务媒体24项、待发送提示词9项、WebUI775项通过，零问题 |
| 4 | 公共Responses、独立扩展、上传硬限制/禁重定向、配置与Manifest | 设置51项、理解15项、Host3项、配置/API149项、Manifest矩阵通过，零问题 |
| 5 | 完整差异、模块注册、文档、测试匹配、生成物与最终门禁 | 六包检查、格式、WebUI构建/架构/lint/775项及差异检查通过，零问题 |

连续零问题计数最终为**5/5**。审核前发现并修复的固定阈值残留与预览命名问题未计入轮次；此后代码代际未再出现真实问题。真实供应商和用户Core浏览器整链保留为发布条件，既有CSS预算及服务Clippy诊断保留为基线。
