# 06：执行环境与进程规格两阶段审核

## 当前结论

2026-08-31，原runtime.rs 9716行已归集两段完整职责：环境生命周期与进程/MCP启动规格。当前根8039行，5个Rust产物合计9978行；32生产节点、2直接测试和1环境常量迁移。当前两批审核通过，14项服务消费者回归也已通过；但06仍有154生产节点及21跨域测试/9辅助节点待继续，尚未充分，不能逐文件闭环。

全部原218节点、22常量/1静态注册表、55原可见项、23测试及9辅助节点完整保留。累计9处父域范围调整均实名核对，无字段变化；业务体、错误字面量、断言和原测试条件保持。

## 当前全部产物覆盖

| 产物 | 物理行 | 审核与充分性 |
| --- | ---: | --- |
| runtime.rs | 8039 | 保留共享管理状态、原桥接注册表与尚未归集的安装/授权/租约/排空等；本文件仍需继续 |
| runtime/environment.rs | 766 | 环境身份、创建/复用/发布、依赖安装及候选/过期清理同域；当前边界充分，不拆断流程 |
| runtime/environment/tests.rs | 76 | 原授权后篡改拒绝用例，唯一共享夹具，匹配环境发布边界 |
| runtime/process.rs | 1064 | 清单驱动的原生/Node/Python规格、MCP stdio和npm入口；只解析规格，不接管Lua租约 |
| runtime/process/tests.rs | 33 | 原Windows路径规格用例，保持cfg与全部断言 |

所有路径相对`crates/vulcan-plugin-runtime/src/`。完整节点、声明、行号及5个产物摘要见[生产映射](06-PRODUCTION-MOVES.md)；全部测试与辅助节点见[测试映射](06-TEST-MOVES.md)。

## 第一阶段：环境所有权审核

- 标记类型、2公开关联入口和11其他函数，共14生产节点归入environment；Windows Release无窗口常量与函数内部cfg一起移动。
- 新建环境安全复制后重新哈希，匹配原已授权指纹才安装明确Registry依赖、写标记、发布；失败清理原暂存目录。已有环境按完整身份标记匹配，不在记录中冒充每次重哈希全目录。
- Bundled源目录、用户私有代次和content-only准备空操作保留原明示条件。Node/Python依赖使用精确受管分发与固定包管理器，不猜测PATH或兼容版本。
- 更新回滚和提交分别调用精确候选删除与过时代次清理；目录边界、链接/重解析拒绝和空父目录删除原样保留。共同文件复制与JSON落盘助手仍位于根，由安装和环境共享，不复制实现。
- 五生产函数仅向父域开放；标记和字段、包管理器、依赖命令与策略键保持私有。直接测试迁到环境模块，共用test_manager和write_user_plugin_package仅增加测试父域范围。
- 第一阶段根8994行；218节点严格等价，7处范围变化、无字段变化。23原运行时测试完整发现并通过，4.16秒；目标包全部目标检查通过，1.52秒。

## 第二阶段：进程与MCP规格审核

- 公开LaunchSpec、2个公开管理器入口和15个其他函数，共18生产节点归入process；公开LaunchSpec在runtime显式重导出，lib原通配导出不变。
- 宿主回调仍由原注册表与manager身份分发；进程解析核对精确租约作用域与生命周期状态、声明能力、唯一逻辑入口、exec/session模式及各自选项名单。未改变权限或信任边界。
- 原生程序的工作区cwd覆盖只有声明授权后才生效，并限制在规范化可信workspace；Node/Python脚本和cwd继续从包声明解析。用户env仅按原生二进制允许名单处理，宿主固定环境键不能被覆盖。
- MCP enabled入口与候选描述符入口保留不同前提；候选入口核对代次而不要求已启用，非stdio显式拒绝。三种launcher按声明分支直接解析，不探测替代协议。
- npm bin解析保留包名语法、私有node_modules内路径、安装清单身份和精确bin键校验；字符串bin仅允许声明定义的默认键。返回Node加具体脚本，不执行包管理器shim。
- 两个需要根hook二进制读取和桥接分发调用的函数仅pub(super)，其余专属助手私有。共享文件/JSON/桥接工作区助手仍留根，不引入第二所有者。
- Windows规格测试整体迁移；test且Windows才接线此测试文件，函数原cfg(windows)仍在。测试输入与两项program/cwd断言保持。
- 第二阶段根8039行，前一阶段环境两产物SHA完全不变；23运行时测试完整发现并通过，4.15秒。最终59包级通过、0失败/0忽略，4.47秒。

## 保全、条件编译与接线

- 冻结摘要为c442f9145589f8e966babe875d7041b3c4e12d4fc6e7a60525901ddd2433b562，当前根摘要为c1975d35ce3a483636856dd24bee94c6f30d8cbaff6d574f94fab950fd354bcd。
- 全部218节点唯一映射；23类型与195函数/方法原样保留。按排除注释/空白、有限格式尾逗号及实名作用域变换的词法核验，218严格等价，0缺失/新增/重复，无额外表达式格式例外。
- 7生产助手和2测试夹具的9范围变化精确核验；所有原字段保持，无外部API扩大或缩减，55原可见项声明保持。
- 22常量与1static均核对完整多行声明和值；只有Windows创建标志随环境移动，进程级OnceLock管理器注册表保持唯一原根位置。
- 5文件经4条新增模块边完整接线；根仍原cfg(test)内联测试，环境为cfg(test)子测试，进程测试为cfg(all(test, windows))并保留函数自身Windows条件。
- 空Error实现仍唯一存在；原include_str桥接Lua相对路径及3个CARGO_MANIFEST_DIR宏仍在原runtime.rs，未因文件移动改变路径语义。
- catalog、manifest、host_services、atomic_file、lib、目标Cargo.toml、根Cargo.toml与Cargo.lock相对冻结提交无差异；不更改协议、清单、依赖或资源产物。

