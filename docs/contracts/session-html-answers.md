# 会话 HTML 回答

## 权威与当前交付边界

本契约归属于 `vulcan-session-core::v1::html_answers` 和
`vulcan-service-host::html_answers`。修改协议、身份、容量、HTTP 隔离或浏览器支持边界时，
必须同步本文及相应行为测试。

当前已交付协议与隔离基座、会话偏好及迁移、请求指引、GUI/WebUI 输入区开关、可靠结果投影，
以及 Core 查看授权、可独立打开的已构建查看器、GUI/WebUI 会话结果卡及应用内弹窗、TUI 等价
开关与链接动作。下述验收使用真实浏览器及正式客户端组件，数据与服务由隔离夹具提供；
不以此声称真实模型一定遵循输出格式，也不覆盖其他操作系统的原生浏览器关联。

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

GUI/WebUI 仅为精确 `v1:seq` 的供应商可见助手正文建立结果上下文。现有 Markdown AST 在原文
位置替换顶层专用围栏，图片规范化记录 UTF-16 编辑映射，Core 围栏内部的图片形字符串保持原样。
引用链接仍在整篇 Markdown 中解析；用户、工具、思考和嵌套示例没有执行权威。流式或等待落盘
只有不可打开占位；大段 HTML 流式尾部不进入整块 Markdown 高亮。

HTML 卡片及专属样式按需加载，点击前不签发票据、不挂载 iframe。模块失败只显示该结果的纯文本
源码，不使周围会话失败。点击后复用共享 Dialog，最大化保持页面状态；关闭、消息身份替换或卸载
撤销授权，签发迟到也会立即撤销。无关事件增量复用原事件上下文，不重新挂载已经打开的页面。
嵌入子会话使用与窗口加载一致的显式父会话。复制临时链接说明由 Core 返回的有效秒数；复制失败
撤销本次授权。查看页与结果卡共用稳定错误分类，绝不显示原始凭据或服务诊断内容。

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

可信查看器仅允许自身响应用户点击复制源码；嵌入工作台对该可信框架显式委派剪贴板写入。
生成内容的响应策略仍禁止读写剪贴板，并保持不透明来源。

`content_response` 仅包装已授权内容，不能单独作为未鉴权路由使用。

## 临时查看授权与生命周期

已鉴权 RPC `service_create_html_answer_view_link` 接收 `request`：客户端生命周期 `clientId`、
共享目录 `locale` 与 `source`。后者包含规范会话 UUID、显式可空嵌入父会话、历史代次、事件
序号、语义修订和完整结果摘要。`service_revoke_html_answer_view_link` 按 `clientId` 与非秘密
`grantId` 撤销自身授权，包含尚在兑换中的记录。

返回地址为配置的 Core 来源下 `/html-answers/{session_id}/{artifact_id}#ticket=...`。
票据与租约使用操作系统安全随机源，熵不足或随机源失败直接拒绝，不回退到时间戳或 UUID。
秘密仅存于有容量与速率上限的内存表，相关 DTO 和记录不派生 `Debug`，不进入会话持久化。

查看器立即移除 URL 片段，以不带 Cookie 或 Core 实例令牌的 POST 调用
`/v1/html-answers/exchange`。接口要求唯一且精确匹配的可信查看器 `Origin`，不继承通用 RPC
的 CORS 层。单次消费与租约容量预留原子完成；源码校验失败、任务取消或撤销时不会留下可读
租约。租约只能访问单个结果，`/v1/html-answers/release` 可重复释放自己的有限能力。

Core 通过索引定位持久源码，不使用内存正文或 HTML 缓存代替读取。在与编辑、删除相同的全局
锁序下取得生命周期守卫，重新检查代次、事件修订、原始围栏和完整摘要。嵌入子会话必须提供
精确父归属，不扫描候选路径。历史开关关闭不影响读取。每次渲染请求重新验证；已删除或更改
结果不可通过旧链接重新读取，已经交付到浏览器的字节不会被远程抹除。

渲染监听器仅在合法查看时绑定回环地址，只提供 `/content/{lease}`；没有通用 RPC、附件、
发现或 WebSocket 路由。顶层文档读取被拒绝，实际内容要求 iframe 导航且仍附 HTTP 沙箱。
主 HTTP future 取消、停止或所属运行时释放时，撤销所有授权并中止独立监听器。
请求时立即裁掉到期项；监听器运行期间还按最短授权有效期清理空闲项，清理循环与监听器共用
同一个可中止任务。到期后始终不能访问，不依赖清理定时器是否已经执行。

调试构建只额外允许既有固定 Vite 工作台来源作为 iframe 祖先，使用与健康准入相同的 Rust 常量；
发行构建仍仅允许配置的查看器来源。嵌套内容策略同时校验整个祖先链，不接受任意客户端来源。

