# 原生 VMM 功能对齐交接说明

交接日期：2026-09-07。工作目录：`D:/openvulcan/vulcan-code`。

## 零、交接后的追加授权：全量快照提交

用户在本说明生成后明确追加要求：为防止接手会话破坏现有成果，对当前仓库执行一次全量本地提交。因此，**本文件所在的交接快照提交是已授权的例外**，包含当前整个工作树的记忆、代码语义、供应商、界面等并行成果和在制测试，不仅是本任务代码；并显式纳入原本被忽略的本交接、主计划和审核台账。不推送、不打标签、不批量强行纳入其他被忽略的缓存或运行数据。

本快照用于可恢复基线，**不是完成声明，也不是全量验收通过声明**。下文“未提交／脏工作树”等描述是快照创建前的工作状态；实际接手基线以包含本说明的提交和接手时 `git status` 为准。PreCheck 测试结果未知、生产修复未实施及完整审核 0/5 均不因提交而改变。后续仍不得擅自重置、恢复或覆盖其他模块，新的中途提交仍需用户授权。

## 一、先读结论

**目标未完成。主体实现及大量验收已有成果，但仍在逐项对照、发现差异并补修；最终连续完整清洁审核为 0/5。不能宣称功能全部完成、只剩例行检查，也没有可靠的整体百分比。**

用户因当前会话用量不足要求生成本交接文档，交给新会话继续。本次交接没有创建新会话、提交代码、归档计划或修改目标状态。

新会话的第一件业务工作：**接续 PreCheck 历史业务 JSON／Token 预算同数据对照。外置样本和原生真实宿主测试已写入，原生测试结果未取得，生产修复尚未实施。** 详见第五节，不要从头重建已完成的浏览器或 PostAction 夹具。

## 二、完整目标与权威资料

用户完整目标：

> 完成 docs/plan/20260905-04-NATIVE_VMM_FUNCTIONAL_PARITY_REPAIR.md 中的原生 VMM 功能对齐修复及验收，使实际业务能力不低于外置 VMM（保留已确认产品差异，不涉及容灾）；完成后循环完整代码审核与自动修复，所有本地提交纳入范围，任何新问题使连续清洁计数归零，连续5轮完整审核未发现问题且全部验收完成后结束目标。遇到需要用户确认的产品或重大架构选择先确认；共享AI契约变更通知任务01a05bfd-e8a2-7121-a1da-9336b71a562a。

必读资料：

1. [完整实施方案](20260905-04-NATIVE_VMM_FUNCTIONAL_PARITY_REPAIR.md)：A—H 设计、产品冻结、完整回归矩阵、三层验收和结束条件。不能只读顶部最近进展就认为覆盖了要求。
2. [实施与审核台账](20260905-04-NATIVE_VMM_FUNCTIONAL_PARITY_AUDIT.md)：各批逐文件清单、真实通过／失败记录和遗留边界。文件很长，应先使用 Markdown 标题定位。顶部按新进展排列，较早的“运行中／待做”段落可能已经被后续同主题终态覆盖，不能孤立解读。
3. 仓库 `AGENTS.md` 及用户会话工程约束；本交接不能替代当前源码、协议和真实测试输出。

本交接把历史验收结论归纳为导航，不声称本次交接重跑了全部测试。交接时实时核对：

- 本仓库 HEAD 为 `df1dc7410be67d5acd24ac399f8d8fdb02005c04`。
- 外置基线最近实测为 `82f05fa5e3c7657e770fefd84e220fba5654ce07`，目录 `D:/openvulcan/VulcanMemoryMesh`；交接时 `git status --short` 仍为空。没有拉取远端或声称覆盖未读取的远端提交。
- 本仓库是大量并行修改的脏工作树。VMM、VCE、Provider、工作区存储、界面等修改交错，**整个脏树并非都属于本任务**。
- 不做中途提交。最终审核必须包括所有本地提交，不能以 `upstream..HEAD` 没有新提交缩小审核范围。

## 三、不可改变的边界

