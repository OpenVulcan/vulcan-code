# 06：插件运行时完整逐文件终审

## 结论与边界

2026-08-31，原9716行文件完成十三阶段职责归集，当前根631行，共26个Rust产物。186生产节点、23原运行时测试及9辅助节点完整保留；没有只靠外置测试达成拆分。06逐文件已闭环，后续仅进入全部已实施范围的循环审核，不扩展其他六个原始目标。

## 为什么拆分，以及为何现在足够

原根混合了宿主桥接、安全授权、磁盘安装事务、Lua租约、目录代次、临时会话和持久运行时等不同生命周期。按实际类型及消费者归属拆分后，各入口仍使用同一个管理器和共享状态，未引入第二事实源。

| 产物 | 保持完整的职责 | 充分性与不再拆分理由 |
| --- | --- | --- |
| runtime.rs | 共享状态、租约身份、构造、引擎附加、Drop、注册表锁 | 24个共享核心节点及私有字段保留；继续拆会扩大状态字段访问或制造循环 |
| actions.rs | 业务Action授权、输入/输出Schema、临时或UI租约选择 | 一次完整调用事务，不能将校验与清理切开 |
| authorization.rs | 内容指纹授权、权限预览、唯一授权路径 | 授权声明与准确校验同域，脱敏信息不混入安装事务 |
| bridge.rs | 固定工具路由、唯一弱注册表、精确作用域与请求信封 | 18固定工具名及分发保留唯一来源，不按单个工具碎切 |
| configuration.rs | 包Schema约束的配置读写 | 复用原原子文件机制，无第二套配置事实 |
| drain.rs | 关闭所有权、失败租约领取/恢复、排空报告和诊断 | 原子领取与失败保留不可分开，未承诺整个排空墙钟有界 |
| environment.rs | 不可变执行环境、依赖声明、环境代次清理 | 保留完整发布与完整性检查，不扩大为依赖系统重构 |
| installation.rs | 安装、更新候选、提交/回滚、卸载 | 完整磁盘事务保持同域，974行已足够内聚 |
| lease.rs | LuaSkills租约创建/加载、执行、解码与关闭 | 相同sid/代次始终绑定，失败清理仍由既有策略执行 |
| lifecycle.rs | 目录刷新、启停过渡、代次守卫、探测与暴露面重建 | 保留注册门内代次更新及唯一快照发布 |
| local_files.rs | 多域共享的包内普通文件和原子JSON读写 | 仅四个真实多方助手，无“通用杂物”或备用路径探测 |
| persistent.rs | Hook/UI类别隔离、初始化门、代次退役 | 两种类别通过原封闭枚举选择，既有状态和门不复制 |
| process.rs | 清单绑定的进程与MCP stdio启动规格 | 1064行但职责完整，不为行数继续割裂平台与声明校验 |
| 各领域tests.rs | 原有直接回归测试 | 随实际消费者同迁，私有事务字段不用开放给测试 |
| tests.rs及tests/bundled.rs、content.rs | 唯一共享夹具、真实内建包、内容型跨层验证 | 跨域用例保持完整，不强迫每个生产文件配凑数测试 |

## 完整性与差异核验

- [完整最终映射](06-FINAL-MOVES.md)覆盖全部218节点；原始基线仍是冻结提交，不覆盖历史源码。
- 209节点精确词法相等；4个参数尾逗号、1个调用尾逗号、1个include_str路径移动和3个有意义旧测试修复逐项留证。保留原始字符串内容，不通用忽略逗号或花括号。
- 所有原前置属性一致；55公开项和原lib.rs再导出路径保留。累计51处内部声明可见性调整对应既有父域调用，生产字段可见性没有变化。
- 22常量与1静态唯一且值一致，空Error实现保留；第四阶段已有9个子产物摘要完全保持。
- 根唯一管理状态及Drop关闭顺序、每类注册表/门、Arc身份比较、失败保留和名额补偿均保持。固定桥接仍来自同一真实源码文件。
- 测试发现及执行保持59包级、23 runtime；无删除、无新增忽略。Context7测试现在真正加载/探测/关闭租约，Lua声明断言阻止内容型提前返回空通过；不调用在线查询。

