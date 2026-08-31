# 02：第十一至第十二阶段选择与内部推理拆分审核

## 范围与检查点

- 日期：2026-08-31。审核模型选择/精确租约规则与内部推理设置，不是02号最终逐文件审核或全局零问题轮次。
- 根文件6383→5232→4251行，本轮减少2132行；累计23个Rust产物合计17255行。新增model_selection.rs为815行、其tests.rs为395行，inference_settings.rs为843行、其tests.rs为215行。
- 本轮迁移27个原生产区域节点、4测试及一个原用途常量；累计193生产区域节点、31测试与1夹具归位。根仍73生产节点和16跨层测试，仍有媒体工具、快照等职责，低于5000行不等于结构拆分已充分。
- 原314节点、71类型、47测试、2常量保全；全部定位与摘要见[生产映射](02-PRODUCTION-MOVES.md)、[测试映射](02-TEST-MOVES.md)。逐文件仍6／15，全局0／5。

## 第十一阶段：模型选择与精确会话租约

### 理解与拆分理由

精确选择、账号偏好、全局同模型账号重绑、目录观测与显式租约撤销围绕同一路由身份，但拥有不同的状态修改权。原来分散在选择RPC与底部助手中，容易把全局替代误当会话自动换模。归入model_selection.rs，维持独立命名函数和既有发布策略，而非合并这些不同语义。

迁移请求类型、选择RPC、待生效规划和十个选择/观测/撤销助手，共13原生产区域节点。三个直接回归随模块归位；其中两个原助手只在cfg(test)编译，继续保留原条件和跨文件测试入口。

### 关键审核

1. 选择RPC保留精确账号归属、待验证隔离、模型启用、账号目录存在性及压缩锁定节点严格小于目标模型窗口的校验，失败仍发生在持久化之前。
2. conversationId存在时先取得真实V1句柄；配置事务内始终记录精确账号偏好，但仅无会话作用域时才替换全局active。
3. 会话选择规划只修改旧内存镜像的pending；当前活动三元组与冻结租约不变。原幂等判断优先pending，再比较active，不生成旧可靠审计记录。
4. 可失败提交仍调用set_pending_model_request_v1，由V1 metadata持久化完整下一请求路由。已追踪该函数的MetadataOnly提交，保留思考/温度控制与初始化前fallback规则，未修改独立持久化所有者。
5. 全局账号失效先尝试同一供应商/模型的首选账号，再按稳定顺序寻找启用模型；没有全局选择时保持不存在，不制造新默认选择。
6. 普通目录协调返回未改写工作区和空变更集合；显式禁用才对精确账号/模型或供应商/模型撤销活动及归档租约，不重写用户自有三元组。
7. 六个真实跨领域助手仅向设置父域开放；原请求/RPC保持crate范围，原pub(crate)测试协调入口保持同名、同条件的原根重导出。
8. 三项直接测试仍覆盖同模型重绑、pending幂等/冻结租约保留、目录观测与活动/归档精确撤销。提交链重复选择回归继续经原根入口调用，运行通过。

## 第十二阶段：内部推理候选与消费者配置档

### 理解与拆分理由

内部推理候选、严格诊断、内部启用/操作覆盖、消费者精确路由和知识库独立profile共享内部调度身份，与交互会话发布、纯能力校验和真实连接测试不同。归入inference_settings.rs，保留既有inference_validation与inference_testing职责，不新建重复运行时。

迁移5类型、3RPC、6候选/诊断/知识库函数，共14节点，以及固定知识库用途常量；唯一直接候选绑定回归随模块迁移。

### 关键审核

1. 待验证凭据不生成候选；供应商内部默认只解析一次。内部启用与会话发布开关保持独立，不把关闭交互会话模型误当关闭知识库/VMM使用。
2. LLM消费模型冻结协议绑定；Embedding/Rerank只消费已注册Action绑定，不从凭据或模型名称猜协议。InternalBackground与ExternalService用途范围原样保留。
3. 操作覆盖校验全部匹配候选；路由保存不能擅自修改模型内部开关。LLM必须提供用途标识，Embedding/Rerank拒绝用途标识，槽位操作必须精确匹配。
4. 诊断使用与执行一致的严格解析器，保留consumer/purpose/operation/route及失败原因；槽位操作不一致优先拒绝，不静默替换无效配置。
5. 知识库按不可变库标识维护独立profile，不覆盖其他库或VMM；用途常量和原三个知识库入口由原根同名重导出，保存/删除事务与发布补偿顺序不变。
6. 内部候选四字段route/catalog_generation/enabled/capabilities原根pub(super)实际为crate范围，迁移后声明pub(crate)保留原范围；其他六字段私有。诊断及请求字段也保持私有。
7. 11个原可见类型/服务/知识库入口保留路径；候选和诊断构建仅向父域开放，追加诊断助手私有，原too_many_arguments属性不变。
8. 207行原候选回归的所有断言保留：冻结协议、两种用途、非对话模型排除、内部开关独立，以及后半段路线投影的协议/来源/规则/修订/可执行性溯源。通过测试专用导入使用原投影所有者，没有拆掉其跨层不变量。