- 原生 Rust 与外置 gRPC 必须分离；不恢复独立 Go／FFI 运行时，不自动迁移、清空另一模式的数据。
- 原生固定用户 ID 1；默认记忆项目 ID 1 受保护，可改名不可删除。绑定按稳定 ID，不按显示名称。
- 每个会话只有一个写入归属；连带查询只扩大读取范围，不能扩大写入、替代和删除权限。
- 项目迁移移动内容及归属，不复制、不重新嵌入；冲突会话按既定后缀规则改名，个人画像不随项目迁移。
- **用户已明确确认：手动回收 Session／Turn 保留全部个人画像和项目画像，不增加连带删除画像选项。整项目清空仍删除项目画像、保留个人画像。** 此问题已解决，不再询问或按外置差异扩大删除范围。
- 归档只是分类，不禁用记忆；回收／遗忘禁用；显式重新启用不重放旧聊天、不恢复已永久清除内容。
- 真实模型质量测试按用户要求暂缓，**不是免验**。没有使用真实／付费模型完成质量验收，后续不能自行启动付费调用。
- 不涉及容灾，不清空用户目录，不为了测试修复其他并行任务的代码。遇到共同文件先确认具体逻辑归属。
- 最近两批没有修改共享 AI 契约，无需补发通知；以后真实改变共享契约时通知指定任务。
- 所有回复中文；代码注释英文第一行、中文第二行；文件编辑用 `apply_patch`。
- Shell 命令一律 `rtk` 前缀；PowerShell 内置命令用 `rtk proxy powershell -NoProfile -Command "..."`。文本读取指定 UTF8，注意外层 PowerShell 的变量展开。
- Rust 编译并发上限 4，涉及共享控制器／Provider／端口的测试串行 `--test-threads=1`。不要在 Windows 测试可执行文件还运行时重新链接它。
- 仅在权威资源变化后执行 `make.ps1 sync`；不要直接修改 `output/`。最近 JSON 业务序列化修复没有资源同步需求。
- 源码定位必须使用 `vulcan-codekit` 技能；新会话先完整读取该技能。先定位结构、真实字段与调用链，禁止猜测字段或增加投机性兜底。

## 四、全部工作域目前完成到哪里

以下“已有证据”不等于对应整域全部关闭；最终仍须逐条对照原计划验收要求。

| 工作域 | 已实施／已有验收证据 | 未闭环部分 |
| --- | --- | --- |
| A：宿主全文检索及融合 | 已接实际宿主全文检索、词法／向量来源和评分语义；主动写入、维护、策略消费等有真实控制器测试 | 外置／原生完整相同数据的候选、融合、最终结果矩阵及所有操作组合仍须核对 |
| B：PostAction 清洗与业务载荷 | 外置真实清洗 38 样本、四阶段对照；正常准入与配置生效已有真实服务测试；最近修复 JSON 转义，9 载荷／63 预算边界全部匹配 | 不能据此认定整条 PostAction 全阶段均完成双端对照；当前延伸到 PreCheck 的相关缺口未修完 |
| C：提炼观察水位 | 同会话顺序队列、近期直写边界、空／非空提炼、启动续跑等已有实现和真实队列验收 | 全矩阵的交错／迁移等组合仍需逐条核销，不能只凭数量通过 |
| D：相邻回合详情 | 动态前后三轮、数值身份及作用域边界、回收恢复／迁移有后端验收；浏览器观察过首／中／尾准确跳转 | 完整 AI 暴露与批量／所有管理组合最终复核仍保留 |
| E：Session／Turn 管理 | 归档、取消归档、回收、恢复、永久清除、重新启用已实现；实际浏览器 Session 全链与物理数据验收通过；画像独立保留已冻结 | 按 Turn 选择、批量多 Session 等浏览器操作尚未本轮完整验收；全管理矩阵仍需收尾 |
| F：全局／项目策略 | 检索、文本、召回提炼、画像、保留及调度字段已接保存／解析／命令消费；真实组件经 HTTP 到服务、队列和物理存储已有多批通过 | 完整生产宿主装配及全部界面／继承组合仍需逐项核对；不能用组件联网代替完整启动 |
| G：检索来源语义 | 生成来源与向量／全文／融合等检索来源已分开；L2 模型请求投影和来源测试通过 | 真实双端全部阶段、说明、分值／计数映射的完整对照仍需核销 |
| H：画像 Full／Split | 互斥正文、用户／项目分区、连带项目和实际缓存消费已有验收；手动与自动规则、生命周期已有真实测试 | 全场景双端对照、完整页面及最终暴露链审核仍保留 |
| PreCheck 全流程 | 混合 DETAILS／RAW_TURN 历史、配置隔离／冻结、L1／L2、采纳生命周期已有真实服务测试 | 当前新发现 RAW_TURN 重新序列化未统一外置转义，已加对照，结果与修复待完成 |
| 主动写入／删除／替代 | 真实主动写入、硬去重、候选池、作用域、替代与物理结果有专项验收；外置既有相关测试实际运行过 | 不等于全部双端同数据矩阵完成；维护后的查询不可见性要在最终回归核销 |
| 会话压缩与宿主缓存 | 已有压缩边界、来源写回、缓存／准入代际、会话状态立即生效等专项工作 | 与 C／D／E 全组合、旧消息编辑和首次发送等原方案回归项仍须完整复核 |
| 项目与绑定 | 稳定 ID、默认项目保护、绑定、迁移／清空等主体能力已有；浏览器看过保护提示 | 默认／补绑／改名／连带／迁移／清空／删除的完整用户操作验收未全部关闭 |
| 周期维护 | 实际生产循环验证启停、延长／缩短周期、截止锚点；手动清理与持久热／冷数据、到期时间有证据 | 完整规则组合和全宿主装配仍保留 |
| 资源／模型／模式隔离 | 既有资源覆盖、宿主统一推理及原生外置分流必须保留；近期没有改共同契约 | 资源页面、模式切换保留数据／配置等完整端到端矩阵仍需验收 |
| 真实模型效果 | 测试 Provider 证明确定性集成链，不证明模型质量 | 用户暂缓，未完成，不可标为通过 |
| 最终完整审核 | 台账和文件覆盖记录持续维护 | 还没开始计入合格完整轮次，0/5；所有本地提交必须纳入 |