## 发现、自动修复与复验

| 发现 | 修复 | 复验 |
| --- | --- | --- |
| 移动环境后Command与解码导入滞留原根 | 仅移除无消费者的导入，子模块显式导入 | 无警告全部目标检查与严格生产Clippy通过 |
| 移动进程后启动器/受管运行时导入滞留原根 | 归入真实所有者，保留根仍使用的类型 | 默认全部目标与消费者编译通过 |
| 旧依赖命令说明承诺有界执行，但实际同步output无本地超时 | 修正双语注释，明确同步和无本地超时，不改调用 | 业务token严格保持 |
| 环境入口说明混淆新建内容复核、已有标记匹配及纯内容包空操作 | 按实际三种路径修正说明 | 原环境与content-only回归通过 |
| 旧MCP入口说明把所有启动器描述为受管运行时 | 明确也可为声明的原生二进制 | 三分支原实现及公共签名保持 |
| 审核脚本初次复用保留旧模块名前缀，误报告发现数0 | 修正为实际runtime前缀，并与真实test --list逐项核对 | 23原测试与当前23项一一匹配，缺失/新增均0 |

没有修改测试断言或通过忽略绕过问题。审核脚本的误报单独留痕，不被当作测试真的消失。当前两批迁移问题均已修复并复验，不宣称其余原文件已审核完毕。

## 实际验证

| 命令 | 结果 |
| --- | --- |
| rtk proxy cargo check -p vulcan-plugin-runtime --all-targets | 最终通过，1.48秒 |
| rtk proxy cargo test -p vulcan-plugin-runtime --lib -- --list | Windows发现59项，runtime23项，所有原测试匹配 |
| rtk proxy cargo test -p vulcan-plugin-runtime --lib runtime:: -- --test-threads=1 --quiet | 第一阶段23通过/4.16秒；第二阶段23通过/4.15秒 |
| rtk proxy cargo test -p vulcan-plugin-runtime --lib -- --test-threads=1 --quiet | 最终59通过、0失败/0忽略，4.47秒 |
| rtk proxy cargo clippy -p vulcan-plugin-runtime --lib -- -D warnings | 通过，2.19秒 |
| rtk proxy cargo check -p vulcan-service-kernel --all-targets | 通过，19.74秒，包含相关agent-runtime依赖 |
| rtk proxy cargo test -p vulcan-service-kernel --lib skills_services:: -- --test-threads=1 --quiet | 14通过、0失败/0忽略，0.01秒；正常结束 |
| rtk proxy cargo test -p vulcan-plugin-runtime --doc | 实际0项，编译通过/3.71秒 |
| rtk cargo fmt --all -- --check / rtk git diff --check | 通过 |
| 218节点、22常量/1静态、55原可见项、23测试/9辅助节点及接线核验 | 严格等价且完整，属性保持 |

资源相关测试保持单线程和仓库编译并发4。MSVC创建导入库的linker_messages输出不影响编译/测试退出；生产严格Clippy独立通过。

服务层14项属于skills_services完整过滤范围，其中5项是插件技能/Agent伪造修改拒绝、MCP初始秘密精确输入和生命周期状态/回滚错误，另外9项是该服务域已有通用回归。它们不是实际安装、MCP进程启动或在线依赖安装验收；具体入口的结构保全还由218节点核验和消费者编译共同支持。

## 未运行边界及下一步

未运行Linux/macOS、Release运行、真实在线pnpm/uv注册表依赖安装及完整桌面/WebUI验收；现有隔离安装、Lua和bundled插件回归不替代这些验收。新旧同一cfg分支及源码token保全支持结构迁移结论，不夸大运行覆盖。

同口径复扫仍7个原始超5000行文件，没有新增超大产物。06原根仍8039行，需要继续配置/授权、安装更新、宿主桥接、生命周期/排空、租约和会话边界。整体8／15、全局0／5；本阶段不是06终审，更不是全局审核轮次。

## 执行变更总结

1. 核心修复与调整概述：按不可变环境与清单进程规格两个真实生命周期归集32生产节点，并迁移2直接测试；保留原共享管理状态和公共入口。
2. 📂文件变更清单：新增runtime/environment.rs、environment/tests.rs、process.rs、process/tests.rs；修改原runtime.rs；新增06测试基线、生产/测试映射及本阶段审核，更新06独立决策、README、FILES及实体计划。无源码/有效测试删除，无依赖或资源变化。
3. 💻关键代码调整详情：9处父域范围调整均有实际调用依据，公开LaunchSpec保留根重导出；Windows配置、路径宏和进程静态注册表完整；只清理迁移残留导入并修正过时说明，不改业务或断言。
4. ⚠️遗留问题与注意事项：当前阶段验证以本表终态为准，未运行边界如上；06仍未充分，其余原目标继续，保持整体目标活动，不提前提交推送或归档。