## 审核发现与修复

1. 原安装拒绝和卸载测试误用已淘汰授权路径，第四阶段已修复；卸载前增加真实文件存在断言。
2. Context7当前没有Hook，旧预热返回Ok(false)不能证明Bundle加载；改用真实启用探测，详见[实名修复记录](06-TEST-REPAIRS.md)。
3. 逐批编译发现的跨域导入与共享超时归属问题已修复并重复编译；没有通过复制常量或删除测试绕过。
4. 终审把根公开入口注释改为完整插件API，并移除排空报告“整体有界”的过强说明。close_gate可能等待已有所有者，只有实际执行阶段使用原超时。
5. 只读核验工具自身的缺失行号输入已改为显式失败；它只提取证据，不把摘要或测试运行自动算作审核轮次。

## 最终实际验证

| 检查 | 结果 |
| --- | --- |
| 插件全部目标检查 | 最终六包联合全部目标检查通过（25.44秒） |
| 插件生产严格Clippy | 最终注释校正后再次通过（1.42秒） |
| 59包级测试 | 本次联合回归再次全部通过，0忽略（4.77秒） |
| 服务层skills_services回归 | 14通过，0失败/忽略（0.02秒） |
| 服务内核全部目标检查 | 第十三阶段通过（18.17秒） |
| 原函数、属性、常量及公开入口核查 | 218节点、23常量/静态、55公开项保全 |
| 文档测试 | 命令成功；当前0项，不伪称有额外覆盖 |
| 全仓格式检查、Git差异检查 | 通过；Git有原LF/CRLF提醒，无差异错误 |

## 执行变更总结

1. 核心调整：完成桥接、生命周期、租约、排空、持久运行时、会话、Action、本地文件及跨层测试归集；生产表达式未改变。
2. 文件清单：原根修改；累计新增25个Rust子产物。第四阶段原9个子产物本次不改。新增最终审核、完整映射、只读核验辅助与机器可读证据；更新现有覆盖和修复记录。没有删除有效测试、资源或其他任务文件。
3. 关键代码：原类型经根重导出，原impl按职责分文件；管理器、状态、配置来源和Drop仍唯一。Context7改为probe，资源宏只调整相对层级。
4. 遗留边界：没有验证实际在线供应商、在线包依赖、真实MCP进程、桌面UI、非Windows平台及Release运行。服务内核原有无关Clippy基线单列，不纳入插件通过结论。全范围五轮审核另计，不能用本终审代替；第三轮发现未修改视频夹具的既有失败后已归零，等待范围确认。

## 当前26产物摘要

