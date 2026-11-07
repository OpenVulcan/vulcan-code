# 反馈与消息中心

本文是 Vulcan Code 客户端侧反馈、公告与个人消息的运行时契约。服务端（OpenVulcan 用户中心）由
openvulcan-web 仓库实现；本文只记录客户端的义务与客户端可观察到的接口形状。

- 过滤与草稿：`crates/vulcan-feedback/`（`limits.rs` 是全部边界值的唯一来源）。
- 用户中心客户端：`crates/vulcan-provider-adapters/src/client/openvulcan_inbox.rs`。
- 服务：`crates/vulcan-service-kernel/src/feedback_services.rs`，RPC 入口 `src/rpc_dispatch/feedback.rs`。
- WebUI：`apps/vulcan-code-web/src/components/features/feedback/`、`src/service/feedback-service.ts`。
- TUI：`apps/vulcan-code-tui/src/service_api/feedback.rs`、`src/app/pages/feedback_messages.rs`。

## 一、发送反馈

1. **只有用户能发送。** 发送分两步：`service_prepare_feedback` 在本机过滤并返回将要发出的精确请求体
   （`payload`）、其 `digest`、字节数、日志行数与各类替换次数；`service_submit_feedback` 只接受
   `draftId` 与 `digest`，并只发送与该摘要完全一致的已准备请求。草稿内容在两步之间变化时摘要不再匹配，
   服务返回 `serviceError.feedback.previewChanged`，用户必须重新预览。界面返回编辑会丢弃预览。
2. **幂等。** `draftId` 是客户端保存的 UUID；同一窗口的多次预览与重试复用同一编号，作为服务端的幂等键。
   服务只在内存中保留最近 `PREPARED_DRAFTS_KEPT` 份已准备草稿，重启后需重新预览。
3. **必须登录。** 发送使用已登录 OpenVulcan 账户的凭据；未登录可以编辑和预览，但不能发送，界面提供
   登录入口且不丢弃草稿。凭据所属区域决定发送到中国区或国际区，两区用户与数据互相独立。
4. **入口（`source`）。** `manual`（账户菜单、设置页）、`error_card`（会话错误卡的“报告此问题”，附带
   该错误的结构化事实，HTTP 状态以整数发送）、`ai_draft`（AI `feedback` 工具起草，用户检查后发送）。
5. **可选附件。** 客户端信息（版本、系统、界面语言）与最近日志默认勾选；允许联系默认不勾选。日志从
   服务日志最近 `LOG_SCAN_SEGMENTS` 个分段读取，每段最多扫描 `LOG_SCAN_MAX_BYTES_PER_SEGMENT`，只取
   级别为 warn 或 error 的 JSON 行及包含失败调用编号的行，保留最新的 `LOG_EXCERPT_MAX_LINES` 行、`LOG_EXCERPT_MAX_BYTES`。

## 二、本机过滤

1. 标题、正文、错误事实与日志在预览前全部经过过滤；预览展示的就是将要发送的内容，过滤只发生在本机。
2. 顺序固定：已知秘密（当前会话与配置中登记的密钥、令牌，精确替换）→ 路径（项目根替换为 `<workspace>`，
   主目录折叠为 `~`）→ 系统用户名（`<user>`）→ URL（保留协议、主机与路径，去掉凭据、查询与片段，隐藏
   私有网络主机）→ 随包 PII 规则。
3. 替换结果只返回各类别次数（`secret`、`path`、`user`、`url` 及 PII 类别），从不返回或保存原文。
4. 过滤后请求超过 `REQUEST_MAX_BYTES` 时拒绝准备，不截断后静默发送。

## 三、公告

1. 公告无需登录。区域：已登录取账户区域；未登录时界面语言为 `zh-CN` 取中国区，其他语言取国际区。
2. 客户端按 ETag 条件获取，离线时使用本进程最近一次结果并标记 `offline`。
3. 定向在客户端判断：`minVersion`/`maxVersion` 必须是精确 `x.y.z` 版本，否则排除该公告；`os` 为空表示
   全部系统；`startsAt`/`endsAt` 按 RFC 3339 判断当前时间是否在窗口内，无法解析即排除。
4. 内容选界面语言，缺失时依次回退到默认语言（`vulcan_i18n::DEFAULT_LOCALE`）与公告中的任一语言。正文为 Markdown；图片只允许 OpenVulcan 自有 CDN 前缀，
   由用户中心发布时校验。
5. 重要公告（`priority: important`）在工作台顶部显示横幅，只显示最新一条未关闭的；关闭后永不再显示该条
   横幅。打开消息中心公告页即把列出的公告记为已看。已看与已关闭标记按区域保存在应用数据库
   `announcement_states_v1`。
6. 接收开关为 `vulcan-code.json` 的 `announcementsEnabled`，缺省视为开启，只在内存中补默认值；关闭后不再
   获取公告，消息中心提示开关位置。
7. 公告链接只有三种动作：`app_update`（打开应用更新设置）、`url`（外部浏览器打开）、`feedback`（打开
   对应反馈详情）。

## 四、个人消息与反馈记录

1. 个人消息需要登录，包括维护者对反馈的回复。未读数在登录期间每 60 秒读取一次，公告每 10 分钟读取一次
   （WebUI `feedback-center-runtime.ts`）；后台读取失败保留上次状态，由打开的页面报告错误。
2. 标记已读按编号（每次最多 `OPENVULCAN_MARK_READ_MAX_IDS` 个）或按“某编号及之前全部”（`allBefore`）。
3. 用户可以查看自己的反馈与回复、关联拉取请求、删除反馈。删除不可撤销，界面必须二次确认。
4. 关联的拉取请求地址必须以 `OPENVULCAN_FEEDBACK_PR_URL_PREFIX`（本仓库的 PR 地址）开头。