### 最近已通过的重要批次

1. **PostAction 业务 JSON 对照（最新已闭环批次）**
   - `prompt.rs` 在目标预算和最终 L1 输出两处统一 Go JSON 的 `< > & U+2028 U+2029` 转义。
   - 首轮实际失败：2 项失败，HTML 成本原生 40／外置 62，Unicode 分隔符成本原生 37／外置 44，7 个历史边界集合不一致。不是模型问题。
   - 修复后：2 项对照通过（0.02 秒），9 载荷／63 边界一致；PostAction 相关 5 项通过（19.43 秒，含真实控制器和队列冻结）；提示词 4 项通过（0.02 秒）。这些命令有重复用例，不累加为独立覆盖数。
   - 生产库严格检查、限定格式／差异通过。含测试严格检查仍为其他模块既有 347 条诊断，原生及本批无命中，**整体严格门禁未通过**。
   - 日志：`C:/Users/lingeasy/AppData/Local/rtk/tee/1788775402_cargo_clippy.log`。
   - 样本独立重新生成 SHA256 相同：`be751edcd87137c0aadbcede47c132ecd8932a2e17f211e00bde2d794a3404f8`。

2. **真实浏览器 Session 生命周期**
   - 最终 1 项通过，1095 过滤，349.59 秒（含浏览器等待）；57 次范围内 RPC 全成功。
   - 七个目标回合、另一个会话一回合、三条直写记忆、用户与项目两类手动画像。
   - 归档／取消归档只变状态；回收准确集合及三十天期限；恢复原内容、原 ID 和物理向量；永久清除准确旧回执、七个作业及三个物理向量；未选会话／队列和画像不变；重新启用不重放。
   - 私有生产 HTTP／前端资产是真实的，但外围 VCE、AI 设置和终端 RPC 被守卫拒绝，**不代表完整生产宿主启动通过**。
   - 证据：`C:/Users/lingeasy/AppData/Local/Temp/vulcan-vmm-workbench-lifecycle-20260907-04/handshake`。
   - 十个确认文件包括 saved、restored、archived、unarchived、recycled、sources-restored、recycled-again、purged、restarted、complete，统一 `browser-` 前缀。
   - 私有浏览器、HTTP 服务和控制器已关闭，旧 127.0.0.1 地址不再有效。真实页面前端相关五文件 42 项通过（2.69 秒）。

3. **策略组件／实际后台循环**
   - 历史台账中已有：检索 16 字段；文本 7 字段及四角色入库；召回提炼 13 字段；全局画像 18／项目画像 9；全局保留 9／项目保留 6；三个工作器调度字段。
   - 包括正常策略保存、调用方隔离、新命令生效、已准入命令冻结、物理存储重建。
   - 调度链最终前端 1 项 179.94 秒，后端 1 项 191.41 秒；实际跨过分钟截止，不使用假时钟。
   - 最近完整原生整组历史结果为 130 通过、1 显式联网忽略、964 过滤、464.33 秒，**发生在新增浏览器和最新 JSON 对照之前**，不能当成当前树全量通过。
   - 前端记忆整组历史结果 113 通过、1 显式联网跳过；联网另行显式执行，不以忽略代替通过。