## 文档问题的自动修正

- 关闭上一轮已记录问题：reconcile_disabled_connection_sessions不再描述不存在的fallback参数，明确其只撤销精确活动/归档租约。
- 删除待生效规划函数前误挂的旧“追加审计记录”说明；实际函数没有显示名称参数，也不返回非空工作区持久计划，现按V1提交与空协调计划准确描述。
- 补齐选择/回退助手的runtime及参数/返回说明；明确内部模型开关独立于会话发布，而非排除内部LLM；知识库路由说明与当前注册表来源一致。
- 以上均为双语说明修正，函数体、错误字面量、派生/序列化属性与测试断言不变。

## 接口、条件编译与保全证据

1. 23文件重新AST扫描：314原节点唯一，无新增业务节点、遗漏、重复或无法解析节点；71原类型仍完整。
2. 312节点严格词法一致，仍仅两个既有publication泛型声明尾逗号差异按实名参数序列精确接受。本批没有增加格式例外，47测试体全部严格一致。
3. 独立保留可见性的比对核准111处声明、22字段变换。其中18字段仍限制在原设置域，新增4字段保留原crate范围；未扩大私有状态。
4. 原127可见顶层节点完整，80项通过原根重导出、47项仍定义在根。知识库常量另行精确核查声明、值、唯一归属与原根路径：根pub(super)→子模块pub(crate)保持crate范围。
5. 原两个cfg(test)选择助手保留属性；reconcile_missing_session_model_selections的原根重导出同样带cfg(test)，生产构建不引用配置关闭的项。
6. 测试实际发现47项：31迁移、16原路径保留；唯一推理夹具仍与原消费者归位。没有删除、复制、忽略或弱化测试。
7. 前十阶段18个子产物摘要未变。23产物无位置敏感include/file/line/column/module_path/env宏；未修改Schema、资源、Index、外部协议或持久化引擎。
8. 清理迁移残留导入；仅为保持原类型路径使用八组有明确原因的expect，其余实现与导入没有广泛屏蔽警告。

## 验证命令与结果

| 命令 | 结果 |
| --- | --- |
| rtk proxy cargo check -p vulcan-service-kernel --all-targets | 第十一阶段3.69秒、第十二阶段3.78秒，均通过且迁移导入警告已清理 |
| rtk proxy cargo test -p vulcan-service-kernel --lib public_ai_settings:: -- --list | 两阶段均精确47项，累计31路径迁移 |
| rtk proxy cargo test -p vulcan-service-kernel --lib public_ai_settings:: -- --test-threads=1 --quiet | 两阶段各47通过、0失败、0忽略，分别1.36秒/1.33秒 |
| rtk proxy cargo test -p vulcan-service-kernel --lib repeated_submission_selection_preserves_lease_until_target_changes -- --test-threads=1 | 提交链租约消费者1通过，0.11秒 |
| rtk proxy cargo test -p vulcan-service-kernel --lib -- --test-threads=1 --quiet | 788通过、0失败、0忽略，145.05秒；一次通过并确认进程正常退出 |
| rtk proxy cargo test -p vulcan-service-kernel --lib session_request_config_v1:: -- --test-threads=1 --quiet | V1请求消费者9通过，0.46秒 |
| rtk proxy cargo test -p vulcan-service-kernel --lib knowledge_base_services:: -- --test-threads=1 --quiet | 知识库消费者14通过，0.00秒 |
| rtk proxy cargo check -p vulcan-code-tauri --all-targets | 通过，8.38秒 |
| rtk proxy cargo clippy -p vulcan-service-kernel --lib -- -D warnings | 未通过：仍仅workbench_refresh.rs:400既有collapsible_if；Git对象8a61c080ce2818904c2340bf614e4a287ee2d324与冻结基线一致 |
| rtk proxy cargo test -p vulcan-service-kernel --doc -- --test-threads=1 | 通过，实际0项文档测试，34.98秒 |
| rtk proxy cargo fmt -p vulcan-service-kernel -- --check | 通过 |
| rtk git diff --check | 通过 |

保持Cargo默认并发4及测试单线程；各消费者定向回归与全包运行串行，不并发争用共享资源。24项消费者定向复验包含在包级测试范围内，不重复计为新增测试。历史控制器连接拒绝记录继续保留，本轮未复现，不宣称已修复独立控制器生命周期。未触发真实带凭据在线推理、授权或桌面交互验收。

## 后续充分性与复扫

全仓相同忽略规则复扫仍有8个原始文件严格超过5000行，无新增超大产物。02根4251行只说明已越过行数阈值，仍有73生产节点、16跨层测试和媒体工具/快照职责待归集；02及全部23产物仍必须完成逐文件终审。未完成原始目标仍9个，逐文件保持6／15，全局0／5。

当前根摘要399037274674286f39071abff042978a7c7babd331347bb6d3f128e884ddbd3e；四个新产物及全部23产物摘要见生产映射。已记录的会话注释问题本轮关闭，后续继续按实际职责处理剩余代码，不因行数下降提前完成目标。
