# 02：第一至第三阶段拆分审核

## 审核状态

- 审核日期：2026-08-31。仅为02号阶段审核，不是逐文件终审或全范围零问题轮次。
- 原文件16244行，当前根14925行；迁出三个生产职责域及三个匹配测试文件，共7个Rust产物、16396物理行。
- 累计迁出26个生产节点、7项测试与唯一1个夹具；根仍有240个生产节点及40项跨层测试。
- 全部314原始节点、71类型、47测试与2常量完整；没有为了达标删除测试。主记录、生产映射、测试映射和基线均已落盘。
- 整体仍6／15逐文件闭环，全局0／5未开始；02号保持进行中。

## 职责与充分性审核

| 职责域 | 实际内容 | 边界结论 |
| --- | --- | --- |
| inference_validation | 内部LLM/Embedding/Rerank覆盖、精确模型参数、限值、思考档位及VMM限制 | 8个纯校验函数内聚；仅3个真实父域复用入口开放，不单独碎分每个函数 |
| media_candidates | 候选类型、媒体资源映射、模型与已注册协议交集、当前目录候选稳定投影 | 1类型+4函数构成只读候选域；不加入开关写入或伪造协议能力 |
| inference_testing | 设置测试请求/结果、真实供应商执行、持久用途槽位、宿主权限预算、固定请求和响应校验 | 2类型+11函数保留从请求到校验结果的完整链；共享服务句柄访问仍供定制模型验证使用 |

当前三域均有明确类型/函数归属，但设置根仍混合定制供应商/模型、OAuth、连接秘密、同步补偿、工作台发布、会话选择和总快照等职责。当前拆分显然不足，不能因迁走7项测试或行数有所下降就结束02号。后续优先核查OAuth与凭据生命周期、保持事务补偿顺序，再处理其他设置域和剩余跨层测试。

## 结构、行为与接口核验

1. 用当前CodeKit AST重新扫描全部7个文件：314节点可解析且唯一，无遗漏、重复或新增生产节点。
2. 对266生产节点、47测试和1夹具逐token比对：全部严格等价，无业务表达式或断言变化。比较器只剔除注释、空白、可选末尾逗号及单独审核的可见性，不使用通用“格式兼容”例外。
3. 两个原始常量唯一且内容相同；迁移前节点的派生、序列化、测试和条件属性均保全。
4. 127个原crate可见顶层项完整，迁出的4项保留原根明确重导出。子模块pub(crate)与原根pub(super)实际同域；未对外新增API。
5. 对保留可见性的词法流再做第二次比对，仅允许明确记录的11处节点声明和7处媒体候选字段调整，其余314节点无额外差异。
6. 七个媒体候选字段仍只向原设置域开放，保持跨层选择及测试构造；两个推理测试合同的十字段仍私有，序列化形式不变。
7. 两个RPC合同类型的兼容性重导出因分发层使用类型推导而未直接被引用，仅对此明确设置带原因的expect(unused_imports)；没有广泛压制未使用导入、测试或生产警告。
8. 共享服务句柄助手的调用者已追踪至设置测试与定制模型验证。迁移后仍先取得Arc再释放状态锁，没有跨await持锁或复制运行时所有权。
9. 未改变普通发布、工具专用发布、显式会话撤销三种策略，持久前镜像、运行时激活、准备RPC结果、发布及失败补偿顺序均保留。
10. 新模块不含依赖源文件位置的include、file、line或module_path宏，不含猜测性的相对路径补偿；三个测试文件均通过各生产模块的cfg(test)接线。

## 测试匹配与保全

- 基线47项实际通过，本检查点再次发现47项；7项搬迁和40项原路径构成精确一一映射。
- inference_validation保留2项直接回归；media_candidates保留3项；inference_testing保留2项和唯一共享路由夹具。
- 剩余40项保留原跨层归属，覆盖连接、供应商、会话设置、投影及取消补偿，未为拆分将它们机械归入纯校验层。
- 本检查点定向执行47通过、0失败、0忽略，1.40秒。路径与断言完整列表见[测试映射](02-TEST-MOVES.md)。
- 不以测试名包含test认定无效代码：inference_testing是设置页真实生产连接测试能力，完整保留。
- 未触发真实账号的在线推理测试、OAuth授权或桌面交互验收；现有回归通过不等于这些运行边界已经人工验收。

