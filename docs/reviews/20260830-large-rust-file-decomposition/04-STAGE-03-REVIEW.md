# 04：第二至第三阶段生命周期与操作协调审核

## 范围与结论

2026-08-31，完成包安装/覆盖/崩溃恢复，以及操作预留/取消/持久进度/终态两个职责域的归集和阶段审核。根9425→8119→7334行；本轮新增package_lifecycle.rs为1412行、其测试70行，operations.rs为924行、其测试25行。累计7个Rust产物、11044行。

本轮迁移57生产节点、3测试与4常量；累计76生产区域节点、6测试、3夹具和9常量归位。原279核验节点（含3方法内局部类型）、13测试、3夹具、21顶层常量完整。04尚未整体完成，当前7／15、全局0／5。

## 第二阶段：包生命周期

- 包文件归档与控制器事务是两个独立边界。本模块将16个运行时方法、日志/守卫两个类型及4个守卫方法、三个相关校验/计数助手归在一起，不拆断完整失败补偿路径。
- 暂存库先复制并通过SQLite内身份、计数及FTS校验，prepared日志在第一次改名前持久化；备份改名、暂存切换、身份重定向、注册记录提交和committed日志顺序原样保留。
- 未达committed时失败仍恢复精确旧目录与旧注册记录；无法恢复时保留日志。达committed后备份/日志清理失败仅进入成功结果的警告，不把已经提交的覆盖误报成未提交。
- prepared恢复按实际备份存在性执行回滚或验证原目录；committed恢复先核查目录、索引和权威计数再清理。日志与文件名、目标UUID、操作UUID、备份名、旧注册身份及阶段相互校验，不猜测候选路径。
- 目录回收保留所有生命周期的已注册库，只接受规范孤立UUID目录与保留解压前缀；仍通过原普通目录/重解析点及宿主子路径删除守卫。
- 原四个公开包方法保留原KnowledgeBaseRuntime关联接口；两个启动恢复入口只向runtime父域开放。日志类型/五字段与两个产物方法为原跨领域E2E共享，守卫及其字段和其他助手私有。
- 两项直接回归随本模块迁移；归档包准备、内容校验和二进制夹具继续由package_archive持有，不重复实现。

## 第三阶段：共享操作协调

- 活动身份、预备执行身份、持久任务行三个类型，26个运行时方法及3个纯函数同属操作生命周期，共32生产节点。排队/取消错误语义由此模块持有，不把导入、索引和外部派生任务拆成多份登记。
- 预留在持久插入await之前写入同一active_operations映射，失败按精确library/job释放；每库唯一任务和过期任务保护不变。所有包装继续共享ControllerRuntimeHandle持有的原锁/缓存，没有新锁或复制状态。
- Prepared任务五字段供原导入/索引工作器读取；Active身份字段和任务行字段仍私有。仅实际跨领域进度/终态/预留方法以及排队/取消纯助手开放到runtime。
- 外部派生入口仍只接受DisplayBuild/VectorBuild，成功返回running及curating/embedding阶段。排队快照仍只供分离导入/索引入口返回，不能按旧注释把外部入口退回queued。
- 成功、失败与用户取消的持久终态保持区分；外部终态写入成功后才释放预留。running进度约束、首次started_at、计数、告警有界性与可观测JSON序列化保持原语义。
- 根统一启动编排和各领域实际工作器暂留原所有者；操作模块不接管Markdown内容、FTS执行或外部模型推理。过期取消用例随共享身份逻辑迁移，原断言不变。

## 逐节点、接口与测试保全

- 全279核验节点均严格词法一致，0遗漏、0重复、0新增业务节点、0属性差异。34顶层类型及3个方法内局部类型保留；局部类型随完整所属方法审核，不单独迁走。
- 71原可见类型/方法声明不变，16个公开方法仅移动其固有实现所在模块，关联类型与调用路径不变。30节点声明、12指定字段和4共享常量的范围变化独立逐token验证，均不超出原runtime域。
- 21顶层常量值完整且唯一；取消错误声明被rustfmt折行，读取完整声明后与原值严格等价，不引入通用格式或表达式忽略例外。
- 7个源码产物、6条文件模块边全部从原根可达，无孤儿文件；唯一环境宏是原根E2E的CARGO_MANIFEST_DIR，原路径不动。此前四个子产物SHA-256不变。
- 13原测试继续全部可发现：6随职责迁移、7保留原路径。3夹具唯一，测试体、全部正反断言、ignore及tokio属性严格一致。没有删除、复制、弱化或新跳过用例。

