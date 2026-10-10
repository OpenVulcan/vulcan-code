# 权限模式与判定审批管线

## 归属

本契约约束会话权限模式、工具调用权限闸门、判定模型（本机或云端）、复核 LLM、用户审批，以及它们的
系统级设置、项目级策略和 WebUI/TUI 设置界面。修改下列任一入口时，先读本文，再以当前源码与测试为准。

## 红线

- **管线顺序固定，错误绝不放行。** 模式闸门 → 确定性规则 → 判定模型（可选）→ 复核 LLM（可选）→
  用户。每层只能放行、拒绝或交给下一层；后端出错、超时、输入超限或读不到模式时一律交给下一层或拒绝。
  没有可回复的前端时用户层的结果为拒绝。
- **放行只对本次调用有效。** 判定模型、复核 LLM 与用户的“允许本次”都不形成持久规则。
- **高风险与判定拦截不能由单层放行。** 命中高风险规则时判定模型的 allow 不生效；判定为 block 或命中
  高风险的调用，复核 LLM 只有在用户开启“允许 LLM 放行判定器拦截项”时才能放行，否则交给用户。
- **仅可查看不使用判定层与复核层。** 该模式的语义就是写入必须由人批准。
- **模型是系统级，策略是项目级。** 后端、设备、模型登记表与复核路由只存在于用户全局配置；项目配置
  只保存各类别档位，Schema 中没有模型或后端字段，不允许不同项目使用不同模型或后端。
- **策略文本与权重一致。** 本机模型的策略只来自已登记模型包；云端判定读取随构建发布的副本。类别与
  档位是固定枚举，代理与工具参数不能添加策略文本；未知档位报告为设置问题，不静默改成默认值。
- **只有显式选择写入设置。** 判定出厂为“未设定”；初始化引导中跳过或关闭不写入任何内容。启动或刷新
  不得回写权限模式、判定设置或项目策略（AGENTS 第 5 条）。
- **不提供模型下载链接。** 本机模型只从用户指定的本机路径登记；官方模型包清单只用于识别官方标记。

## 权限模式

| 能力 | 仅可查看（`read_only`） | 工作区修改（`workspace_write`，默认） | 完全权限（`full_access`） |
| --- | --- | --- | --- |
| 文件写入工具 | 交给用户 | 工作区内直接放行，其余进入管线 | 放行 |
| `shell_command`、`terminal_session` | 已知只读命令放行，其余交给用户 | 进入管线 | 放行 |
| MCP、插件与其他写入能力 | 已知只读放行，其余交给用户 | 进入管线 | 放行 |

- 文件写入工具指 `file_write`、`file_edit`、`apply_patch` 与 `file` 的移动、删除动作；`file_read` 与 `file` 的
  列表、属性动作不经闸门。
- 模式保存在会话配置（`PermissionMode`，会话元数据模式版本见
  [`session-metadata-migrations.md`](session-metadata-migrations.md)）；新会话默认值只存在于运行时内存。
- 子代理会话使用其根会话的模式；读不到模式时拒绝调用。
- 草稿会话的显式选择随首次提交的 `sessionSettings.permissionMode` 发送并在首个提示边界写入；未选择时
  不发送值。已存在会话通过 `service_set_session_permission_mode` 修改，只影响下一次工具调用。

## 判定设置

- **系统级状态**：`state` 为未设定（`null`）、`off` 或 `enabled`；`backend` 为 `local` 或 `cloud`。工作台
  投影 `permissionJudgeState` 只供界面决定入口显示：未设定时入口提示“需设定”并打开设置页，关闭时隐藏
  判定入口与项目策略，启用时全部显示。
- **本机后端**：模型包登记、切换、回退与移除；包内声明策略版本与放行余量，载入时验证。设备探测给出推荐
  设备，可手动覆写；CUDA 执行库可锁定文件夹或一键安装。闲置卸载尚未实现。
  登记表单可从服务宿主浏览选择文件夹，并先经 `service_inspect_permission_judge_package` 完整校验（不登记，
  结果按稳定代码本地化）；只有校验通过才可提交，非官方模型包须显式勾选信任。
- **云端后端**：使用 `permission_judge` 消费者配置档的判定路由（TypeSafe 或 OpenRouter 上的 jev）。放行
  概率门槛只在 `CLOUD_ALLOW_PROBABILITY` 定义，当前值尚未用样本校准；不记录费用。
- **复核 LLM**：`permission_review` 配置档、用途 `tool_call_review` 的对话路由；只收到判定层结论标签，
  不收到原始分数。给用户的理由使用界面语言：宿主在 `<reply_language>` 区段写入已保存的界面语言（未保存时为
  系统语言），以代码与本族语名称表示。
- **速度测试**：1 条与 10 条两种，用户触发，结果只留在当前界面；云端测试会产生费用，执行前先确认。

## 项目策略

- 保存在项目根目录的 `.vulcan/code/config.json` 的 `permission_judge.options`，键为类别、值为档位；
  未列出的类别使用模型包默认档位。判定时按调用所在工作区读取。
- 读取与保存只针对使用中模型的类别；保存时保留模型已不再有的类别的旧选择，但不发送给判定器。
- 设置界面只列出使用中模型的类别，判定未启用时整个区块不显示。WebUI 位于项目设置对话框，并在权限判定
  设置页总览列出各类别与默认档位、经“类别授权”子页面选择项目后编辑；TUI 位于设置区的同名页面并在页首
  选择项目。各类别附带说明其涵盖范围的提示。

