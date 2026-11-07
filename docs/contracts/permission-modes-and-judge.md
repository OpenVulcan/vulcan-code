# 权限模式与判定审批管线

## 归属

本契约约束会话权限模式、工具调用权限闸门、判定模型（本机、云端或 API）、复核 LLM、用户审批，以及它们的
系统级设置、项目级策略和 WebUI/TUI 设置界面。修改下列任一入口时，先读本文，再以当前源码与测试为准。

## 红线

- **防护目标是 AI 误操作，不是 AI 刻意绕过。** 工作区内写入不审核内容，AI 总能写脚本再执行；能做的只是在
  执行处按模式设闸门。因此确定性规则的缺陷只按“正常写出的命令是否会误触”衡量（如注释中的撇号使引号范围错位），
  变量、命令替换或拆写命令名等刻意改写不属于缺陷，也不为其加固。
- **管线顺序固定，错误绝不放行。** 模式闸门 → 确定性规则 → 判定模型（可选）→ 复核 LLM（可选）→
  用户。每层只能放行、拒绝或交给下一层；后端出错、超时、输入超限或读不到模式时一律交给下一层或拒绝。
  没有可回复的前端时用户层的结果为拒绝。
- **放行只对本次调用有效。** 判定模型、复核 LLM 与用户的“允许本次”都不形成持久规则。
- **高风险与判定拦截不能由单层放行。** 命中高风险规则时判定模型的 allow 不生效；判定为 block 或命中
  高风险的调用，复核 LLM 只有在用户开启“允许 LLM 放行判定器拦截项”时才能放行，否则交给用户。
- **只有自动审批使用判定层与复核层。** 仅可查看与工作区修改的语义是超出范围的调用必须由人批准，直接交给
  用户；自动审批需要已配置的判定器（已保存的 `backend`），未配置时不可选择。
- **无人值守只属于自动审批。** 只有自动审批模式能开启无人值守；切换到其他模式时一并关闭，切回自动审批也不
  恢复，必须再次确认拒绝策略。其他模式下读到无人值守视为不可读状态而拒绝调用。
- **无人值守永不询问用户。** 开启无人值守的会话（含其子代理）不调用用户审批，用户层的结果恒为拒绝并把
  指引写回代理；复核 LLM 放行（仍受“允许 LLM 放行判定器拦截项”约束）才执行。同一调用在同一用户请求下
  连续被拒 `UNATTENDED_REVIEWED_REPEATS` 次后不再复核、直接拒绝，避免代理反复改写理由绕过。
- **复核规则只是复核材料。** 用户在设置中写的自然语言复核规则（拒绝 / 问我 / 放行）只以 `<review_rules>`
  区段进入复核 LLM 输入，不改变确定性规则、高风险或判定拦截约束、项目类别档位与无人值守拒绝策略；项目配置
  读取失败时复核报错，不退回系统规则。长度上限只在 `MAX_REVIEW_RULE_CHARS` 定义。
- **复核附加提示词只用于用户在场的复核。** 用户写给复核器的附加提示词只在未开启无人值守的复核中以
  `<review_prompt>` 区段进入复核 LLM 输入，排在复核规则的拒绝 / 问我分组与项目策略之后，不能放宽它们；
  无人值守复核只读拒绝策略。会话自有文本读取失败时复核报错，不退回系统默认。长度上限只在
  `MAX_REVIEW_PROMPT_CHARS` 定义。
- **配置修改只由用户批准。** `mcp_config` 与 `provider_config` 的修改动作属于配置类别：完全权限放行，其他模式
  （含自动审批）跳过判定模型与复核 LLM 直接交给用户，无人值守会话直接拒绝；审批卡显示内核计算的变更预览，
  预览出错即拒绝。详见 [`ai-configuration-tools.md`](ai-configuration-tools.md)。
- **审批等待到期即拒绝。** 系统级 `approvalTimeoutSeconds` 大于 0 时，用户审批等待该秒数后按拒绝处理
  （判定记录结果 `approval_expired`），绝不按放行处理；0 表示一直等到回答。上限只在
  `MAX_APPROVAL_TIMEOUT_SECONDS` 定义，截止时间由服务掌握，客户端倒计时只是显示。
- **代理理由只是复核材料。** 命令工具的理由参数（`shell_command` 的 `title`、`terminal_session` 的 `purpose`）与 `declare_intent` 只进入复核 LLM 输入的 `<agent_reason>` 区段，
  不改变任何确定性规则、判定档位或放行条件；理由长度上限只在 `MAX_PERMISSION_REASON_CHARS` 定义。
- **模型是系统级，策略是项目级。** 后端、设备、模型登记表与复核路由只存在于用户全局配置；项目配置
  只保存各类别档位，Schema 中没有模型或后端字段，不允许不同项目使用不同模型或后端。
- **OpenVulcan 只经云端判定参与。** 三类后端面向用户的名称为本机（`local`）、云端（`cloud`，OpenVulcan
  官方判定，经已登录账号调用边缘端点）与 API（`api`，用户以自己的供应商账号接入的判定路由）。OpenVulcan
  账户供应商（`credential_directory_visibility: core_account`）的路由不能作为 API 判定：保存
  `permission_judge` 判定路由时拒绝，判定与保存后端时再次拒绝（含手工编辑的配置），界面候选也不列出。