| 文件 | 物理行数 | SHA-256 |
| --- | ---: | --- |
| `crates/vulcan-plugin-runtime/src/runtime.rs` | 631 | `1cb910c7309986d8115d03f2ffb053b72575772df488a632040546b12925a110` |
| `crates/vulcan-plugin-runtime/src/runtime/actions.rs` | 240 | `95779a7077c46c5809f520399d10532825ca341b732f6fab3d60d2772aed72e1` |
| `crates/vulcan-plugin-runtime/src/runtime/authorization.rs` | 468 | `9f6ea810e53c6e02663309b18882651ca5d3014e3e62d7b019490f43c8343745` |
| `crates/vulcan-plugin-runtime/src/runtime/authorization/tests.rs` | 70 | `926101b73f8c2dc67e5e8a5e204b4bb0ccbc5b772c76e3373478311716aa38a6` |
| `crates/vulcan-plugin-runtime/src/runtime/bridge.rs` | 802 | `c538c17661fef9c5c2c2ebab6c159ca827b288383260797c42eda34dfa20538d` |
| `crates/vulcan-plugin-runtime/src/runtime/bridge/tests.rs` | 65 | `2f0309fd3ac699c2a830ee54f6899e840d7cbcbfc6144ff5814961c3840db01b` |
| `crates/vulcan-plugin-runtime/src/runtime/configuration.rs` | 178 | `1be8d30af88d104b137fd25713fe981cd69a02476ba9075a4b8ef71da991c5af` |
| `crates/vulcan-plugin-runtime/src/runtime/drain.rs` | 509 | `e9d6b58ebee822f960253dae8a3513853d07f6e33393072a9ccea09fb26ebd9b` |
| `crates/vulcan-plugin-runtime/src/runtime/drain/tests.rs` | 252 | `e3e4db72bc8d58392320e6d4934479dbe1b06cedee4c351ebe466225600b5d91` |
| `crates/vulcan-plugin-runtime/src/runtime/environment.rs` | 766 | `bfdbfcb1b6d731fdcdbca8f81c9b4e6fb514caada5f82c032ab6a2121ca868b9` |
| `crates/vulcan-plugin-runtime/src/runtime/environment/tests.rs` | 76 | `1f6e99817e64c35757dc9c69c46c28cfee45d04af366e3764f84a6bf1454dec4` |
| `crates/vulcan-plugin-runtime/src/runtime/installation.rs` | 974 | `e8aa8c4056464749e85dfb18d0b75ee72f9238ab9dc400e99fcc9ed9a2dbbb91` |
| `crates/vulcan-plugin-runtime/src/runtime/installation/tests.rs` | 355 | `7eef8d052a872abf213c12e2ac647f5f92325d2be87cf784931c12be3fa15ef5` |
| `crates/vulcan-plugin-runtime/src/runtime/lease.rs` | 466 | `87da642ef1e15f83f9b77aeffaabe5f4f311f6e70e4f0d6ffa7e575097d9f5c2` |
| `crates/vulcan-plugin-runtime/src/runtime/lifecycle.rs` | 705 | `8e53423ebd0151b34e03557fe05b4d81cc8dfe93bf5692723941cb1d704b86e0` |
| `crates/vulcan-plugin-runtime/src/runtime/lifecycle/tests.rs` | 115 | `d8adf07c9156bd6da0e575d1a6cdd212bd18005308fbec36c615f594fdc3f36f` |
| `crates/vulcan-plugin-runtime/src/runtime/local_files.rs` | 151 | `84b179724899fe704fc147c77b564519a58389c68374f4eee59181841529e966` |
| `crates/vulcan-plugin-runtime/src/runtime/persistent.rs` | 479 | `e03c26eadc67e079d64b25ebe485d19f4fe31b0d83b1ae63f5b43b57bbdc2768` |
| `crates/vulcan-plugin-runtime/src/runtime/persistent/tests.rs` | 97 | `aaeacce2223fb486945e5f48aeb9958feb3d1209c985ffdaf811c981f9ff6e7a` |
| `crates/vulcan-plugin-runtime/src/runtime/process.rs` | 1064 | `a36815f624d03bb5ef070cc7666a59b2fc802fdd8d877832b9cc80e9a52d952b` |
| `crates/vulcan-plugin-runtime/src/runtime/process/tests.rs` | 33 | `e15c81242686d73a5bab63526171788eaeba015a61b40122781d06014cbf676f` |
| `crates/vulcan-plugin-runtime/src/runtime/sessions.rs` | 829 | `edde69a9f2f5a81d7348cb5e527b7c018499e2f314f56951c90fc640d2cafb57` |
| `crates/vulcan-plugin-runtime/src/runtime/sessions/tests.rs` | 124 | `ac1bfd5edc668589c5b170fed0eacf6373f8ec2c7bc7cbb54ea9cf586b8d56ed` |
| `crates/vulcan-plugin-runtime/src/runtime/tests.rs` | 255 | `40a9624b34e55954ca31934e3068f3fef08a2934e7b5c20eea2a155882ab5f69` |
| `crates/vulcan-plugin-runtime/src/runtime/tests/bundled.rs` | 721 | `57e3949768969dd21c71090d4c585c3b18f0496a8ddf3923bdc446346e06673a` |
| `crates/vulcan-plugin-runtime/src/runtime/tests/content.rs` | 125 | `76676c2f58e52501991951fb6085a2b135299198fe6831083091e10048690308` |