## 五、AI `feedback` 工具

实现：`crates/vulcan-agent-runtime/src/tooling/native/feedback.rs`，宿主
`crates/vulcan-service-kernel/src/feedback_tool_host.rs`，WebUI 卡片 `src/conversation/tools/cards/feedback-card.tsx`。

1. **只起草，永不发送。** `action: draft` 只校验并返回 `{kind, title, body}`（`status: awaiting_user`），没有副作用，
   权限分类为 `Ungoverned`。工具卡提供“检查并发送”，打开 `source: ai_draft` 的反馈窗口；之后与手动反馈完全相同，
   由用户预览并发送。标题与正文上限取自 `vulcan_feedback::limits`。
2. **`action: link_pr`** 把拉取请求关联到用户自己的反馈，经 `service_link_feedback_pr`，分类为 `Opaque`：只读与工作区
   写入模式询问用户，自动审批模式走判定与复核，完全权限直接执行。缺失或未知的 `action` 同样按 `Opaque` 处理。
3. 计划模式只允许 `draft`；子代理永不可见（`CHILD_AGENT_FORBIDDEN_TOOL_IDS`），宿主也拒绝非根会话。只有安装了
   宿主时才注册该工具。

## 六、本机修复

1. **入口。** 反馈窗口在类型为“问题”时（发送前后均可）提供“在本机修复…”。窗口说明流程后询问上级目录（每次都由
   用户选择，没有默认值；只在本浏览器记住上次位置作为选择器起点）与文件夹名（预填 `vulcan-code`）。
2. **准备目录。** `service_prepare_self_fix_workspace {parentDir, folderName}`：`folderName` 必须是单个普通路径分量，
   上级目录必须是已存在的绝对目录；目标不存在时创建空目录，已存在时必须为空或其 `.git/config` 任一远端指向
   `github.com/openvulcan/vulcan-code`（https、ssh、scp 写法均可），其他情况拒绝、不覆盖。随后把目录加为项目，返回
   `{projectId, path, existingCheckout, installedVersion}`。这里不克隆，克隆由会话在用户的权限模式下执行。
3. **会话草稿。** WebUI 在该项目中打开新会话草稿，预填问题、结构化错误事实（不含原始错误消息）、已安装版本、
   反馈编号（已发送时）与目录；草稿会话设置带 `promptSkillOverrides: {"vulcan-self-fix": "enabled"}`。用户点击发送
   才创建会话，这一步就是确认。
4. **按需技能。** `vulcan-self-fix` 是随包技能，`vulcan.yaml` 声明 `activation: on_demand`：没有任何层级覆盖时
   视为关闭（状态来源 `on_demand`），不会进入普通会话；技能列表显示“按需启用”。新会话提交只接受对随包按需技能的
   `promptSkillOverrides`，其他名称直接拒绝；开关作为会话层覆盖写入 `selection.prompt_skill_overrides`，恢复会话时
   同样读回。
5. 技能本身（`crates/vulcan-prompt-skills/src/assets/system/vulcan-self-fix/SKILL.md`）要求：不触碰正在运行的 Release
   实例，Debug 构建在检出目录的 `output/` 中并行运行；推送、fork 与创建拉取请求都要等用户在会话中明确同意；从不经手
   GitHub 令牌。拉取请求说明按 `.github/pull_request_template.md` 编写，并通过 `feedback` 工具的 `link_pr` 关联反馈。

## 七、RPC

| 方法 | 参数 | 说明 |
| --- | --- | --- |
| `service_prepare_feedback` | `{draft}` | 过滤并返回预览 |
| `service_submit_feedback` | `{draftId, digest}` | 发送已预览的请求 |
| `service_list_feedback` | `{cursor?}` | 我的反馈，最新在前 |
| `service_feedback_detail` | `{feedbackId}` | 详情、回复与关联 |
| `service_link_feedback_pr` | `{feedbackId, url}` | 关联拉取请求 |
| `service_delete_feedback` | `{feedbackId}` | 删除反馈 |
| `service_list_announcements` | 无 | 适用于本安装的公告 |
| `service_mark_announcements` | `{announcementIds, dismissed}` | 记为已看或关闭横幅 |
| `service_message_summary` | 无 | 未读数与最新编号 |
| `service_list_messages` | `{unreadOnly, cursor?}` | 个人消息分页 |
| `service_mark_messages_read` | `{ids}` 或 `{allBefore}` | 标记已读 |
| `service_message_center_settings` | 无 | 读取接收开关 |
| `service_save_message_center_settings` | `{settings}` | 保存接收开关 |
| `service_prepare_self_fix_workspace` | `{parentDir, folderName}` | 准备本机修复目录并加为项目（仅 WebUI） |

WebUI 与 TUI 一对一调用除本机修复以外的方法。用户中心路由为 `{origin}/api/v1/vulcancode/` 下的 `feedback`、
`feedback/{id}`、`feedback/{id}/links`、`feedback/{id}/delete`、`announcements`、`messages`、`messages/read`。

## 八、本地联调

Debug 构建读取 `VULCAN_DEBUG_OPENVULCAN_AUTH_ORIGIN`（例如 `http://127.0.0.1:35101`），以一个回环来源同时
替换两个区域的用户中心来源，用于对接本地 openvulcan-web。只接受 `127.0.0.1`、`localhost` 或 `[::1]` 上
不带路径、用户信息、查询与片段的 `http(s)` 来源，其他取值带警告忽略；Release 构建从不读取。签名清单中的模型目录
地址仍是生产授权来源，覆盖生效时 OpenVulcan 凭据的元数据请求随之重定向到该回环来源（`debug_redirect_auth_url`），
否则登录后的凭据校验会因离开被覆盖的部署而失败；推理来源不受影响。