- **策略文本与权重一致。** 本机模型的策略只来自已登记模型包；云端与 API 判定读取随构建发布的同一副本，云端还拒绝
  策略版本或检查点与该副本及官方清单不符的回答。类别与
  档位是固定枚举，代理与工具参数不能添加策略文本；未知档位报告为设置问题，不静默改成默认值。
- **只有显式选择写入设置。** 判定出厂为“未设定”；初始化引导中选择“暂不启用”不写入任何内容。启动或刷新
  不得回写权限模式、判定设置或项目策略（AGENTS 第 5 条）。
- **官方下载只走固定地址并完整校验。** 官方模型包清单（`known-packages.json`）按格式列出已发布变体；只有
  清单固定了 `MANIFEST.json` 摘要、总大小与至少一个托管平台目录的变体才能下载，未发布地址时不显示下载按钮、
  服务拒绝。同一格式的各平台（`huggingface`、`modelscope-cn`、`modelscope-intl`，每个至多一次）提供同一份
  模型包，目录为固定到提交的 HTTPS 地址；每个平台一个下载按钮，由用户选择，不写入设置，下载失败直接报错，
  绝不自动改用其他平台。
  下载先校验清单摘要与总大小，再逐个文件校验大小与摘要，全部通过后原子发布到
  `<user-vulcan-root>/code/judge-models/<包>-<格式>-<清单摘要前 12 位>`，再按普通登记流程验证并登记；
  下载绝不自动投入使用，也不替换已登记目录。用户自选本机路径登记照常可用。
- **推荐格式按设备与显卡决定。** 推荐格式取生效设备（已选设备，否则推荐设备）：GPU 设备优先
  `fp16-mixed`，CPU 优先 `fp32`，只在清单为该格式列出该设备时推荐；CUDA 上最低计算能力低于
  `FP16_MIN_CUDA_COMPUTE_CAPABILITY` 时不推荐半精度。计算能力经 `nvidia-smi` 单独查询，旧驱动不支持时视为未知，
  不阻止推荐。推荐只是标记，不改写设备或模型选择。
- **模型包格式只有一种接口。** 官方模型包来自 decision-shell 运行包仓库（Mini、Flash、Standard），清单
  `format_version` 为 2，旧格式明确拒绝。计算图一律输入 `input_ids`、`attention_mask` 并输出 `logits`；
  `runtime-config.json` 的 `encoding` 声明文本编码（`marker-head` 或 `decision-prompt`），`devices` 声明已验证
  设备，加载到未声明的设备直接拒绝（部分计算图在 CPU 上会崩溃）。
- **批次上限来自模型包。** `max_tokens`、`max_request_items`、`micro_batch`、`padded_batch_tokens` 只在模型包
  `runtime-config.json` 中定义，加载报告与 worker 状态原样投影。并发的受管控调用（同一模型回复中的多个工具
  调用）在服务端排队合并，由下一个持有 worker 的调用方按 `max_request_items` 拆分提交；宿主按长度排序，每次
  计算图运行不超过 `micro_batch` 行与 `padded_batch_tokens` 个填充词元，单条超过填充预算的调用单独运行。

## 权限模式

| 能力 | 仅可查看（`read_only`） | 工作区修改（`workspace_write`，默认） | 自动审批（`auto_approve`） | 完全权限（`full_access`） |
| --- | --- | --- | --- | --- |
| 文件写入工具 | 交给用户 | 工作区内直接放行，其余交给用户 | 工作区内直接放行，其余进入管线 | 放行 |
| `shell_command`、`terminal_session` | 已知只读命令放行，其余交给用户 | 已知只读命令放行，其余交给用户 | 已知只读命令放行，其余进入管线 | 放行 |
| MCP、插件与其他写入能力 | 已知只读放行，其余交给用户 | 已知只读放行，其余交给用户 | 已知只读放行，其余进入管线 | 放行 |
| 配置修改（`mcp_config`、`provider_config`） | 交给用户 | 交给用户 | 交给用户（无人值守时拒绝） | 放行 |
| 无人值守 | 不可用 | 不可用 | 可开启 | 不可用 |

- “进入管线”指判定模型 → 复核 LLM → 用户；“交给用户”跳过判定层与复核层。

- 文件写入工具指 `file_write`、`file_edit`、`apply_patch` 与 `file` 的移动、删除动作；`file_read` 与 `file` 的
  列表、属性动作不经闸门。
- “工作区内”不含 Git 元数据：路径在工作区根目录之下任意深度含 `.git` 组件（规范化已存在部分后比较，忽略
  大小写与结尾的点或空格）时按工作区外处理，因为 Git 会运行其中配置的命令（`core.fsmonitor`、钩子、差异
  驱动），写入后再执行“只读”的 `git status` 即可运行任意程序。