默认来源取实际绑定的 Core 与临时渲染端口。反向代理部署必须同时显式提供
`VULCAN_HTML_VIEWER_ORIGIN`、`VULCAN_HTML_RENDERER_ORIGIN` 和非零回环
`VULCAN_HTML_RENDERER_PORT`；来源必须是不同的规范 HTTP(S) origin。只配置其中一项会报错，
不从 Host 或转发头猜来源，也不创建公网监听和隧道。代理仍负责两个来源的可达性与原 Core
连接配置；查看功能不替代代理本身的鉴权。

查看器入口为 Vite 多入口构建产物 `html-answer-viewer.html`，仅导入专用展示代码及共享语言
目录，不导入服务发现或通用 RPC。有效租约才创建 iframe；源码模式以文本显示并移除执行
框架。pagehide 释放租约，过期移除框架，历史缓存恢复要求重新打开，不重用已消费的秘密。

生成响应在原文之前添加标准模式与 Esc 捕获脚本；持久源码、摘要及源码复制保持原字节。
该脚本只发送无参数的 `vulcan-html-answer-close-v1` 通知。可信查看器检查当前子窗口及
不透明来源，工作台再次检查当前查看器窗口及精确来源，然后关闭并恢复触发按钮焦点。
这不是通用宿主调用接口，不传递凭据或页面数据；独立浏览器查看器不会关闭浏览器窗口。
此通知仍不承诺终止任意阻塞浏览器事件循环的页面。