## 判定记录

- **开关**：系统级 `recordDecisions`，出厂关闭，只有用户显式开启才写入；保存请求必须携带该字段，任何保存都不能
  因遗漏而改变它。关闭时不写入新记录，已有记录仍可查看、标记、导出与清除。
- **记录范围**：只记录判定模型或复核 LLM 参与的决定；模式闸门与确定性规则直接决定的调用不记录。时间为判定
  开始时刻，记录在最终结论（判定放行、复核放行或拒绝、用户放行或拒绝、失败）产生后写入，写入不阻塞调用。
- **内容**：原样保存工具参数文本（含命令与路径）、工作区、会话、高风险类别、判定标签或错误与耗时、判定输入
  轨迹（后端、模型、策略版本、类别设置、渲染文本、本机三类分数或云端放行概率、输入令牌数）、复核结论与耗时。
- **存储**：只保存在本机用户目录 `<用户 Vulcan 根>/code/judge-records/records.v1.jsonl`，不上传。滚动上限只在
  `permission_judge/records.rs` 的 `RETENTION_RECORDS` 与 `RETENTION_DAYS` 定义，超出时整体压缩重写；文件
  任一行无法解析时整份记录报错且不再写入，直到用户清除（V0.1.x 不做旧格式兼容）。
- **标记与导出**：用户可为有判定轨迹的记录标记“本应给出”的标签，再次选择同一标签清除标记；没有轨迹（判定
  失败或未运行）的记录不可标记。导出只包含已标记记录，每行为判定训练输入格式 `{settings, rendered}` 加
  `label` 与来源元数据，可直接作为本机测试集或训练样本。
- **服务**：`service_permission_judge_records`（筛选判定结果 / 工作区 / 只看已标记，最新在前，以记录 ID 为游标
  分页）、`service_mark_permission_judge_record`、`service_export_permission_judge_records`、
  `service_clear_permission_judge_records`。

## 界面

- 输入区权限模式选择器（WebUI 为输入区工具栏中加号右侧的权限标签按钮，切换到完全权限前须勾选风险确认；TUI `⛨` 标签、`Alt+P`、`/permissions`）附带判定入口。
- “权限判定”设置页：总览依次为开关与后端选择、所选后端的设置（本机为使用中模型、设备、CUDA 摘要与运行
  状态；云端为判定路由）、启用时的类别授权摘要、复核路由（含“允许 LLM 放行判定器拦截项”）与工具入口；模型管理与登记、CUDA
  环境（组件安装、库文件夹、手动下载）、类别授权（选择项目后编辑类别档位）、诊断（模型状态表、速度测试）和判定记录（筛选、标记、导出、清除）是带返回按钮的子页面。状态按服务发布的
  `pollIntervalMs` 重读，唯一定义为服务内核 `STATUS_POLL_INTERVAL_MS`。WebUI 每个选择立即保存；判定未设定时
  选择暂存，开启时一并写入；已启用时切换到尚未设置完成的后端会等待其设置完成，不先写入。
- 初始化引导：第一步为权限判定，三选一（本机、云端、暂不启用）。WebUI 在判定未设定时于启动后自动
  打开一次（浏览器本地标记），之后可从设置页再次进入；TUI 没有本地持久偏好，只能从 `/setup` 或权限判定
  页进入，不自动弹出。
- 用户审批复用 Ask 面板，显示命令或路径、权限模式、高风险类别、判定结论与复核理由。

## 当前实现与验证入口

| 职责 | 入口 |
| --- | --- |
| 权限闸门与管线顺序 | `crates/vulcan-agent-runtime/src/agent_runtime/tool_permission.rs` 中的 `admit_tool_call` |
| 已知只读与高风险命令规则 | `crates/vulcan-agent-runtime/src/agent_runtime/tool_permission/command_rules.rs` |
| 模式读取、判定与复核宿主 | `crates/vulcan-service-kernel/src/tool_permission_host.rs` |
| 会话模式修改与草稿首次提交 | `crates/vulcan-service-kernel/src/permission_mode.rs`、`conversation_submission.rs` |
| 判定设置、项目策略与速度测试服务 | `crates/vulcan-service-kernel/src/permission_judge/services.rs` |
| 判定记录存储、筛选、标记与导出 | `crates/vulcan-service-kernel/src/permission_judge/records.rs` |
| 复核 LLM 请求 | `crates/vulcan-service-kernel/src/permission_judge/review.rs` |
| 本机 worker、设备与 CUDA | `crates/vulcan-service-kernel/src/permission_judge/{worker,environment}.rs` |
| 策略、模型包、官方清单、云端副本与门槛 | `crates/vulcan-permission-judge/src/{policy,package,known,cloud,decision}.rs` |
| WebUI 界面 | `apps/vulcan-code-web/src/components/features/settings/permission-judge-*`、`components/features/onboarding/`、`components/project/project-permission-judge-section.tsx`、`components/composer/` |
| TUI 界面 | `apps/vulcan-code-tui/src/app/pages/{permission_judge,setup_guide,project_permission_judge}.rs`、`app/permission_mode.rs` |
