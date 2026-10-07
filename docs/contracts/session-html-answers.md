# 会话 HTML 回答

## 权威与当前交付边界

本契约归属于 `vulcan-session-core::v1::html_answers` 和
`vulcan-service-host::html_answers`。修改协议、身份、容量、HTTP 隔离或浏览器支持边界时，
必须同步本文及相应行为测试。

当前已交付协议与隔离基座、会话偏好及迁移、请求指引，以及 GUI/WebUI 输入区开关。
Core 查看授权、结果卡与 TUI 完整功能尚未交付。
下述隔离回归使用真实浏览器和合成监听器，不等同于完整产品集成验收。

## 会话偏好与请求边界

`SessionConfig.html_answers` 是唯一权威，含 `enabled`、独立比较交换 `revision` 与可空
`prepared` 快照。元数据模式 2 → 3 显式迁移为默认关闭，原始字节备份和严格读取规则见
[元数据迁移](session-metadata-migrations.md)。新版本缺字段或不一致修订拒绝读取。

`service_set_session_html_answer_mode` 接收 `request`（`sessionId`、`enabled`、
`expectedRevision`），返回真实会话/代次、文档修订、偏好和冲突标志。冲突返回当前值，不自动
覆盖或重试。正在生成时只保存期望，不启动模型回合，不更改已发出的请求，与 Plan/Goal 独立。

首轮草稿通过 `sessionSettings.htmlAnswersEnabled` 与其他选择共同提交；已有会话不能借草稿
参数覆盖设置。明确分叉保留期望、重置准备快照；下一请求重新声明。自动新建子会话沿默认关闭。

请求准备通过 `dynamic.html_answers` 注入权威资源 `agent-prompts/html-answers.md` 或明确关闭
说明。默认未开启的会话不发送关闭说明；重复同步幂等；压缩请求不切换偏好。宿主 XML 信封负责
正文转义，持久原文仍是原始指引。不开启时不生成新页面，也不删除已保存的 HTML 回答原文。

客户端沿已有事件窗口加载和重连读取**持久**偏好。实时偏好只附在成功候选的 durable 确认中，
不放在 volatile 流式边上；客户端按会话、历史代次及偏好修订拒绝迟到值。存储失败时不宣称保存
成功；既有写入器保留待恢复候选，之后的成功确认仍可更新状态，不能在结果不明时回滚真实存储。

GUI/WebUI 开关有 `aria-pressed`、文字与亮起状态，保存中防重复，下一请求生效状态有提示。
偏好保存与正常发送共享等待边界；跨会话切换后旧回包不会修改新输入区。当前共享 11 种界面语言
均已添加对应文案，语言成员仍以共享目录为唯一来源。

## 内容与身份

专用围栏语言为 `vulcan-html`，其后必须包含 JSON 对象，且仅允许必填的
`version` 与 `title`。当前协议版本由 `HTML_ANSWER_VERSION` 定义。
普通 HTML、SVG、Mermaid、用户输入、思考和工具回包不会获得本协议的执行身份。

围栏须为顶层反引号围栏，显式闭合且满足 CommonMark 长度规则；块引用、列表、脚注
及其他代码块里的示例不是可执行结果。元信息重复字段、未知字段、未知版本、非法标题、
空源码及超限块均保留原文，通过稳定错误码解释不可执行原因。

`project_html_answers` 只对终态且对供应商可见的助手正文进行解析，流式事件不重复扫描
完整历史。可打开状态还要求调用方确认原事件已可靠保存；这个事实不能由模型提供。
历史加载、分页和重连只对实际读到的持久源码投影，随后排除被更新驻留修订取代的标注；实时标注只随成功落盘候选的确认发布，不能直接将
内存终态视作可靠保存。纯元数据候选不重复解析历史，普通流式追加不执行整文 Markdown 解析。

`html_answer_projections` 是独立于原始事件的可选派生扩展，包含精确 `seq`、`event_revision`
及描述。原事件修订不因保存确认而改变，也不把展示字段写进事件文件。客户端仅接纳窗口内匹配
归属、代次、源码修订及可靠发布边界的标注；重复确认保持展示身份，历史代次替换清除旧标注。
事件语义修订与文档持久修订是独立计数，不允许通过比较两者推断源码已保存。
仅包含协议示例而无可展示围栏的候选返回空标注确认，避免重连重复扫描；普通消息省略标注。