## 五、精确续接点：PreCheck 同数据对照（未完成）

### 5.1 当前事实与推测的界限

已从当前源码查证：

- 外置 `internal/app/usecase/precheck.go`：
  - `loadRecentTurnContexts` 先预算选行，再将已完成且非空详情投影成 `DETAILS`，其余用 `RAW_TURN`。
  - `preCheckTurnBudget` 对已完成详情取详情预算，否则取持久脱水 JSON 预算，缺省时才重新估算。
  - `trimRecentTurnsByBudget` 从最新向前保留连续后缀，正预算至少保留最新一条；不把当前用户提示词加入历史预算。
- 外置 `postaction_intake.go::buildPostActionTurnPayload` 经 `json.Marshal` 形成脱水正文和预算，包含上述五种额外转义。
- 原生 `client.rs::recent_precheck_turns` 从真实快照筛选同用户／项目／Session，窗口后调用 `precheck_turn_context`，再按实际 content 估算历史成本。
- 原生 `client.rs::precheck_turn_context` 仍对 `NativeTurnDialogue` 直接 `serde_json::to_string`，没有调用统一转义。
- 上轮新增 `escape_postaction_json` **仍私有地位于 `prompt.rs`**；`dialogue.rs` 尚未新增共享转义函数。当前 PostAction 修复只覆盖 L1，不自动覆盖 PreCheck。

这是已查明的独立表示路径差异。是否在新真实宿主样例中导致什么准确失败，**尚未取得原生测试终态，不要写成已复现并修复**。

### 5.2 已落盘但未验收完成的改动

| 文件（相对仓库根） | 精确状态 |
| --- | --- |
| `crates/vulcan-service-kernel/src/native_vmm/fixtures/precheck_oracle_test.go` | 新增并已运行成功；通过 Go 只读覆盖层直接调用外置真实清洗／序列化／预算／上下文方法，存储行交付使用外置既有测试桩 |
| `crates/vulcan-service-kernel/src/native_vmm/fixtures/precheck_history.json` | 已由外置实际生成：4 个混合状态上下文、10 个正数预算边界；不是手写预期 |
| `crates/vulcan-service-kernel/src/native_vmm/precheck_history_parity_acceptance_tests.rs` | 新增；接真实客户端／服务保存／L1 捕获，对照完整 content 字节和预算选中身份；尚无原生通过证据 |
| `crates/vulcan-service-kernel/src/native_vmm/precheck_history_policy_acceptance_tests.rs` | 已注册上述子模块；前四条来源改用同一外置生成样本；原有测试保持，额外调用十次预算探测；最终计数改为 `17 + external_calls`／`15 + external_calls` |
| `client.rs`／`dialogue.rs`／`prompt.rs` | 此未完成批次尚未改生产逻辑；保留上批 PostAction 已通过修复及更早脏树修改 |

外置生成命令已通过：`TestNativeParityPrecheckOracle`，4 上下文、10 边界，包耗时 0.707 秒。

样本前两条实际在原生调用后台队列完成分析，后两条保留真实待分析作业。第三条含 `< > &&`；第四条含 U+2028／U+2029 和字面 `\u003c`。原生不是直接伪造 Core 行入库。

原生扩展保留原测试的不同宿主／项目／Session 隔离、策略继承、已准入冻结、完整持久回合与队列不变、重新创建运行时等断言。额外预期只允许外置 1—4 的来源序号映射到真实接纳的数值 Turn ID，**不允许归一掉正文转义和预算差异**。

### 5.3 中断的进程状态

启动过：

```powershell
rtk proxy cargo test -p vulcan-service-kernel --lib real_controller_precheck_history_policy_preserves_mixed_turns -- --test-threads=1 --nocapture
```

- 原工具进程句柄为 `5517`，最后明确取得的输出仅为编译 `vulcan-service-kernel`。
- 用户主动中断会话，后续查询该句柄返回 `Unknown process id 5517`。
- 交接时按进程名查询 `cargo`、`vulcan_service_kernel*` 无匹配进程输出。
- **没有取得通过或失败终态，不能引用通过数，也不能说已确认失败。** 新会话先核对无本方同一可执行文件运行，再重新执行这个唯一目标；无需反复轮询旧句柄。
- 此 PreCheck 在制批次尚未写入主审核台账终态，**本交接是额外在制状态记录**。完成后应把证据与执行变更总结补回主方案／台账。