## 自动修正与原有问题

- 清理两阶段迁移后遗留的根导入；StructuredOutputCapability只有原跨层测试使用，归入真实测试模块导入，不扩大生产依赖。
- 补全纯校验函数的中英文参数/返回说明，保持函数体；将误挂在媒体资源映射前的内部推理候选说明归还实际函数。
- 保留RPC类型原路径，以仅针对兼容性重导出的明确预期解决未使用导入提示，其他导入警告已实际清理。
- 严格Clippy发现未修改workbench_refresh.rs:400的collapsible_if。该文件Git对象与冻结基线均为8a61c080ce2818904c2340bf614e4a287ee2d324；未越界修改，不伪称严格静态检查全绿。
- 包级首轮787通过、1失败：capability_metadata_reopen_and_rebuild_rpc_preserves_conversation在测试夹具连接回环控制器时被拒绝，发生在被测设置RPC前。测试和夹具均与基线相同，源码验证用例仍有意义，因此保留。随后单项原样重跑1通过，未增加重试代码或弱化断言。完整复跑与环境边界如下记录。

## 验证命令与结果

| 命令 | 结果 |
| --- | --- |
| rtk proxy cargo check -p vulcan-service-kernel --all-targets | 通过，最终3.32秒，无源码警告 |
| rtk proxy cargo test -p vulcan-service-kernel --lib public_ai_settings:: -- --list | 精确47项，7项路径迁移，0基准 |
| rtk proxy cargo test -p vulcan-service-kernel --lib public_ai_settings:: -- --test-threads=1 | 47通过、0失败、0忽略 |
| rtk proxy cargo test -p vulcan-service-kernel --lib -- --test-threads=1 | 首轮787通过、1失败、0忽略，141.89秒；保留控制器连接失败事实 |
| rtk proxy cargo test -p vulcan-service-kernel --lib tests::capability_metadata_tests::capability_metadata_reopen_and_rebuild_rpc_preserves_conversation -- --exact --test-threads=1 | 单项1通过，6.63秒 |
| rtk proxy cargo test -p vulcan-service-kernel --lib -- --test-threads=1 --quiet | 完整复跑788通过、0失败、0忽略，142.89秒；首轮连接失败仍保留记录 |
| rtk proxy cargo clippy -p vulcan-service-kernel --lib -- -D warnings | 未通过：仅未修改workbench_refresh.rs的既有collapsible_if |
| rtk proxy cargo clippy -p vulcan-service-kernel --lib | 退出成功，仍有上述1项既有警告，不等于严格模式通过 |
| rtk proxy cargo test -p vulcan-service-kernel --doc -- --test-threads=1 | 通过，实际0项文档测试 |
| rtk proxy cargo check -p vulcan-code-tauri --all-targets | 实际桌面消费者通过，38.38秒 |
| rtk proxy cargo fmt -p vulcan-service-kernel -- --check | 通过 |
| rtk git diff --check | 通过 |

保持Cargo编译并发4；所有本阶段Rust回归使用单线程。Windows链接器库/对象创建提示仍存在，不将其记录成测试失败。未运行无约束工作区全量测试，也未进行真实凭据在线验收。

## 可复核产物及下一步

- [主决策记录](02-vulcan-service-kernel-src-public_ai_settings.md)、[原始测试基线](02-SETTINGS_TEST_BASELINE.md)、[266生产节点及全部产物摘要](02-PRODUCTION-MOVES.md)、[47测试与夹具映射](02-TEST-MOVES.md)。
- 当前根SHA-256：c22d0172ff54bc16c800818afdeca0594e1498fb937d076bb72e0074f57b3a64；另六个产物摘要见生产映射。
- 全仓按忽略规则复扫，超过5000行的仍是原名单剩余9个，未产生新的超大文件；其余8个未开始的目标行数未变。
- 完整包级复跑已通过，但未修改控制器生命周期，也不能据此保证首轮回环连接故障永不复现。本检查点不是全局零问题轮次，不能抵扣最后连续五轮审核；继续02号下一职责域。