重连可带 `htmlAnswersFromSeq`：缺少 Core 标注的终态 HTML 候选最早序号。它必须位于客户端
已加载范围。Core 从权威存储补回早于常规恢复尾段的标注，解决漏收保存确认的问题，不重发该
前缀 HTML 正文。普通消息与标注齐全的窗口省略该字段；客户端文本标记只用于决定是否补查，
不能据此创建执行身份。

描述仅包含原始围栏/源码区间、标题、序号和状态。区间同时提供 UTF-8 字节偏移与
UTF-16 码元偏移，客户端必须以未改写的原始助手正文为坐标系。HTML 正文不产生第二份
持久副本，不改变供应商回放和计费事实。

身份是带域隔离、长度前缀编码的完整 SHA-256：绑定会话 ID、事件 seq、API 调用 ID、
专用围栏序号、协议版本、标题与源码摘要。重放相同事实保持身份，分叉或内容替换产生新身份。

容量只在 Core 常量定义，其他模块应引用或由其派生；本文不另存一份数字表。

## 页面隔离

`HtmlAnswerOrigins` 只接受两个明确配置且互不相同的规范 HTTP(S) 来源，拒绝凭据、路径、
查询参数、片段和 CSP 注入。来源不能从未经验证的 `Host` 或转发头推断。

可信外壳的脚本来自自身来源，子框架只允许导航到登记的只读渲染来源。生成内容通过独立
HTTP 响应使用 `sandbox allow-scripts`，同时 iframe 保留 `sandbox="allow-scripts"`；
没有 `allow-same-origin`、弹窗、下载、顶层导航或宿主桥接权限。

生成内容仅允许页内脚本、内联样式和内嵌资源；连接、外部脚本、worker、嵌套框架、object、
base 和表单提交被禁止。HTTP 禁止缓存与来源外传，并限制设备权限。不得为此放宽现有普通
附件的响应策略。CSP 限制不能替代授权，也不能提供硬 CPU/内存配额。

完整接入仍须实现单次票据、只读租约、每次读取的最新会话归属校验，以及拒绝将生成内容作为
独立顶层页面直接打开。`content_response` 仅包装已授权内容，不能单独作为未鉴权路由使用。

规范依据：
[CSP sandbox](https://developer.mozilla.org/en-US/docs/Web/HTTP/Reference/Headers/Content-Security-Policy/sandbox)、
[Electron 安全建议](https://www.electronjs.org/docs/latest/tutorial/security)。
浏览器或 Electron 升级、响应策略调整、桌面预加载变更时重跑真实隔离回归。

## 定向验证

```powershell
rtk cargo test -p vulcan-session-core html_answers --lib
rtk cargo test -p vulcan-service-host html_answers --lib -- --test-threads=1
rtk cargo clippy -p vulcan-session-core -p vulcan-service-host --all-targets -- -D warnings
rtk cargo build -p vulcan-service-host --example html_answers_security_fixture
rtk proxy pnpm.cmd --dir apps/vulcan-code-desktop run build
rtk proxy node apps/vulcan-code-desktop/tools/html-answers-security.mjs --fixture=target/debug/examples/html_answers_security_fixture.exe
```

真实回归要求本机 Playwright 管理的 Chromium、Firefox 和当前 Electron。
可通过 `--chromium=<实际浏览器路径>` 指定 Chromium，通过 `--report=<本地文件>` 保存证据。
`--browser=electron` 等过滤只用于定向复现；报告保留执行矩阵，不能据此宣称三种浏览器全部通过。

验收只监听临时回环端口，不访问用户会话或付费模型。Electron 会短暂显示一个不抢焦点的独立
验收窗口并自动关闭，用真实鼠标输入验证跨来源页面；隐藏窗口不可靠地分发这类输入。
Windows 打包宿主可能虚拟化 LocalAppData，测试对浏览器路径调用 `realpath`，不修改系统配置。

2026-10-07 原型已在 Chromium 154.0.8037.0、Firefox 156.0、Electron 44.4.1 通过。
页面按钮正常执行；父 DOM、存储、Node 和桌面桥接不可访问；网络与导航逃逸没有到达合成禁止
目标；打开关闭循环后生成框架被销毁。此版本记录是验收证据，实际依赖版本以清单为准。

## 设计参考

参考 [Answer me with HTML](https://github.com/QingYunA/answer-me-with-html)
的按需可视化及交互页面呈现思路。本期自行实现宿主协议，不依赖其 CLI 或本机参考仓库。
如后续复制受许可文件，必须保留实际版权、许可和修改来源，README 鸣谢不能替代许可。