## 自动修正与验证边界

- 清理迁移后的归档/模型结果/File及可观测类型/Ordering等残留导入，没有添加业务警告压制。
- 修正外部派生入口“queued”说明为真实running快照；修正新包装共享控制器状态而非全新缓存的说明，以及原子落盘目标也包括恢复日志/归档的参数说明。代码表达式、错误与SQL未改。
- 真实控制器原忽略用例已完整读取并单独执行。它使用独立临时目录、随机本地端口和测试注册，完成后shutdown；本次仅在子进程清空可选性能语料环境变量，不读取未请求的生产语料。
- 该用例验证真实控制器上的Markdown/Jieba BM25、独立包装绑定复用、FTS修复/重建与持久进度、包内SQLite身份、覆盖后项目绑定保留、prepared回滚和committed清理、多来源增量同步。
- 崩溃恢复由构造持久产物模拟，不是实际强杀进程；外部在线模型的显示/向量推理和UI交互未验收。保留原ignore声明方便无bundled控制器环境，并将显式执行结果单独记录。
- 本轮发现的是可见性接线与说明一致性问题，修正后复验通过。服务内核此前独立Clippy基线不在本轮目标crate严格检查范围，不把本轮Workspace Store通过扩称整个工作区Clippy通过。

## 实际验证

| 命令/范围 | 结果 |
| --- | --- |
| 目标包all-targets检查 | 第二阶段2.62秒、第三阶段1.76秒，均通过 |
| 两阶段knowledge_base::runtime::测试发现 | 均精确13项，路径分别迁移5项/6项 |
| 两阶段knowledge_base::runtime::单线程定向回归 | 各12通过、0失败、1原有忽略，各0.04秒 |
| 原真实控制器E2E，--ignored --exact --test-threads=1 | 第二阶段前10.93秒、第二阶段后11.18秒、第三阶段后10.74秒；每次1通过、0失败 |
| rtk proxy cargo test -p vulcan-workspace-store --lib -- --test-threads=1 --quiet | 183通过、0失败、5原有忽略，73.71秒，进程正常退出 |
| rtk proxy cargo check -p vulcan-service-kernel --all-targets | 通过，25.70秒 |
| rtk proxy cargo test -p vulcan-service-kernel --lib knowledge_base_services:: -- --test-threads=1 --quiet | 14通过、0失败/忽略；测试0.00秒，编译约1分29秒 |
| rtk proxy cargo clippy -p vulcan-workspace-store --lib -- -D warnings | 通过，19.89秒，包含等待构建锁 |
| rtk proxy cargo test -p vulcan-workspace-store --doc -- --test-threads=1 | 通过，实际0项；含构建等待约1分25秒 |
| rtk proxy cargo fmt -p vulcan-workspace-store -- --check | 通过 |
| rtk git diff --check | 通过 |

控制器精确用例命令为：rtk proxy cargo test -p vulcan-workspace-store --lib knowledge_base::runtime::tests::real_controller_markdown_bm25_end_to_end -- --ignored --exact --test-threads=1 --quiet。实际由独立PowerShell子进程清空VULCAN_KNOWLEDGE_PERF_SOURCE后执行。保留Cargo默认并发4；共享资源测试逐一单线程执行，所有进程已回收终态。

## 充分性与记录

包事务与操作协调已分别形成完整边界，不按单个校验器或进度字段继续碎分。原根仍7334行、187个生产区域核验节点与7个跨领域测试，库注册/授权、控制器绑定、导入/FTS、原始与派生索引/检索等需要继续归集，不能将本批通过当作04号完整终审。

[完整决策](04-vulcan-workspace-store-src-knowledge_base-runtime.md)、[全量生产/常量/摘要映射](04-PRODUCTION-MOVES.md)、[测试与夹具映射](04-TEST-MOVES.md)及[冻结测试基线](04-KNOWLEDGE_TEST_BASELINE.md)共同留证。全部已闭环文件及当前产物仍须纳入最终五轮完整审核；当前7／15、全局0／5，整体计划和目标继续。
