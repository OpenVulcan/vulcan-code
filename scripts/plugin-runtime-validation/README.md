# 有限完整宿主性能及长稳消费者

该目录落实原计划第 13.3、22.146.65 节的有限消费者及后述真实生产 Action 分支。两个独立消费者通过相同名称的 `vulcan-plugin-runtime` path 依赖，分别消费显式指定的旧主树与新宿主树；旧消费者固定使用已发布注册表 LuaSkills 0.5.9，不应用 LuaSkills 本地补丁；新消费者固定使用显式 UPSTREAM 的本地 LuaSkills 0.6.0 候选。原七种合成负载均调用公开 `VulcanPluginRuntimeManager::invoke`；真实 Action 分支调用同一管理器的公开 `invoke_action`。旧合成组执行完整租约创建、桥分发、shutdown、close；新合成组执行公开 SingleCall／公共复用／专用复用。没有裸核心 submit 对照，没有生产接口改动。

`Cargo.toml` 是包含路径占位符的模板。脚本只在源码树之外的全新运行目录展开两个独立工作区，不改源仓库 Cargo、锁或版本。`prepare` 冻结三棵树的当前修订、完整脏范围、指定构建源码的逐文件 SHA-256，以及驱动、配置的字节哈希；明确包含 Cargo 配置；lance-bitpacking 补丁由模板按提交固定。两组共用 MAIN 已安装的原生 protoc 和 include，逐文件记录该被忽略安装的哈希，并显式传入构建环境。旧 MAIN 是当前脏基线，不是历史正式产品的干净提交。调用方必须在首次测量之前另外冻结机器、绝对时延/容量及原相对门槛；本目录不提供结果后可修改的测量默认值。

```powershell
rtk proxy python -X utf8 execute.py prepare --main-root MAIN --host-root HOST --upstream-root UPSTREAM --run-dir RUN --config CONFIG
rtk proxy python -X utf8 execute.py build --main-root MAIN --host-root HOST --upstream-root UPSTREAM --run-dir RUN --config CONFIG
rtk proxy python -X utf8 execute.py run --main-root MAIN --host-root HOST --upstream-root UPSTREAM --run-dir RUN --config CONFIG
```

`--config` 可重复，配置和顺序在 prepare 前冻结。build 固定 `cargo +1.97.1`，串行构建两份 release 消费者，保留四编译任务和四测试线程，显式指定各自 `RUN/old/target`、`RUN/new/target`；不依赖系统默认工具链或默认 target 目录。生成的清单位于 `RUN/old/Cargo.toml`、`RUN/new/Cargo.toml`；也可由主执行者显式设置 `VALIDATION_BRIDGE_SOURCE` 后使用同一工具链单独执行定向 cargo check。独立消费者从对应冻结宿主锁开始解析，不写源工作区锁；生成的最终锁必须作为实际依赖证据保留，两个消费者依赖版本需要在运行前比对。脚本 build/run 前严格比较冻结源码，变化要求新的运行目录。

可重复的 `--evidence-file` 仅保存显式附加记录的字节哈希，不执行它们；正式测量提供实际机器观察、固定绝对／相对门槛以及资源采样器。所有动作使用同一记录列表，缺失或改变任一已声明记录明确失败。功能冒烟可声明为空，不能因此声称正式测量门槛已冻结或资源已采样。

Windows 正式测量须在三个动作中同时声明 `--windows-sampling-gates GATES`；该精确文件必须也列入 `--evidence-file`。采样选择与配置的有序路径／字节哈希数组共同冻结，反转配置顺序明确拒绝。run 通过原生采样器执行每个原消费者，保存调用、资源、原标准输出／错误四份独立产物；消费者外层时限仅由 execute.py 从冻结负载、调用及清理上限派生并传入，不复制第二时限公式。