### 5.4 建议的最短正确续接顺序

1. 读取本节涉及的准确源码与新增测试，核对工作树没有被并行会话改动。
2. 重新运行上述真实控制器 PreCheck 目标，取得准确终态。若先出现夹具／编译问题，只修已确认的测试问题，不先改生产使它“看起来通过”。
3. 若实际证据确认 JSON 转义导致历史正文／预算选择偏差，考虑将已通过的五字符业务 JSON 转义放入 `dialogue.rs` 共用，由 PostAction 目标预算、PostAction 实际输出、PreCheck RAW_TURN 序列化明确调用。先核对调用链；这只是当前建议，**尚未实现**。
4. 保持已提炼 DETAILS 直接使用正文、不额外包 JSON；保持字面反斜杠不被二次解释；不改宿主原始会话、不重写已存回合、不重新提炼历史，不扩大到其他模型提示词协议。
5. 修复后复跑 PreCheck 真实控制器目标、上批 PostAction 对照／真实预算冻结、提示词回归；再按风险运行相关原生整组，记录真实终态。
6. 完成限定格式、生产严格检查和含测试严格检查；后者历史有其他模块 347 条诊断，核对本批是否新增，不宣称整个门禁通过。
7. 增补 PreCheck 样本来源文档、独立再生成一致性检查、主计划进度及审核台账的文件清单／失败修正／执行总结。

## 六、可直接复用的入口与命令

所有本仓库命令工作目录为 `D:/openvulcan/vulcan-code`。

```powershell
rtk proxy cargo test -p vulcan-service-kernel --lib real_controller_precheck_history_policy_preserves_mixed_turns -- --test-threads=1 --nocapture
rtk proxy cargo test -p vulcan-service-kernel --lib postaction_parity_tests -- --test-threads=1 --nocapture
rtk proxy cargo test -p vulcan-service-kernel --lib postaction -- --test-threads=1 --nocapture
rtk proxy cargo test -p vulcan-service-kernel --lib native_vmm::prompt -- --test-threads=1
rtk proxy cargo clippy -p vulcan-service-kernel --lib -- -D warnings
rtk cargo clippy -p vulcan-service-kernel --lib --tests -- -D warnings
```

格式仅指定本任务改动的文件：

```powershell
rtk proxy rustfmt --edition 2024 --config skip_children=true <准确文件列表>
rtk git diff --check -- <准确文件列表>
```

不要对运行中的同一测试可执行文件重新编译／链接。长期验收用 `rtk proxy cargo`，避免优化代理延后输出失败诊断。`functions.wait` 与 shell 的 `write_stdin` 句柄不同，不可混用。

### 外置只读覆盖层

路径：`C:/Users/lingeasy/AppData/Local/Temp/vmm-postaction-parity-20260907-01/overlay.json`。

当前映射外置业务包中的两个虚拟测试文件到本仓库：

- `native_parity_oracle_test.go` → `fixtures/postaction_oracle_test.go`。
- `native_precheck_oracle_test.go` → `fixtures/precheck_oracle_test.go`。

PreCheck 文件使用前一个文件定义的 `parityBudget`，两个映射须同时保留。它只改变 Go 编译输入视图，不修改外置仓库。

工作目录改为 `D:/openvulcan/VulcanMemoryMesh`，设置进程环境变量 `VULCAN_PRECHECK_ORACLE_OUTPUT` 为准确生成路径，然后执行：

```powershell
rtk proxy go test -overlay C:/Users/lingeasy/AppData/Local/Temp/vmm-postaction-parity-20260907-01/overlay.json -p 4 -parallel 1 -count 1 ./internal/app/usecase -run '^TestNativeParityPrecheckOracle$' -v
```

PostAction 入口为 `TestNativeParityPostActionOracle`，环境变量 `VULCAN_PARITY_ORACLE_OUTPUT`，规则说明见 `crates/vulcan-service-kernel/src/native_vmm/fixtures/POSTACTION_PARITY.md`。独立再生成应输出私有临时文件再比较，避免在 Rust 编译时更改 `include_str!` 的输入。

当前旧 PowerShell 无 `Get-FileHash`，可用 `rtk proxy certutil -hashfile <准确文件> SHA256`。不要把失败的哈希命令当成已校验。

### 浏览器验收后续复用