- 已知只读命令清单唯一定义在 `tool_permission/command_rules.rs`：命令带有写入文件、改变系统状态或运行其他
  程序的选项（如 `sort -o`、`date -s`、`uniq` 的输出文件、`tree -o`）时不是只读；PowerShell 中内容本身只读的
  裸分组 `(...)`（可后接 `.Count` 等属性访问）与普通变量赋值 `$name = <只读命令>` 视为只读，`$env:`、偏好变量
  与 `PS` 前缀变量的赋值不是只读。
- 命令判定使用实际启动 shell；工具 Schema 的 shell 默认值由执行器同一构建快照派生，显式 `auto` 同样解析到该值。
  终端 `send` 不能证明当前前台程序仍是原 shell，因此文本输入不走只读快捷放行。
- 反斜杠紧邻引号、引号外的 `^`、弯引号及未闭合引号均视为无法可靠解析，进入后续权限层。
  注释（词首 `#`、PowerShell `<#`，cmd 除外）、here 文档 `<<` 与 PowerShell here 字符串 `@'`/`@"` 中的引号字符
  不被 shell 当作引号，同样视为无法可靠解析。出现这类歧义时，高风险匹配再以“引号与 `^` 均为普通文本”
  的方式切分一次，任一次命中即为高风险，引号技巧不能让高风险命令降为普通命令。
  带 `/` 或 `\` 的程序名不套用只读白名单；高风险匹配仍识别带路径的程序。cmd 只允许已声明的只读内建命令，
  外部程序及环境变量展开必须进入后续层，不修改子进程环境来改变程序搜索行为。
- 已知只读的 `git` 只接受不改变配置与所运行程序的全局选项（`-C`、`--no-pager`、`-P`、`--no-optional-locks`、
  各类 pathspec 选项、`--no-replace-objects`）；带 `-c`、`--config-env`、`--git-dir`、`--work-tree`、`--bare`、
  `--exec-path` 或其他全局选项的调用不是只读。`git grep -O`/`--open-files-in-pager` 与
  `git ls-remote --upload-pack`/`-u` 会运行指定程序，同样不是只读。git 选项解析器接受合并短选项
  （`-nO<程序>`）与长选项缩写（`--open=<程序>`、`--upl=<程序>`），因此含该字母的任一短选项组合、该长选项的
  任一前缀都按指定程序处理。
- 默认安装的 LuaSkills 入口按完整规范名称精确分类（`tool_permission/builtin_luaskills.rs`，同一清单也决定
  计划模式可见性）：`vulcan-codekit` 的 `repo-map`、`ast-detail`、`ast-tree`、`rg`、`markdown-menu`、
  `node-source` 为只读；`vulcan-codekit-patch` 按 `file` 与 `patches[].file` 视为文件写入，相对路径以 `PWD` 为基准
  （省略时宿主注入工作区根目录；显式绝对 `PWD` 拼接后判断，显式非绝对 `PWD` 交给后续层）；`vulcan-curl-get`
  无 `download_to` 时为网络读取，绝对 `download_to` 视为文件写入，相对路径（按服务进程目录解析）交给后续层；
  `vulcan-curl-post`、`vulcan-curl-request` 与其他未列出入口沿用名称只读启发式。`call_luaskill` 按其目标入口
  分类，规则与直接调用相同，参数无法解析时不放行。
- 模式保存在会话配置（`PermissionMode`，会话元数据模式版本见
  [`session-metadata-migrations.md`](session-metadata-migrations.md)）；新会话默认值只存在于运行时内存。
- 子代理会话使用其根会话的模式；读不到模式时拒绝调用。
- 草稿会话的显式选择随首次提交的 `sessionSettings.permissionMode` 发送并在首个提示边界写入；未选择时
  不发送值。已存在会话通过 `service_set_session_permission_mode` 修改，只影响下一次工具调用。
- 事件窗口的 `session_config` 是界面显示模式的唯一权威：可靠确认（`durability: durable`）与仅元数据重连都携带
  写入后的 `access`（`permission_mode`、`unattended`），窗口据此更新。输入区的保存回执只在组件内存中过渡，
  弹窗或切换导致输入区重新挂载后必须仍显示已保存模式，不得回落到重新加载前的投影。

## 无人值守模式

- **范围**：按会话开启，保存在会话配置 `AccessConfig.unattended`（`UnattendedConfig { deny_policy }`）；子代理
  跟随根会话。只有已保存且处于自动审批模式的会话可以开启，草稿入口不可用并说明原因。修改通过
  `service_set_session_unattended`，只影响下一次工具调用；非自动审批模式下开启请求被拒绝。
  `service_set_session_permission_mode` 切换到其他模式时在同一次写入中清除该配置，回执携带写入后的
  `unattended`，界面以回执为准。
- **定位**：无人值守只替代管线最后的“用户”一层，不是覆盖整个审核的规则。确定性规则与判定模型已放行的调用照常
  执行、不经过拒绝策略；要彻底禁止某类操作应收紧项目类别档位。界面文案必须说明这一范围，示例只写复核时适用的
  规则，避免被理解为全局禁令。
- **拒绝策略**：用户确认的文本写入会话，并以 `<deny_policy>` 区段交给复核 LLM；进入复核的调用中策略涵盖的必须拒绝。长度
  上限只在 `MAX_UNATTENDED_DENY_POLICY_CHARS` 定义。开启对话框的预填值按项目默认 → 系统默认 → 空白分层读取
  （`service_session_unattended_default`，返回来源 `project`/`system`/`none`）；预填只是默认值，用户确认后才写入，
  之后修改默认值不影响已开启的会话。
- **默认值存储**：系统默认为用户全局判定设置的 `unattended_deny_policy`
  （`service_unattended_system_default` / `service_save_unattended_system_default`）；项目默认为项目配置
  `permission_judge.unattended_deny_policy`，缺省表示跟随系统默认
  （`service_project_unattended_default` / `service_save_project_unattended_default`，`null` 恢复跟随）。
- **管线行为**：需要用户批准的调用交给复核 LLM，答案只能是 `allow` 或 `deny`；放行才执行，其余拒绝，判定记录
  结果为 `unattended_denied`。未配置复核 LLM 时全部拒绝。
- **拒绝回馈**：拒绝文本包含复核理由。只有理由可能改变结论时（调用未说明理由、复核器已实际复核、且复核器可以
  放行该调用）才邀请代理在 `title` 或 `purpose` 中写明理由、或调用 `declare_intent` 后重试一次；其余情况（已说明理由、复核器不可用
  或未配置、高风险或判定拦截且不允许 LLM 放行）告知不要重试，改用其他方式，或跳过该步骤留待用户后续指示，
  继续完成其余部分。拒绝文字与是否邀请由同一判断决定，不能分别演化。
- **连续拒绝上限**：按会话在内存中记录最近一次被拒调用的指纹（工具与全部参数，不含命令工具的理由参数 `title` 或 `purpose`；内容或 `cwd`
  不同即为不同调用）与所属用户请求（回合与请求文本，后续回合发送相同文本也算新的请求）；
  只有进入审批的调用计数。放行清零，换成另一次被拒调用时重新计数，新的用户请求重置；不进入审批的调用与
  `declare_intent` 不影响计数。上限为连续次数，不阻止之后的同一调用。

## 代理理由

- `shell_command` 的 `title` 必填，是命令的简短标题（上限只在 `MAX_SHELL_COMMAND_TITLE_CHARS` 定义，超长即拒绝）；
  `terminal_session` 的 `purpose` 可选，说明该调用的目的。两者执行时都被忽略，只在升级到复核时作为
  `<agent_reason>` 的 `purpose` 交给复核 LLM。`shell_command` 的 `title` 同时显示在命令卡与会话活动面板上；
  它们都是代理的自述，不是放行依据。
- `shell_job` 只查看、等待或停止本会话自己的命令任务，权限分类为 `Ungoverned`，不进入判定模型与复核 LLM。
- `declare_intent` 是始终可见、计划模式可用的原生工具，只用于说明被拒调用的重试理由，不是备注工具。权限闸门
  只在“最近一次受管控调用被拒、且拒绝文字邀请了 `declare_intent`”时准入，并要求仍在同一用户请求下；其他任何
  时候直接拒绝且不记录（`DECLARE_INTENT_UNINVITED`）。一次邀请只允许一次声明；下一次受管控调用（无论结果）
  也会结束邀请。目前只有无人值守拒绝会发出邀请；有人值守的拒绝要求不重试，因此不邀请。
- 已接受的意图只由下一次（任意）工具调用消费，作为 `declared_intent` 交给复核 LLM，之后失效；该次重试视为
  已说明理由，再被拒时不再邀请。

## 判定设置

- **系统级状态**：没有启用开关；`backend` 为未配置（`null`）、`local`、`cloud` 或 `api`，已保存的后端即表示
  判定器已配置。本版本无法识别的已保存 `backend` 或 `device` 值在读取时视为未选择（不影响服务启动，也不在
  读取时改写文件），用户重新选择后由下次保存写入。保存后端前服务端校验其可用（本机须有使用中模型，API 须已选择非 OpenVulcan 账户的判定路由，云端须已登录
  OpenVulcan 账号并选择云端模型；新选的云端模型或刚切换到云端时，模型必须在服务端当前清单中，未变的
  已保存选择不再对照清单，使运营方停用模型后其他设置仍可保存），未配置时其他选择照常
  保存。工作台投影 `permissionJudgeBackend` 只供界面决定入口：未配置时自动审批显示为灰色、点击打开设置页，
  并隐藏项目策略；已配置时全部可用。
- **本机后端**：模型包登记、切换、回退与移除；包内声明策略版本与放行余量，载入时验证。设备探测给出推荐
  设备，可手动覆写；CUDA 执行库可锁定文件夹或一键安装。闲置卸载尚未实现。
  登记表单可从服务宿主浏览选择文件夹，并先经 `service_inspect_permission_judge_package` 完整校验（不登记，
  结果按稳定代码本地化）；只有校验通过才可提交，非官方模型包须显式勾选信任。
  官方下载经 `service_download_permission_judge_model { packageId, format, source }` 在后台运行，同一时间只有一个；
  进度在状态的 `modelDownload`（含所选 `source`，阶段 `manifest`、`downloading`、`verifying`、`installing`，字节数覆盖整个
  模型包）中轮询，完成后给出登记标识；已有同一登记时保留用户命名。快照的 `knownPackages[].variants` 给出
  格式、适用设备、下载大小（未发布为 `null`）、提供该格式的平台 `downloadSources`（未发布为空）、是否推荐与本机已有登记。
- **API 后端**（`api`）：使用 `permission_judge` 消费者配置档的判定路由（用户自己的 TypeSafe 或 OpenRouter
  账号上的 jev）。路由的供应商或其连接所属供应商为 OpenVulcan 账户时直接报错，不退回其他路由。放行
  概率门槛只在 `API_ALLOW_PROBABILITY` 定义，当前值尚未用样本校准；不记录费用。
- **云端后端**（`cloud`）：调用 OpenVulcan 官方部署的判定端点（内部传输仍称 edge），不收费，但判定输入（工具参数文本）离开本机。可用的
  云端模型由运营方在服务端启用，客户端不内置模型或检查点清单：`service_permission_judge_edge_models`
  （无参数，返回 `{ models: [{ modelId }] }`，按 id 排序，可为空；未登录时报错）以已登录账号读取
  `GET {认证源}/api/v1/vulcancode/edge-models`，id 必须是 1–64 个 `[A-Za-z0-9._:-]` 字符且不重复。所选
  模型以完整 id 原样保存在设置的 `edge_model_id`，服务端不再列出时也不改写。服务以已登录账号向
  `POST {认证源}/api/v1/vulcancode/edge-access`（`model_id`、`client_id`）换取
  `{ model_id, site, endpoints: [{ endpoint_id, endpoint, priority, online }], access_token, expires_at }`：
  `site` 是签发部署 id（1–32 个 `[a-z0-9-]` 字符，与令牌 `iss` 相同，客户端不按域名推断），`endpoints`
  是该部署为该模型启用的全部端点（非空，id 规则同模型 id 且不重复，地址只接受 HTTPS 源、回环地址可用
  HTTP），令牌对该模型的任一端点有效；旧的单端点字段不再接受。`404 model_unavailable` 表示当前未启用该
  模型。令牌按模型缓存并绑定账号连接，剩余不足 `ACCESS_REFRESH_AHEAD_MS` 时续期。每个判定请求都带
  `X-OpenVulcan-Site: <site>` 发往当前选中的端点；端点以 `401` 拒绝令牌时续期一次并只重试一次；
  `400 site_required` 与 `403 site_not_accepted` 记为独立的来源拒绝且不重试；端点按调用方（签发部署、
  用户与安装）限速，超限时回答 `429 rate_limited` 与 `Retry-After`，记为独立的限速拒绝（注明等待秒数）
  且不重试；其他拒绝、超时或格式错误同样直接交给下一层，从不放行。请求失败不在请求内改投其他端点，不跟随
  重定向。
- **云端端点选择**：后台任务（`edge_select.rs`）在服务启动时、每 `PROBE_INTERVAL`（10 分钟）、判定设置
  或锁定保存后（`WAKE_DEBOUNCE` 去抖），以及请求遇到网络错误、`5xx` 或 `site_not_accepted` 后（每
  `FAILURE_REPROBE_GAP_MS` 至多一次）探测一轮；只在后端为云端且已登录时探测，从不随单个请求探测。
  每轮依次对每个端点无凭证请求 `GET {endpoint}/v1/status` `PROBE_ATTEMPTS`（3）次，往返取成功次数的
  中位数，全部失败才算探测失败；预计延迟 = 往返 + `compute_ms.p50`（为 `null` 时只取往返）。状态的端点 id
  或模型不符、`sites` 不含本账号 `site`、`ready` 为假或探测失败的端点不参与自动选择。评分最低者胜出，平局
  按 `priority`、id；当前端点仍可用时，只有最佳评分低于其 `EDGE_SWITCH_PERCENT`（80%）才切换。尚未探测或
  无可用端点时使用 `priority` 最小的端点。探测结果只在内存中，重启后重新探测。
  **锁定**：系统级设置 `edge_endpoint_id` 只由用户显式锁定写入
  （`service_lock_permission_judge_edge_endpoint`，`{ request: { endpointId: string | null } }`，`null` 恢复
  自动；锁定时校验该端点在当前部署为所选模型提供的列表中），选择其他云端模型时清除，判定设置保存请求不
  携带它，避免遗漏字段改写锁定。锁定后请求始终发往该端点；该端点不再提供或不可达时直接报错交给下一层，
  绝不回退到其他端点。`service_permission_judge_edge_endpoints`（无参数）返回内存快照，
  `service_probe_permission_judge_edge_endpoints`（无参数）立即探测一轮后返回快照：
  `{ modelId, site, mode: "auto" | "locked", lockedEndpointId, selectedEndpointId, probedAtMs, nextProbeAtMs,
  error, endpoints: [{ endpointId, endpoint, priority, online, region, ready, acceptsSite, rttMs,
  computeP50Ms, scoreMs, error }] }`；`selectedEndpointId` 是请求当前发往的端点（锁定时为锁定端点，未提供
  时为 `null`），`scoreMs` 为 `null` 表示不参与自动选择。设置页每 `pollIntervalMs` 读取快照。
  **请求测试**：`service_test_permission_judge_edge_request`（`{ request: { endpointId, input } }`，`input` 为
  `{ kind: "preset", preset: "ordinary" | "destructive" | "database_reset" | "oversized" }` 或
  `{ kind: "custom", command }`，自定义命令不得为空且不超过 `REQUEST_TEST_MAX_COMMAND_CHARS`）以已保存的云端
  模型、按真实判定的渲染与传输向指定端点依次发送 `REQUEST_TEST_ATTEMPTS`（5）个单条请求，不改变选择与锁定，
  计入账号用量，结果只留在界面。令牌被拒时续期一次并重发该请求；某次请求未得到 HTTP 状态即停止。返回
  `{ modelId, endpointId, endpoint, site, renderedChars, attempts: [{ requestId, totalMs, networkMs, endpointMs,
  nodeMs, queueMs, computeMs, httpStatus, tokenRenewed, answer, errorCode, error }], succeeded, minMs, medianMs,
  maxMs }`：各阶段取自回答的 `Server-Timing`（`edge`、`node`、`queue`、`compute`），网络为总耗时减端点耗时；
  `answer` 原样保留端点回答（区域、模型、检查点、策略版本、每条结果的标签、分数、token 数、是否评分与原因），
  未通过客户端校验时同时给出 `error`；汇总只统计有回答且无错误的请求，中位数取偏低的中间值。
  端点 `POST /v1/judge` 已在节点应用模型包余量规则，标签原样采用；回答必须回显请求标识、检查点摘要为 64 位
  小写十六进制、策略版本等于随构建副本、结果数等于输入数，超过 token 上限的结果视为输入超限交给下一层。
  检查点不与本地清单比对，只记入判定轨迹（`checkpointSha256`）。官方模型包清单 `known-packages.json`
  只服务本机后端，格式版本为 5（下载位置按托管平台列出），不含云端字段，旧版本明确拒绝。
  OpenVulcan 账户页的“模型”分区以“工具裁定模型”类型列出云端模型及本账号的调用：
  `service_openvulcan_edge_usage`（无参数，未登录时报错）读取 `GET {认证源}/api/v1/vulcancode/edge-usage`，
  回答 `{ hours, models: [{ model_id, enabled, requests, items, errors, last_hour? }] }` 按 id 升序，含每个
  启用的模型及时间窗内调用过的已停用模型；合计跨该账号全部安装，由端点上报，没有费用字段。
- **复核 LLM**：`permission_review` 配置档、用途 `tool_call_review` 的对话路由；只收到判定层结论标签，
  不收到原始分数。给用户的理由使用界面语言：宿主在 `<reply_language>` 区段写入已保存的界面语言（未保存时为
  系统语言），以代码与本族语名称表示。
- **复核规则**：系统默认保存在用户全局判定设置的 `review_rules`
  （`service_review_rules_system_default` / `service_save_review_rules_system_default`）；项目可在项目配置
  `permission_judge.review_rules` 保留自己的整套规则，缺省表示跟随系统规则
  （`service_project_review_rules` / `service_save_project_review_rules`，`null` 恢复跟随）。三组均为
  `{ allow, ask, deny }` 文本，保存时去除首尾空白，超出上限拒绝保存。复核时按调用所在工作区取有效规则，
  按拒绝 → 问我（无人值守时为拒绝）→ 放行的顺序应用；放行规则可覆盖“超出项目或改动共享状态即询问”的默认
  指引，但不覆盖项目策略中设为 `confirm` 或 `review` 的类别。保存后下一次复核即生效。
- **复核附加提示词**：系统默认保存在用户全局判定设置的 `review_prompt`（空串不写出；
  `service_review_prompt_system_default` / `service_save_review_prompt_system_default`）；会话可在会话目录
  旁置文件 `review-prompt.json`（`{ version: 1, prompt }`，未知字段或其他版本即报错）保存自有文本，文件缺失表示
  跟随系统默认（`service_session_review_prompt` / `service_save_session_review_prompt`，`prompt: null` 删除
  文件恢复跟随）。子会话按根会话读取。保存时去除首尾空白，超出上限拒绝保存；下一次复核即读取。
- **审批等待**：`approvalTimeoutSeconds` 随判定设置保存（保存请求必须携带，任何保存都不能因遗漏而改变它），
  快照同时发布上限 `maxApprovalTimeoutSeconds`。宿主读取该值传给 `QuestionRuntime::request_approval`；挂起的
  审批以 `deadlineUnixMs` 投影给客户端。到期时经持久回复路径写入拒绝，先到的用户回答优先；拒绝文本
  `APPROVAL_EXPIRED_DENIAL` 告知代理无人及时回答、不要立即重试同一调用。等待适用于全部用户审批（含仅可查看
  与工作区修改模式的审批），无人值守会话不进入用户审批。
- **速度测试**：只用于本机与 API，1 条与 10 条两种，用户触发，结果只留在当前界面；API 测试会产生费用，执行前先确认。云端没有速度测试（`service_benchmark_permission_judge` 直接报错），改为在端点表中对单个端点做请求测试（见云端端点选择）。后台端点探测只请求公开状态，不发送判定输入，与请求测试互不替代。

## 项目策略

- 保存在项目根目录的 `.vulcan/code/config.json` 的 `permission_judge.options`，键为类别、值为档位；
  未列出的类别使用模型包默认档位。判定时按调用所在工作区读取。
- 读取与保存只针对使用中模型的类别；保存时保留模型已不再有的类别的旧选择，但不发送给判定器。
- 设置界面只列出使用中模型的类别，判定器未配置时整个区块不显示。WebUI 位于项目首页与输入区权限菜单的自动审批设置子菜单（见“界面”），并在权限判定
  设置页总览列出各类别与默认档位、经“类别授权”子页面选择项目后编辑；TUI 位于设置区的同名页面并在页首
  选择项目。各类别附带说明其涵盖范围的提示。

## 判定记录

- **开关**：系统级 `recordDecisions`，出厂关闭，只有用户显式开启才写入；保存请求必须携带该字段，任何保存都不能
  因遗漏而改变它。关闭时不写入新记录，已有记录仍可查看、标记、导出与清除。
- **记录范围**：只记录判定模型或复核 LLM 参与的决定；模式闸门与确定性规则直接决定的调用不记录。时间为判定
  开始时刻，记录在最终结论（判定放行、复核放行或拒绝、用户放行或拒绝、等待到期拒绝、失败）产生后写入，写入不阻塞调用。
- **内容**：原样保存工具参数文本（含命令与路径）、工作区、会话、高风险类别、判定标签或错误与耗时、判定输入
  轨迹（后端、模型、策略版本、类别设置、渲染文本、本机或云端的三类分数、API 放行概率、输入令牌数、云端
  报告的检查点摘要）、复核结论与耗时。
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

- 输入区权限模式选择器（WebUI 为输入区工具栏中加号右侧的权限标签按钮，切换到完全权限前须勾选风险确认；TUI `⛨` 标签、`Alt+P`、`/permissions`）列出四种模式；判定器未配置时自动审批显示为灰色并说明原因，点击打开权限判定设置。只有当前为自动审批时才附带无人值守入口；开启后标签显示“无人值守”。
  WebUI 菜单中切换仅可查看、工作区修改与已配置的自动审批时就地生效且菜单保持打开；只有完全权限（关闭菜单并
  弹出风险确认）与未配置的自动审批（关闭菜单并打开判定设置）离开菜单。当前为自动审批时，模式下方出现“自动
  审批设置”子菜单（悬停或方向键向右展开），收纳无人值守入口，以及判定器已配置时会话所属项目的类别授权勾选项
  （勾选即允许）：每次展开子菜单重新读取、每次勾选立即保存，改回类别默认值时恢复跟随默认；只收录“允许或不
  允许”两档的类别，其他形态只在设置页编辑。
  无人值守对话框（TUI 为文本输入框）预填分层默认值并注明来源，确认后才写入；开启中可编辑或关闭。
- 无人值守默认值：WebUI 位于权限判定设置页工具区的“无人值守模式”子页面，TUI 为设置区的同名页面；系统默认与
  项目默认分开编辑，项目可切换“使用项目策略”，关闭时显示所跟随的系统文本。
- 复核规则：WebUI 位于权限判定设置页复核路由区段的“复核规则”子页面，TUI 为设置区的同名页面；系统规则与
  项目规则分开编辑，每组一个文本框；项目可切换“使用项目规则”，开启时以系统规则为起点，关闭时说明跟随系统规则。
- 复核附加提示词：会话文本位于输入区权限菜单“自动审批设置”子菜单的“复核附加提示词”（草稿不可用），对话框
  注明跟随系统默认或本会话自定义，可恢复跟随；系统默认位于“复核规则”子页面末尾的同名区段。TUI 的复核规则页
  末尾同时编辑系统默认与所选会话的提示词，权限选择器在自动审批下提供跳转入口。
- 审批等待：WebUI 为权限判定设置页“审批请求”区段的秒数输入（失去焦点或回车时保存，超出范围恢复已保存值），
  TUI 为权限判定页同名区段的数值行。
- “权限判定”设置页：总览依次为配置状态与后端选择、所选后端的设置（本机为使用中模型、设备、CUDA 摘要与运行
  状态；API 为判定路由，候选不含 OpenVulcan 账户；云端为数据离机说明、账号状态与登录入口、云端模型：登录且显示云端时读取服务端
  清单，以完整 id 列出，读取中、清单为空与读取失败（可重试）分别说明，已保存但未再启用的模型保留并标为不可用）、已配置时的类别授权摘要、复核路由（含“允许 LLM 放行判定器拦截项”）与工具入口；模型管理、官方下载与登记、CUDA
  环境（组件安装、库文件夹、手动下载）、类别授权（选择项目后编辑类别档位）、诊断（模型状态表、速度测试）和判定记录（筛选、标记、导出、清除）是带返回按钮的子页面。状态按服务发布的
  `pollIntervalMs` 重读，唯一定义为服务内核 `STATUS_POLL_INTERVAL_MS`。每个选择立即保存，但后端只经明确
  操作写入：WebUI 点选已就绪的后端立即写入；尚未设置完成的后端（以及未配置时默认显示的本机后端）暂存并
  提示，就绪后由“使用…”（未配置）或“改用…”（已配置）按钮确认；其他选择保存时沿用已保存的后端，不会顺带
  配置判定器。TUI 没有暂存状态：未配置时同时显示本机、云端与 API 设置，
  每个已就绪的后端提供“使用…”或“改用…”按钮，未就绪时说明缺少什么；TUI 不提供登录，云端由服务端在保存时校验登录。
- 初始化引导：第一步为权限判定，四选一（本机、云端、API、暂不启用）；云端需已登录 OpenVulcan 账号并有服务端
  启用的模型（默认取清单第一个，可改选），API 需已保存判定路由；“暂不启用”只关闭引导，不写入。WebUI 在判定未设定时于启动后自动
  打开一次（浏览器本地标记），之后可从设置页再次进入；TUI 没有本地持久偏好，只能从 `/setup` 或权限判定
  页进入，不自动弹出。
- 用户审批复用 Ask 面板，显示命令或路径、权限模式、高风险类别、判定结论与复核理由；设有等待上限时显示
  剩余秒数，归零后只等待服务移除请求。

## 当前实现与验证入口

| 职责 | 入口 |
| --- | --- |
| 权限闸门与管线顺序 | `crates/vulcan-agent-runtime/src/agent_runtime/tool_permission.rs` 中的 `admit_tool_call` |
| 已知只读与高风险命令规则 | `crates/vulcan-agent-runtime/src/agent_runtime/tool_permission/command_rules.rs` |
| 内置 LuaSkills 入口与 `call_luaskill` 分类 | `crates/vulcan-agent-runtime/src/agent_runtime/tool_permission/builtin_luaskills.rs` |
| 模式读取、判定与复核宿主 | `crates/vulcan-service-kernel/src/tool_permission_host.rs` |
| 会话模式修改与草稿首次提交 | `crates/vulcan-service-kernel/src/permission_mode.rs`、`conversation_submission.rs` |
| 无人值守、连续拒绝记忆与代理理由 | `tool_permission.rs` 中的 `PermissionMemory`、`UNATTENDED_REVIEWED_REPEATS`；`tooling/native/declare_intent.rs` |
| 配置类别与变更预览 | `tool_permission.rs` 中的 `admit_configuration_change`；预览由 `crates/vulcan-service-kernel/src/ai_configuration.rs` 计算 |
| 无人值守默认值服务 | `crates/vulcan-service-kernel/src/permission_judge/unattended.rs` |
| 复核规则服务与有效规则 | `crates/vulcan-service-kernel/src/permission_judge/review_rules.rs`；类型与上限在 `crates/vulcan-permission-judge/src/system.rs` |
| 复核附加提示词服务 | `crates/vulcan-service-kernel/src/permission_judge/review_prompt.rs`；会话旁置文件在 `crates/vulcan-workspace-store/src/workspace_store/session_sidecars.rs` |
| 审批等待与到期拒绝 | `crates/vulcan-agent-runtime/src/agent_runtime/question_runtime/approval.rs`、`tool_permission.rs` 中的 `APPROVAL_EXPIRED_DENIAL` |
| 判定设置、项目策略与速度测试服务 | `crates/vulcan-service-kernel/src/permission_judge/services.rs`；API 判定请求与 OpenVulcan 路由拒绝（`reject_openvulcan_route`，亦由 `public_ai_settings/inference_settings.rs` 保存路由时调用）在同目录 `api.rs`；云端模型清单、令牌缓存、请求、来源标记、限速处理与请求测试在同目录 `edge.rs`，后台端点探测、选择与锁定在 `edge_select.rs` |
| 云端端点状态探测形状、评分与切换规则 | `crates/vulcan-permission-judge/src/edge_route.rs` 中的 `EdgeStatus`、`edge_score`、`choose_edge_endpoint`、`EDGE_SWITCH_PERCENT` |
| 判定记录存储、筛选、标记与导出 | `crates/vulcan-service-kernel/src/permission_judge/records.rs` |
| 复核 LLM 请求 | `crates/vulcan-service-kernel/src/permission_judge/review.rs` |
| 本机 worker、设备与 CUDA | `crates/vulcan-service-kernel/src/permission_judge/{worker,environment}.rs` |
| 策略、模型包、官方清单、托管策略副本与 API 门槛、云端回答检查与门槛 | `crates/vulcan-permission-judge/src/{policy,package,known,hosted,edge,decision}.rs` |
| 云端模型清单、账号调用量与访问令牌换取 | `crates/vulcan-provider-adapters/src/client/openvulcan.rs` 中的 `request_openvulcan_edge_models`、`request_openvulcan_edge_usage`、`request_openvulcan_edge_access`，标识规则 `is_openvulcan_edge_model_id`、`is_openvulcan_site` |
| WebUI 界面 | `apps/vulcan-code-web/src/components/features/settings/permission-judge-*`、`components/features/onboarding/`、`components/project/project-permission-judge-section.tsx`、`components/composer/` |
| TUI 界面 | `apps/vulcan-code-tui/src/app/pages/{permission_judge,setup_guide,project_permission_judge,unattended,review_rules}.rs`、`app/permission_mode.rs` |