V1 助手投影为专用候选保留完整原始正文，绕过旧转录层的换行转换与 diff 拆块；执行权限仍只
来自 Core 标注。测试沿真实助手投影、转录解析及同一 Markdown AST 验证 CRLF、Unicode、
图片路径规范化、diff 和跨段引用共存时的源码范围。

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
rtk cargo build -p vulcan-service-host --example html_answers_viewer_fixture
rtk proxy pnpm.cmd --dir apps/vulcan-code-desktop run build
rtk proxy node apps/vulcan-code-desktop/tools/html-answers-security.mjs --fixture=target/debug/examples/html_answers_security_fixture.exe
```

独立查看器实际构建验收：先运行 WebUI 构建，再执行
`apps/vulcan-code-desktop/tools/html-answers-viewer-acceptance.mjs`，提供 `--fixture`、
`--bundle` 以及本机已有 Chromium 的 `--chromium` 路径；可用 `--report` 保存非敏感观测。
2026-10-07 Chromium 154.0.8037.0 已验证真实鼠标脚本交互、片段移除、父 DOM/桌面桥接隔离、
20 次预览/源码切换销毁、链接重放拒绝及页面关闭后租约返回 410。此测试使用合成源码回调；
内核另以真实隔离分段存储验证精确读取、修订/代次拒绝及会话删除后失效。

会话卡片真实验收：运行 `apps/vulcan-code-web/scripts/build-html-answer-acceptance.mjs`，构建到
忽略的 `target/html-answer-card-bundle`，再用 `html-answers-card-acceptance.mjs` 指向上述 Rust
夹具及该目录。夹具使用正式 Core HTTP 鉴权、RPC 和查看传输，仅提供合成窗口加载及查看方法；
不访问真实会话、发现文件或模型。当前 Chromium 验证了点击前零框架、嵌套页面真实按钮交互、
最大化保留状态、源码切换销毁、20 次完整打开关闭、390px 窄屏和可选分块加载失败的局部回退。
新增 Esc 跨两个框架关闭、触发按钮焦点返回与标准文档模式验证均通过。
授权循环遵守正式速率限制。生产构建不包含该验收页面，临时产物均可在验收后删除。

真实回归要求本机 Playwright 管理的 Chromium、Firefox 和当前 Electron。
可通过 `--chromium=<实际浏览器路径>` 指定 Chromium，通过 `--report=<本地文件>` 保存证据。
`--browser=electron` 等过滤只用于定向复现；报告保留执行矩阵，不能据此宣称三种浏览器全部通过。

验收只监听临时回环端口，不访问用户会话或付费模型。Electron 会短暂显示一个不抢焦点的独立
验收窗口并自动关闭，用真实鼠标输入验证跨来源页面；隐藏窗口不可靠地分发这类输入。
Windows 打包宿主可能虚拟化 LocalAppData，测试对浏览器路径调用 `realpath`，不修改系统配置。

2026-10-07 原型已在 Chromium 154.0.8037.0、Firefox 156.0、Electron 44.4.1 通过。
页面按钮正常执行；父 DOM、存储、Node 和桌面桥接不可访问；网络与导航逃逸没有到达合成禁止
目标；打开关闭循环后生成框架被销毁。此版本记录是验收证据，实际依赖版本以清单为准。

## TUI 操作与验证

输入区的 HTML 回答按钮、`Alt+H` 和 `/html` 共用比较交换保存入口。已有会话等待 Core 可靠确认；
新草稿通过首次提交的 `sessionSettings.htmlAnswersEnabled` 原子创建，提交失败保留草稿偏好。
`/html-results` 查看所选消息的结果；所选消息没有结果时定位最近一条实际结果。结果面板提供打开、
复制链接、查看及复制源码。未点击不请求票据，未确认的流式内容仅显示摘要，原始字节仍可检查。

链接只通过类型化 RPC 取得，交付时再次核对当前会话、代次、源码修订及完整身份。平台启动使用
`open` 的安全默认处理器，不启用 `insecure` 特性。启动失败显示临时地址并提供重新复制操作；
终端复制使用既有 OSC52，需要终端支持。链接不进入 TUI 的 Debug、日志或持久设置；系统打开
处理器必然取得该临时地址，测试浏览器则通过标准输入接收，避免测试命令行泄露。
模拟后端不启动真实浏览器；合成链接只测试动作与权威归属。

常规验证：`rtk cargo nextest run -p vulcan-code-tui` 和
`rtk cargo clippy -p vulcan-code-tui --all-targets -- -D warnings`。
实际传输验收先构建上述 Rust 查看器夹具和 WebUI 验收包，再给测试进程显式设置
`VULCAN_HTML_FIXTURE`（夹具绝对路径）、`VULCAN_HTML_BUNDLE`（包绝对路径）及
`VULCAN_HTML_CHROMIUM`（已有浏览器绝对路径），运行：

```powershell
rtk cargo test -p vulcan-code-tui html_tui_production_viewer_acceptance -- --ignored --nocapture --test-threads=1
```

该可选测试实际执行：正式 HTTP 加载 → TUI 结果面板动作 → 正式票据签发 → 终端剪贴板交付 →
同一链接在 Chromium 中真实按钮交互、片段移除与框架沙箱检查。附加在 WebUI 与实际 Electron
程序中打开同一 Core 身份的卡片，Electron 使用已构建的正式 preload，测试发现 IPC 仅绑定到
自有合成服务并拒绝子框架。2026-10-07 本机执行通过；临时 Electron 入口退出后删除。
所有内容、端口及服务进程均为测试自有，退出时回收；不读取服务发现或真实用户会话。
此项不声称验证每个平台的系统默认浏览器关联；Windows/Linux/macOS 的原生打开行为仍需各平台
发行验收，页面传输及隔离已按对应测试覆盖。

## 性能验收与清理

冻结的本地矩阵预热 3 次、测量 20 次，结果仅比较本机同构夹具，不外推为其他机器保证。
运行 `rtk cargo run -p vulcan-service-host --example html_answers_performance` 验证
1 KiB、64 KiB、最大单页及最大合法多页的纯 Core 解析。ASCII 坐标换算采用字节长度快路径；
Unicode 仍使用精确 UTF-16 编码。2026-10-07 改进后最大单页 P95 为 0.8604 ms，按原冻结
线性规则计算的预算为 2.3128 ms；首次未改进结果 12.8352 ms 超过 10.0624 ms，保留为失败证据。

浏览器运行 `apps/vulcan-code-desktop/tools/html-answers-performance.mjs`，显式提供
`--fixture`、`--bundle`、`--chromium`、`--report`。100 条普通消息的上下文启用组中位数/P95
为 8.20/12.20 ms，对照为 8.35/14.90 ms，通过对照 1.10 倍加 2 ms 的原预算；100 张未打开
真实卡片为零 iframe、零授权请求。打开关闭 20 轮的清理另由卡片与安全验收覆盖。

可清理产物为仓库 `target/html-answer-card-bundle/` 和 `target/html-answers-*.json`、截图，
以及 Cargo 对应 example/test 构建缓存；正常开发仍可复用这些构建缓存。`apps/vulcan-code-web/dist/`
与桌面 `dist/` 可重新构建，`output/` 属于正常运行所需同步产物，不作为垃圾删除。
本次新增的 Firefox `firefox-1549` 缓存只在不再执行该浏览器验收时按精确目录清理，其他已有浏览器
缓存不属于本任务。未创建独立用户 HTML 缓存、后台常驻辅助进程或计划任务。

## 设计参考

参考 [Answer me with HTML](https://github.com/QingYunA/answer-me-with-html)
的按需可视化及交互页面呈现思路。本期自行实现宿主协议，不依赖其 CLI 或本机参考仓库。
如后续复制受许可文件，必须保留实际版权、许可和修改来源，README 鸣谢不能替代许可。
