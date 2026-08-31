# 04：第一阶段可移植归档拆分审核

## 范围与结论

2026-08-31，完成知识库包文件/ZIP边界的第一阶段归集和拆分后审核。原根10521→9425行，新增package_archive.rs为1039行、其tests.rs为240行，三产物合计10704行。19生产节点、3测试、3夹具及5常量迁移。

这是阶段通过，不是04号完整逐文件终审。其余生命周期、数据库操作、派生索引和检索仍留原根，整体7／15、全局0／5。

## 理解、拆分及保留理由

- 可移植包的普通文件读写、ZIP成员与大小校验、元数据和摘要证明以及导出字节由归档模块拥有，与控制器绑定、注册表提交、两次目录改名和崩溃恢复日志的生命周期不同。
- PreparedKnowledgePackage与三个路径方法整体归位，临时目录负责提取资源生命周期；归档不是新的持久化或控制器权威。
- 15个辅助函数共享同一包格式，不按SQLite/显示库/向量文件再碎分。现行协议允许已声明派生产物，保留v1明确兼容边界及可选文件语义，不能按旧测试描述退回只接受三条目。
- JSON上限同时用于替换日志，临时目录前缀同时用于启动回收，因此暂留原共享基础边界；仅5个归档专属成员常量迁移。内部清单仍由库生命周期持有，归档只是序列化消费者。
- 本阶段不移动export/import/replace的控制器事务。已追踪导出前绑定释放和日志收口、导出后绑定恢复；安装/重定向保留数据库内身份、计数、FTS就绪及派生绑定验证，而不把外层ZIP摘要检查冒充完整SQLite检查。

## 准确性与安全边界审核

1. 精确成员、重复条目、目录/符号链接和路径穿越拒绝原样保留；JSON读取与单项/累计膨胀上限仍在原检查点执行。
2. 版本、名称、Schema、来源身份、ready索引状态、内容大小/SHA-256，以及派生清单与实际文件精确一致性不变；不新增多路径猜测或兼容兜底。
3. 导出仍拒绝残留SQLite日志、非普通文件和超限输入；派生产物遍历有序且不跟随链接/重解析点，最终原子发布及唯一导出名逻辑不变。
4. Prepared类型、5个归档外部入口和3路径方法只开放到runtime父域；仅package/source_manifest两字段供原安装/检查调用读取。临时目录及可选路径字段私有，没有新增crate级生产API。
5. 原71个可见类型/方法未改声明或路径；新models别名仍绑定原knowledge_base::models，以保留原super::models表达式的唯一来源。原21常量均唯一且值/声明不变。
6. 全279核验节点严格词法一致；263生产区域节点包括3个方法内局部类型，局部类型随完整所属方法保留，不作为可独立拆分对象。10项声明及2字段的精确范围变换另行通过比对，没有格式例外或未解析AST节点。
7. 三个产物均由原runtime模块及归档cfg(test)入口接线；迁移生产/测试没有新增位置敏感宏，原控制器集成测试的crate路径环境宏留在原处。

## 测试匹配与自动修正

- 13原测试仍全部可发现：3项归档用例迁移、10项原路径保留；3个原夹具整体归档所有。测试体、正反断言和全部属性严格等价。
- 共享身份不匹配夹具只在cfg(test)下以pub(in super::super)向runtime开放，原真实控制器E2E显式导入并仍能编译，没有复制二进制夹具或删掉集成分支。
- 原真实控制器用例保留ignore原因“requires the bundled output/bin/vldb-controller.exe”，本阶段未手动执行，不计为通过。包级另4项原忽略也未变更。
- 首轮检查识别归档迁移残留的SHA/ZIP/Read及模型常量导入，已清除；没有增加allow或压制业务警告。
- “只接受精确三条目格式”的旧说明按当前源码改为最小三条目测试夹具，合法派生产物规则和原断言未变。新增模块入口补齐双语参数/返回说明。
- 历史Workspace Store测试缺字段问题仅作为线索；当前目标包编译和包级回归实际通过，不沿用旧失败状态。

## 验证结果

| 命令 | 本阶段结果 |
| --- | --- |
| rtk proxy cargo check -p vulcan-workspace-store --all-targets | 通过，最终2.78秒，迁移残留警告已清除 |
| rtk proxy cargo test -p vulcan-workspace-store --lib knowledge_base::runtime:: -- --list | 13项，0遗漏/新增，3路径迁移 |
| rtk proxy cargo test -p vulcan-workspace-store --lib knowledge_base::runtime:: -- --test-threads=1 --quiet | 12通过、0失败、1原有忽略，0.05秒 |
| rtk proxy cargo test -p vulcan-workspace-store --lib -- --test-threads=1 --quiet | 183通过、0失败、5原有忽略，69.69秒；进程正常退出 |
| rtk proxy cargo check -p vulcan-service-kernel --all-targets | 通过，21.71秒，包含运行时消费者检查 |
| rtk proxy cargo clippy -p vulcan-workspace-store --lib -- -D warnings | 通过，15.97秒，包含等待构建锁 |
| rtk proxy cargo test -p vulcan-workspace-store --doc -- --test-threads=1 | 通过，实际0项文档测试，14.57秒 |
| rtk proxy cargo fmt -p vulcan-workspace-store -- --check | 通过 |
| rtk git diff --check | 通过 |

保持Cargo并发4、共享资源测试单线程；所有进程均回收终态。没有执行真实控制器忽略用例、在线模型服务或桌面UI验收。

## 记录与下一步

[原测试基线](04-KNOWLEDGE_TEST_BASELINE.md)、[全量生产/常量/摘要映射](04-PRODUCTION-MOVES.md)、[测试与夹具映射](04-TEST-MOVES.md)及[逐阶段决策](04-vulcan-workspace-store-src-knowledge_base-runtime.md)均已更新。三产物内容摘要复验一致。

剩余原根9425行，不足以完成04号治理；下一阶段沿现有包安装/覆盖与崩溃恢复事务继续核实归属，再处理库/操作/索引/检索职责。仍8个原始文件严格超过5000行，无新增超大产物；当前7／15、全局0／5，计划和总目标继续。
