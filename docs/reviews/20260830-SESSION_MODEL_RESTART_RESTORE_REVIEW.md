# 会话模型重启恢复修复记录

## 用户现象与事实

- 已有会话切换并执行过指定模型，程序重启后输入区却显示统一的全局模型；未注意时发送请求，才生成非预期模型切换记录。
- 本次只读核对会话 `5ffbbeaa-2936-4af1-bad9-192b0ba3586e` 的配置：活动路线已因误发变为 `commandcode / meta/muse-spark-1.2`，最新待生效选择仍为 `kimi / k3-256k`。metadata 没有被本任务改写。
- 模型配置不是应从聊天文本或历史切换提示推断的字段，权威来源是 V1 metadata 的 `config.active_request` 和 `config.pending_request`。

## 根因

1. 工作区轻量索引刻意不加载旧运行态，启动时 `session_runtime_state` 为空，但缺少独立的会话模型配置恢复步骤。
2. 启动首次工作台投影之后，又使用全局选择生成的模型行覆盖 `workbench.providers`。
3. 会话导航快速路径仅使用内存运行态，没有读取尚未恢复的会话模型 metadata。
4. 模型解析把“已有会话尚未恢复配置”混同于“空白草稿”，允许回退到全局模型。
5. 通用设置刷新另有一条全局模型投影路径；索引刷新也不会保留只有模型配置、尚未加载正文的运行态适配器。这些路径可再次覆盖正确选择。

前端会把选择器渲染的精确路线作为提交参数，因此错误展示进一步变成真实请求路线，随后留下用户截图中的模型切换记录。

## 修复范围

| 文件 | 调整 |
| --- | --- |
| `crates/vulcan-workspace-store/src/workspace_store/session_v1_store.rs` | 新增仅读取活动及待生效请求配置的方法，校验精确定位器身份，不读回合、分片或上下文；能力子树的额外校验仍留在能力访问路径 |
| `crates/vulcan-service-kernel/src/session_model_projection_v1.rs` | 新增共享只读模型投影；冷会话读 metadata，常驻会话读唯一 owner，发布前重新读取常驻最新状态，保留生命周期守卫 |
| `crates/vulcan-service-kernel/src/lib.rs` | 注册共享模型投影模块 |
| `crates/vulcan-service-kernel/src/service_bootstrap.rs` | 首次可用快照发布前恢复当前会话模型及思考控件，删除全局模型行覆盖 |
| `crates/vulcan-service-kernel/src/project_conversation_services/conversation_services.rs` | 导航快速路径前恢复目标会话模型，完整刷新前释放相同 owner 的导航守卫，避免重复获取 |
| `crates/vulcan-service-kernel/src/project_conversation_services.rs` | 复用已有选中会话字段投影入口 |
| `crates/vulcan-service-kernel/src/workbench_refresh.rs` | 通用刷新恢复选中会话请求配置；最终提交时协调选中身份与模型；显式编辑或分叉目标保持原优先规则 |
| `crates/vulcan-service-kernel/src/session_hydration/run_state_preservation.rs` | 索引刷新保留纯模型配置适配器，但不标记正文已水合 |
| `crates/vulcan-service-kernel/src/settings_provider_projection.rs` | 已有会话配置未恢复时禁止全局回退，保留空白草稿默认模型规则；移除不再使用的全局模型行入口 |
| `crates/vulcan-service-kernel/src/settings_runtime_projection.rs` | 设置刷新改用与导航相同的会话模型投影 |
| `crates/vulcan-service-kernel/src/tests/active_session_state_tests.rs` | 回归纯 metadata 模型选择跨索引刷新保留，且不创建正文水合标记 |
| `crates/vulcan-service-kernel/src/tests/hot_path_scan_guard_tests.rs` | 守护启动、刷新、导航必须恢复纯配置且不能恢复为正文加载或全局模型覆盖 |

保留既有安全边界：待生效用户选择优先展示，只在下一次 API 安全边界真实生效；不撤换执行中的路由租约，不新建回合或模型切换事件，不改全局默认模型，不扫描全部会话目录，不修复或覆盖用户历史。

## 验证

已通过：

- 后端 `cargo check -p vulcan-service-kernel`，无编译警告。
- 模型冷启动及常驻最新状态投影：2 项。
- 活动/纯配置运行态保留：9 项。
- 热路径与正文加载边界守卫：13 项。
- 请求配置持久化与下一 API 生效：9 项。
- 会话导航相关已有测试：4 项。
- 工作台刷新已有测试：4 项。
- 原有 V1 重启配置投影：5 项。
- 真实磁盘 metadata 恢复、损坏正文隔离及所有者校验：1 项。
- 原能力配置 metadata 回归：4 项。
- 统计卡相关前端回归：96 项，类型检查、界面架构检查、生产构建通过。
- 前端模型选择控制器与模型选择器既有回归：17 项通过；前端合计 113 项通过。
- 本次 Rust 文件格式检查、`git diff --check` 通过。

后端上述测试共 51 项通过。真实磁盘测试只使用临时目录，未运行收费 Provider 请求，未重启或替换用户当前桌面进程。WebUI 构建仍有分包超过 500 kB 的体积提示。

## 独立的现存资源阻碍

5 项模型选择器 Provider 回归在构造原有 OpenRouter 夹具时失败，尚未进入待验证断言，错误为 `Provider openrouter is not registered`。已追到未被本任务修改的随包资源完整性不一致：

- OpenRouter Index 声明 24,071 字节，实际 24,213 字节；Index SHA-256 为 `0d2800603d022474fc4dfdea2d44187d208579211681d922c799c97a1a84ffb1`，实际为 `f40fe7203cdab944ecdf9594d1ca7735422de10eb083c925c0077f35911852e5`。
- 独立执行 Manifest CLI `validate-all`，首个失败是 Alibaba Index 声明 312,144 字节、实际 322,161 字节。
- Provider 适配器及 Manifest 目录相对 HEAD 没有本次差异。未修改这些资源、未重建 Index，也未放宽测试断言或绕过完整性验证。

这属于原有供应商资源发布边界，不能算作本次模型选择器集成验证已通过，需要在获得该资源范围的修改指令后处理。

## 审核结论与交付边界

修复过程中进一步发现并处理了设置刷新全局覆盖、冷配置被索引刷新丢弃、显式分叉目标在最终协调时可能被旧选择覆盖三处关联边界。

最终两轮源码审核分别核对模型权威与发布顺序，以及冷/常驻访问、生命周期锁序、并发刷新归属和无正文访问约束，未发现新增代码问题。上述 5 项集成回归及桌面端重启、切换、首次发送实机验收仍明确保留为未通过或未执行边界，不作全链路已经验证的承诺。

桌面版需要重新编译并彻底重启，才能加载本次后端修复。已经因误发形成的真实历史请求不撤销、不改写；metadata 中仍有最新待生效选择时恢复该选择，没有则保留当前真实活动路线。