采样器在启动前使用 [GetConsoleProcessList](https://learn.microsoft.com/en-us/windows/console/getconsoleprocesslist) 验证自身已经附加控制台，然后让原消费者继承该控制台并明确重定向标准输出／错误；未附加时直接失败，不创建新窗口或探测替代启动方式。此行为遵循 [Windows 默认控制台继承语义](https://learn.microsoft.com/en-us/windows/console/creation-of-a-console)。先前 `CreateNoWindow=true` 在本机产生原消费者的独立隐藏 `conhost.exe`，原非零采样保留为失败记录；当前直接子进程仍全部计数并保存实际身份，零子进程门槛不变，不能过滤操作系统宿主来宣称通过。

编译结束之后同时复核实际复制的三个 Rust 文件及两个展开清单，任一不同就拒绝发布构建冻结。采样错误单独保留，原消费者继续在冻结外层时限内自行排空并关闭；超时明确强制停止并判失败。资源文件本身写入失败时，脚本标准输出仍保留真实原进程退出及观察错误。全部新输出在启动之前相对调用者目录唯一绝对化并核对不同路径，避免子进程改变工作目录后重复拼接相对目录。

每个配置必须完整提供 common.rs 的 Config 字段，不接受隐式测量默认值：

- `group`：`legacy`、`public_single`、`public_reusable`、`dedicated_reusable`。新组完整反序列化 `formal_settings` 为公开 `VulcanPluginRuntimeSettings`，调用 `initialize_formal_runtime`；三新组均要求完整显式 `capacityBudgets.validation-plugin.tool_call`，直接读取公开类型化字段，不调用宿主私有解析器。专用组要求冻结 ToolCall capacity 的真实 `PoolKind::Dedicated`，公共组要求共享容量。禁止临时扩容或改生产常量。
- `workload`：`hot`、`cold`、`burst`、`slow_host`、`compute`、`io`、`mixed`。cold 每次调用之后实际排空该所有者；其余负载按并发工作者同步突发，每个稳定工作者内部顺序提交。mixed 按每个工作者的真实从一开始的业务序号，固定循环 `hot → slow_host → compute → io`，每四次各占一次；每条调用记录包含实际负载。mixed 要求 host_delay_ms、compute_iterations、io_bytes 均为正值。cold 单独用于基准和域退役，不冒充混合长稳负载。
- `concurrency`、`rounds`、`calls_per_worker`、`minimum_duration_ms`、`minimum_calls`、`maximum_duration_ms`：同时满足轮数、时长及**精确成功计数**后停止；最长时长前未满足则记录门禁失败，显式排空和关闭之后退出非零。六档 1/2/4/8/16/64由调用方提供配置，旧 System 租约真实固定上限 8，新容量由公开 settings 决定。高并发错误原文必须保留，公开 invoke 的 Err 没有本驱动可使用的强类型背压分类，不能默认称为真实背压或拒绝。
- `max_active_sessions`、`timeout_ms`、`cleanup_timeout_ms`、`host_delay_ms`、`compute_iterations`、`initialization_iterations`、`io_bytes`、`reuse_idle_ttl_ms`、`reuse_max_uses`、`engine_pool`：冻结实际业务和清单上限，双方共同字段须相同。initialization_iterations 是必需配置，实际注入同源 Lua 模块顶层算术循环，模块导出返回 initialization_total 供宿主校验；每个新原始模块运行一次，真实复用不会重复初始化。初始化昂贵热点和关键轻载必须分别冻结明确次数，不隐式使用同一个默认值。
- `formal_settings`：完整公开设置对象，包含 schemaVersion、parent、pluginBudgets、capacityBudgets、automaticPrewarm、maxCapabilityCallMs、shutdownTimeoutMs。专用组只改变明确冻结的容量策略。旧组记录该对象，但不假称旧桥应用了新父级容量。
- `owner_cycles`、`soak`：正式复用长稳只接受 mixed，要求至少两小时、十万真实成功调用、一千次真实域循环；任何一次 invoke 错误或业务校验错误均使长稳失败，原文写入记录，当前轮收集完成后停止后续负载及新循环，完成实际排空及父级关闭之后报告失败。每周期新 owner 实际调用，要求公开域中存在真实未退役 pool_id；排空之后要求域/调用/会话消失、公开父级与插件资源/retained计数回到稳定基线。summary 的 owner_cycles 仅计实际验证完成次数，configured_owner_cycles 单独保存配置目标。旧组以及新 SingleCall 没有保留复用域，不能声称此项门禁；它们的配置 owner_cycles 必须为零，旧组 soak 必须为假。

slow_host 经现有授权 UI 宿主回调实际等待。compute 在 Lua 请求分发中实际执行算术循环。io 同时执行 JSON 解码、编码/返回和真实原生文件读取：宿主在本验证 root 下预写 io_bytes 个固定字节；Lua 用现有 `vulcan.plugin.ui.invalidate` 的明确 `io` topic 触发同一 callback，callback 每次对唯一固定夹具路径执行 `std::fs::read` 并逐字节比对冻结输入。该路径不来自插件参数，也不假称存在插件文件 API。读取受真实操作系统磁盘缓存影响，不宣称缓存未命中或物理磁盘吞吐；四组使用相同字节和回调。

共同 Lua 自行解码每次原信封，返回实际 tenant、workspace 标记、sequence、算术结果、输入输出内容。模块保留首次 tenant/workspace 并检查以后每次相等，复用回调只检查前次状态已清除并负责清理，不作为业务取信封的前提。FIFO 检查针对稳定工作者的真实顺序提交和业务 sequence，**不宣称跨所有者全局 FIFO，也不证明同一 key 的并发排队顺序**。tenant/workspace 是业务请求标记；公共域按宿主 owner/workspace 分区的事实来自当前实现及域归属查询，业务标记本身不冒充宿主注入工作区身份。

每次真实整调用时间只包围 `manager.invoke`，记录单位为微秒；校验、状态轮询、确认与输出 I/O 在计时之外。JSONL 保存每次实际负载、完整时延、原始结果及错误。错误记为 failed，invoke Err 的 classification 保持 unclassified，不解析错误字符串推断背压；业务校验错误明确为 validation。错误不进入成功百分位或成功吞吐。吞吐分母是整个工作负载墙钟，包含驱动确认和输出成本。每轮记录当前公共状态，周期记录真实 live 域和排空基线。最后新宿主公开 request/poll 验证原正式父级物理 close，旧组验证真实 System 租约列表为空；旧 API 没有正式父级 close，不能假称旧 formal parent 已关闭。清理错误同样保留，不替代真实关闭凭证。

长稳时延内存只保留唯一 SOAK_LATENCY_SAMPLE_LIMIT = 4096 限制的最近已记录成功调用样本，成功计数独立增长，不能使用样本长度满足十万调用。summary 明确 percentile_scope 为 recent_bounded_success_samples、sample_count、sample_limit，且只填写 sampled_p50_us／sampled_p95_us／sampled_p99_us；普通 p50_us／p95_us／p99_us 在长稳中为 null，不冒充全量分位。短基准仍保存全量有限样本并填写普通分位。完整原始时延全部保留在 JSONL，可由主执行者离线计算全量统计；每轮记录缓存不会跨轮累积。域循环的真实调用时延、实际负载及响应也单独保留，但不计入前段负载的十万成功计数、吞吐或分位；cycle_failed 单独记录其调用错误。

每次新提交前核对最长墙钟截止时间，已经启动的真实调用继续完成。整轮满足最小调用数但越过最长时限也明确失败，随后实际排空和关闭，不能因退出循环时最小要求已满足而忽略超时。prepare 将对应冻结宿主的 Cargo.lock 复制到独立消费者目录作为解析起点，生成后锁只保留在运行目录；避免首次独立解析擅自选择源码树尚未使用的新依赖。每个起点锁同时保留不可变的 Cargo.seed.lock 副本。build 首先分别执行完整离线 metadata 以规范化活动锁，再执行 --locked --offline metadata；保留两次完整 JSON、错误输出和精确命令。每个最终依赖的名称、版本、来源、校验和必须来自自身起点锁，只允许新增独立消费者根包，显式记录被裁剪的起点包，任何意外升级、换源或校验和变化直接失败。随后要求唯一 luaskills 包、明确宿主的唯一 vulcan-plugin-runtime 包、正确独立消费者根，以及根包和运行时的直接 luaskills 依赖边都指向同一个 Core 包 ID。旧 Core 必须保留 MAIN 起点锁的 0.5.9 注册表来源及 checksum；新 Core 必须为 UPSTREAM/Cargo.toml 的本地 0.6.0。checksum 仅从真实 Cargo.lock 读取，不假设 metadata 含该字段。两组来源证明全部通过后才执行 --locked --offline 构建，编译前后保持已证明的依赖及 metadata 字节不变，不能把起点锁直接当最终消费者锁。prepare 另存三个实际复制 Rust 文件及两个展开清单的字节哈希，build/run 均先比较这些真实 Cargo 输入；修改运行目录副本也必须失败，不能只比较原模板。

旧宿主没有公开 cleanup diagnostics 查询，观察字段为 null；排空的真实 failures 及原 System 租约物理缺失分别核验，不调用新宿主才提供的方法，也不虚构旧诊断为零。

完整仓库修订及脏范围另外保存于 git-context.json 作为运行前上下文。冻结比较只包含已列出的实际构建输入脏范围及全部实际字节，避免无关并行 Web 编辑或 Python 字节码缓存使不变的原生消费者输入失效。已核验两份消费者锁的十三项本地包，宿主构建脚本仅使用 TARGET 或为空，编译源码没有消费宿主 Git HEAD；无关宿主提交只改变上下文修订，不改变已冻结源码。上游 LuaSkills 修订仍按现有冻结范围严格比较，实际构建来源以完整 metadata、明确清单路径及锁核验为准。构建输入、配置、驱动、原生工具或生成文件变化仍明确拒绝；两组编译之后再次校验输入。两组构建成功之后另存起点副本、实际最终 Cargo.lock、两次完整 metadata、来源证明及两个可执行文件的 dependency-freeze.json，run 开始前严格比较；此前复制的起点锁不代替最终依赖证明。

`phase_timings` 和 `lua_heap_bytes` 固定为 null：现公开接口不暴露精确初始化/阶段计时或 Lua heap；轮询时间不冒充阶段计时。它们及实际既有生产业务延迟仍是完整验收的未完成项。已完成两消费者全目标检查及四组调试功能冒烟，各十六次成功调用，两个复用组各完成两次实际域循环；这些短调试结果不是正式性能结论。100 次实际 Service 更新/回滚已由主任务复用现有服务夹具执行通过，不能由本消费者的域循环替代。

`sample-windows.ps1` 仅负责当前 Windows 验收。它从创建时保留实际原生进程句柄，异步排空终态输出，记录真实工作集、私有字节、线程、句柄和直接子进程，以及原退出码。短寿命进程不能通过缺失退出码变成成功；实际计数缺失也不能替换成零。文件使用 UTF-8 字节顺序标记供 Windows PowerShell 5.1 正确解析双语注释。配置声明的基准／长稳采样间隔来自已冻结门槛；原生计数和较慢子进程查询分别记录观测时间。短调用可能在两个采样之间结束，采样最大值不冒充连续峰值；Lua heap 仍需独立证据。

新增的 `workload: "settings_get_action"` 是有限真实生产 Action 分支。它调用原公开 `manager.invoke_action`，请求固定为插件 `vulcan-usage-statistics`、Action `settings-get`、空参数、`PluginUi` 调用方、实际临时工作区及 `reuse_enabled_runtime: true`。旧消费者的 `group` 仍为 `legacy`，由现有 execute.py 选择旧二进制；新消费者使用 `formal_ui_action`。两者记录均明确标记 `runtime_kind: "UiAction"`，不归入 ToolCall 或 ToolReusable。原七种合成负载不使用此分支，新增字段可省略或为 null；它们不得同时携带生产 Action 输入。

该业务必须显式提供 `settings_get_action` 对象，其 `package_source` 是对应 MAIN 或 HOST 的原插件包绝对目录，`bundle_source` 是双方共同使用的单个原 Bundle 绝对文件。驱动解析原公开清单，从其 `required_files` 连同原 `plugin.yaml`、`dependencies.yaml` 逐字节复制到全新临时包；再将声明的共同 Bundle 原字节复制至实际 `runtime/plugin.bundle.lua`。不转换旧、新清单的 Schema 版本，不序列化重写清单，不搜索候选目录。包路径、Bridge、用户插件目录、插件状态、运行产物、管理运行时和工作区均属于新的临时根；原源码路径只读。真实 `config.get` 读取该私有状态及原默认配置，不注册模拟 getter，不安装 VLDB、网络或桌面服务。

`settings_get_action.validation` 是严格互斥的两个显式阶段：

- `{"mode":"capture"}`：只执行一次未计时的原公开首次 Action，依赖原公开输出 Schema 校验，再检查完整实际 `catalogModels` 的 `modelId` 非空、严格升序。完整结果保存于最终 `summary.captured_result`；热态样本、轮数及成功热态调用计数均为零，持续时间和 P50／P95／P99 为 null。完整排空、关闭和正常进程退出仍是取得可用结果的前提。
- `{"mode":"measure","expected_result":完整实际捕获结果}`：`expected_result` 必须是前一阶段两个真实消费者实际产出的相同完整 JSON 值，不能用合成 Tool 输出、文档示例或人为预期替代。未计时首次 Action 必须先与该值完全相等，然后执行三轮、每轮一千次顺序热态调用。每个完整返回值再次检查实际目录顺序并与该值完全相等；任何原调用或业务校验失败均保留实际结果、停止后续提交，在清理后退出非零。

配置制作顺序固定为：在全新运行目录准备并冻结旧、新 capture 配置及真实输入；独立 release 构建；运行两份 capture；确认两进程退出零、`initialization_completed` 和 `cleanup_confirmed` 为 true、实际 `drain.closed_ui_action_runtimes` 为一、两个完整 `captured_result` 完全相同；保存该实际共同 JSON 作为证据。随后在另一个全新运行目录制作两个带此精确值的 measure 配置，重新准备、构建冻结并运行，不修改既有 capture 或原四十八组的配置／证据。capture 错误直接保留，不能用失败响应推导预期或自动进入 measure。

两个阶段均要求 `concurrency: 1`、`minimum_duration_ms: 0`、`owner_cycles: 0`、`soak: false`。capture 要求 `rounds: 1`、`calls_per_worker: 1`、`minimum_calls: 0`，这三个字段只声明一次初始化，不产生计时轮次；measure 要求 `rounds: 3`、`calls_per_worker: 1000`、`minimum_calls: 3000`。其他 Config 字段仍必须完整声明。`max_active_sessions` 和 `timeout_ms` 必须分别等于原清单 Lua 会话上限和 settings-get 超时，驱动按实际公开类型逐项核对；当前原清单为 8 和 5000 毫秒。清理期限、最长测量墙钟及引擎选项由主执行者在运行前冻结，不使用业务分支默认值。

新配置的完整 `formal_settings` 必须包含精确 `pluginBudgets.vulcan-usage-statistics` 和 `capacityBudgets.vulcan-usage-statistics.ui_action`，使用原公开类型完整反序列化并初始化正式父级。UiAction 容量的实际 Shared／Dedicated 类型和全部上限来自该明确配置，不复用 `validation-plugin.tool_call`，不探测其他用途或修改生产容量。旧消费者记录该对象，但仍只消费其原 System 租约实现。

所有 prepare、build、run 动作的 `--evidence-file` 应包含两份实际原 `plugin.yaml`、`dependencies.yaml`、各自清单列出的全部 `required_files`、单个共同 `bundle_source`；measure 另外包含保存的完整实际 capture JSON、原 capture 结果及事先冻结的机器／门槛／采样记录。共同 Bundle 必须在两配置中指定同一绝对文件，旧、新原 Bundle 的 CRLF 差异不能在运行过程中动态转换。配置本身通过既有 `--config` 冻结；所有动作保持相同证据路径和顺序。该分支不修改 execute.py 或其冻结／进程采样逻辑，输入来源由主执行者明确提供。

热态计时仅包围完整公开 `manager.invoke_action`，包括实际调用入场、原生 `config.get`、原 Lua 业务以及公开输出 Schema 校验；请求构建、独立完整值比较、状态观察及 JSONL 写入在单次计时之外。成功样本保存全部三千次整调用时延并计算 P50／P95／P99；失败不进入成功样本。热态墙钟在初始化后启动、清理前停止。每轮记录公开真实状态。终态始终先调用精确插件的原公开 `drain_plugin_sessions` 并检查实际 failures；旧消费者继而核验真实 System 租约列表为空，新消费者继而通过原父级 request/poll 确认物理关闭并记录实际公开资源／插件状态。排空错误、关闭错误、实际未关闭或观察失败均明确导致非零退出，不能凭删除临时目录宣称资源已释放。Hook prewarm 不用于初始化本 Action。

此分支源码的格式／解析检查不代表消费者已经编译，也不代表上述 capture／measure 或生产性能已通过；实际数量、清理及资源门槛以主执行者后续冻结运行的产物为准。公开接口尚未提供的阶段时延及 Lua heap 仍为 null，不能由本分支的整调用计时替代。

### 可选私有阶段诊断

只有新消费者支持显式环境变量 `VULCAN_PLUGIN_RUNTIME_DIAGNOSTIC_JSONL`。它必须指向父目录已存在、文件尚不存在的绝对路径；相对路径、空路径及已存在文件在开始业务运行前失败，不覆盖旧证据。未设置该变量时沿用原 `common::run`，不创建诊断通道、文件、线程或日志回调。配置字段、原时延门槛、输出格式及 `execute.py` 均不变化。

这一开关仅用于另行冻结的有限诊断，例如并发 1 的 hot／hot_light；正式 48／168 对比及长稳运行必须取消此环境变量。诊断启用时，原整调用时间仍包含同步日志回调的真实开销，不能将其百分位与关闭日志的正式基线直接比较，也不能拿它代替正式性能门禁。

消费者在原管理器初始化前使用既有 Rust `luaskills::set_log_callback` 注册一个容量为 4096 的 `sync_channel`。回调只克隆实际 `event.message` 并执行 `try_send`，不执行文件 I/O、不新增线程、不解析业务内容、不重入 Core，也不捕获 manager／runtime／VM。已核对本消费者的实际初始化路径：`common::engine`、新消费者 `build`、`VulcanPluginRuntimeManager::new` 与 `initialize_formal_runtime` 没有生产代码注册或覆写此日志回调。通道溢出和接收器断开分别保存真实失败事实；不能静默丢弃后宣称诊断完整。

原运行返回后先清除日志注册，再排空有界消息并写入独立 JSONL；只保留能解析且含精确 `luaskills_embedded_diagnostic: 1` 的原消息，不重写其阶段、身份、耗时或堆值。尾记录 `diagnostic_capture` 保存实际接收、选取、丢弃数量、断开状态及原运行是否返回成功。缓冲或文件写入失败使成功业务运行以诊断失败退出；若原业务已经失败，仍尝试保存证据、撤销回调并单独报告诊断错误，最终保留原业务错误。

完整退役投递的依据是原公开关闭链，而非观察到物理 Completed 后等待一段时间：新消费者 `close` 等到 `manager.poll_formal_runtime_closed()` 为真，宿主转调 Core `EmbeddedRuntime::poll_closed`；Core 先等待执行／监督线程退出，再经 `EmbeddedPoolManager::poll_closed` 确认常驻资源为零，并由 `RetirementService::try_shutdown` 检查退役线程 `is_finished`、实际 `join` 后返回成功。因此原 `common::run` 成功返回时，该退役线程的 `retired` 回调已经结束，可以随后撤销日志器，无需额外睡眠或退役 API。

原运行返回错误时不能推断上述关闭链已成功完成，`retired_delivery_complete` 明确为假；已获取的回调快照可以完成进行中投递，部分晚到退役记录可能未进入这一失败运行的文件。成功尾记录且未溢出仅证明原关闭及已发送回调投递完成，不证明每个实例必有记录，不替代真实资源归零门禁。Core 的实例诊断订阅捕获于原分配；先前未订阅的空闲 VM、已撤销或被替换的旧订阅不会自动重新归属新日志回调。

阶段按原身份关联：queue 的 `instance_id` 为 null；真实 allocation／business 给出 operation 与实际 instance 关系；`retired` 仅归属 instance，`operation_id` 为 null。`elapsed_ns` 使用真实单调区间。`vm_creation` 嵌套在 allocation 内，`bundle_compile_and_evaluate` 涵盖编译及实际求值，`*_request_cleanup` 仅记录实际进入的请求作用域重置。`host_wait` 嵌套在初始化或业务时间内，不能相加；Lua 堆值是存活采样而非峰值，销毁后不补零值。真实 Resident 销毁时长包含 Lua GC、VM 销毁及容量释放，不能称为纯 GC 耗时。原 summary 的 `phase_timings`／`lua_heap_bytes` 继续为 null；独立文件是私有诊断证据。

来源拒绝测试使用 `rtk proxy python -X utf8 -m unittest discover -s . -p test_consumer_source.py -v`，验证错误本地基线、重复 Core、非归属清单、缺失或重定向依赖边、起点锁身份漂移及错误消费者根。该测试不执行 Cargo、编译或性能测量；真实消费者来源仍须由 build 中两次实际 metadata 证明。