- 测试入口 `real_controller_native_workbench_over_http` 为显式 ignored 测试，运行须 `--ignored --test-threads=1 --nocapture`。
- 相关源码 `workbench_http_acceptance_tests.rs`、`workbench_lifecycle_acceptance_tests.rs`，以及 `policy_http_acceptance_tests.rs`。
- 生产前端资产历史目录：`C:/Users/lingeasy/AppData/Local/Temp/vulcan-vmm-workbench-20260907-01/assets`；若前端源码变更应重新独立构建，不假定旧资产反映新代码。
- 环境变量 `VULCAN_VMM_WORKBENCH_ACCEPTANCE_DIR` 指向新私有握手目录，`VULCAN_VMM_WORKBENCH_ASSETS_DIR` 指向准确资产目录。不要重用已含成功标记的旧目录运行新验收。
- 读取本次监听地址，不复用旧端口；发现文件含临时令牌，不打印令牌。只允许范围内真实 VMM RPC，外围被拒绝不意味着完整宿主已经通过。
- 浏览器控制前读对应技能及工具文档。页面交互按实际可见 DOM／AX 操作，不用注入伪造响应绕开服务。
- 关闭浏览器后再通知服务完成，避免服务关闭后页面自动重试转入其他连接。
- 历史向量检查注意：`VmmDataPlaneHandle::load_vectors` 在缺少引用向量时返回严格错误，不返回空数组。永久清除要逐个身份验证准确缺失错误；其他错误不能冒充已删除。
- 恢复时物理回合数组可能按追加顺序，而 Core 导出按稳定身份排序；比较仅规范化已核对的顺序表示差异，不能抹掉字段差异。

## 七、后续整体路线与结束条件

完成 PreCheck 当前子项之后，继续原计划，不把目标缩成“这一处修完就结束”：

1. 逐条提取 A—H 及完整回归矩阵的要求，建立明确的完成／待验／不满足／缺证据标记，链接当前源码和实际输出。
2. 补齐原生／外置同一合成数据的完整业务阶段对照，不能把外置自有测试和原生自有测试分别通过等同于同数据对照。
3. 补齐 Turn 级／批量 Session、项目操作、资源及模式切换等实际用户入口；完整生产宿主启动单列验收。
4. 真实模型质量保持“用户暂缓，未完成”；到需要这一最终门槛时明确请求用户允许的模型和调用范围，不能以受控 Provider 代替。
5. 在功能与验收确实全部完成后，再循环完整代码审核。每轮覆盖全部要求、文件和所有本地提交，任何新问题清零；连续五轮合格且全部验收完成才可结束目标。
6. 在计划末尾写完整执行变更总结后，才按用户既定规范归档；当前不得归档或标记目标完成。

## 八、交接本身的执行记录

- 核心工作：创建本说明，归纳完整目标、所有工作域、最近通过证据、未完成的 PreCheck 在制改动和准确下一步。
- 文件变更：仅新增本交接文档；没有在本交接请求中继续实现功能、删除文件或更改业务策略。
- 关键核对：当前 HEAD、外置干净工作树、PreCheck 新测试已注册、生产转义仍仅位于 PostAction、旧测试句柄不存在且进程名查询无匹配。
- 遗留事项：PreCheck 原生终态未知，其生产修复／复验和主台账记录待新会话继续；全目标仍未完成，完整审核 0/5。

## 九、发给新会话的接手提示词

请在 `D:/openvulcan/vulcan-code` 接手原生 VMM 功能对齐任务。先完整阅读 `docs/plan/20260907-01-NATIVE_VMM_PARITY_SESSION_HANDOFF.md`，再读取关联主计划与审核台账，并按当前源码和 Git 状态核实交接。

完整目标不变：完成主计划 A—H 及全部验收，使原生业务能力不低于外置（保留已确认产品差异，不涉及容灾）；然后审核全部本地提交，任何问题清零，连续五轮完整无问题才结束。当前为 0/5。不要中途提交、归档或宣称完成；保护并行脏树；真实模型质量按用户要求暂缓但没有免验；共享 AI 契约变化通知任务 `01a05bfd-e8a2-7121-a1da-9336b71a562a`。

第一步接续交接第五节的 PreCheck 同数据对照：外置 4 个上下文／10 个预算边界已生成，原生真实宿主测试已写入但中断后没有终态，生产修复尚未实施。先确认没有相关测试进程占用同一可执行文件，重跑准确目标取得证据，再按查明的调用链修复与复验，不能改外置期望掩盖差异。已有 PostAction 9 载荷／63 边界和浏览器 Session 生命周期通过，不要从头重复搭建。完成当前子项后继续完整目标，逐项维护证据与文件覆盖台账。
