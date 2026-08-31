# 02：第六至第七阶段拆分审核

## 范围与检查点

- 日期：2026-08-31。本轮只审核公共AI设置层的配置发布、补偿与供应商同步生命周期，不是02号逐文件终审或全局零问题轮次。
- 根文件由12793→11781→10774行，本轮减少2019行。当前14个Rust产物合计16783行，包含模块接线、明确导入与补充说明。
- 累计95个生产节点、12项测试和1个夹具按职责迁移；根仍有171个生产节点及35项跨层测试，整体拆分仍不足。
- 原始314节点、71类型、47测试、2常量继续保全；全量当前映射及摘要见[生产映射](02-PRODUCTION-MOVES.md)和[测试映射](02-TEST-MOVES.md)。
- 当前逐文件仍6／15，全局0／5，02号不提前闭环。

## 第六阶段：配置发布与精确补偿

### 理解与拆分理由

配置变更、持久/运行时前镜像补偿、工作台候选构建、会话租约排除及最终提交构成同一发布边界。它们在设置根中散布，容易隐藏“响应完成前不能发布”和“工具保存不能协调无关模型”的顺序约束。

新增publication.rs，共950行，迁移19个生产节点，包含八个配置激活/补偿助手、节点预算失效助手、工作台协调类型与三个构造方法、六条工作台发布入口。预算失效直接回归随之进入publication/tests.rs，112行。

### 关键审核

- 普通发布继续执行目录模型协调；工具专用发布明确关闭该协调与无关租约缺失检查；显式撤销只处理原精确所有者集合，三条策略未合并。
- changed_session_ids、revoked_session_ids、invalidated_context_budget_session_ids保持三个不同集合，不把持久变更、租约撤销及预算失效混成一个含糊状态。
- 候选工作区、有效账号、模型行、附件能力、预算与工作台均在实时状态外完成预备；最后可失败提交及租约检查结束后，才替换Service状态、递增修订和提交租约移除。
- 持久配置与运行时前镜像保持配对；补偿沿原比较交换路径，主失败与两个独立补偿错误不互相覆盖。
- 协调类型与方法整体迁移。仅workspace_only构造及两个原跨层测试读取字段向设置父域开放；预算失效字段和内部构造仍私有。
- publish_public_ai_workbench保持原pub(crate)路径重导出。仅实际父域复用助手使用pub(super)，其他助手私有。
- 原唯一节点预算测试的空模块外壳随函数迁移移除；测试函数本身保留，通过新模块cfg(test)接线。活动与归档锁定预算失效、未锁定状态及组装指纹不变的断言全部保留。
- combine_results补齐参数/返回双语说明；清理迁移残留导入，不以压制警告代替清理。

## 第七阶段：供应商同步发布与取消补偿

### 理解与拆分理由

供应商清单设置入口负责调用注入的同步服务，并协调暂存代际、运行时目录和工作台发布。它不拥有清单下载、验签、文件持久化引擎，不应与原独立provider_manifest_sync.rs混成另一份实现。

新增provider_synchronization.rs，共885行，迁移17个生产节点，包括五个RPC、封闭失败类型、暂存取消守卫及new/disarm/Drop、前镜像校验、精确补偿和报告收尾。三项直接回归进入同域tests.rs，189行。

### 关键审核

- 同一批同步仍只抓取一次签名Index，但逐供应商完成激活/补偿后才处理下一项；已经提交的无关供应商不被后续失败回滚。
- ProviderProjectionPublicationFailure仍区分Compensated与Uncertain；前者允许形成逐供应商拒绝报告，后者必须中止批次，未转为成功或统一吞错。
- StagedProviderActivationCancellationGuard与借用生命周期、new/disarm/Drop一起迁移；从前镜像完整性确认后到最后异步激活等待期间保持armed，解除位置完全不变。
- 取消析构继续尝试精确恢复磁盘与暂存所有权，失败时保留Applying日志并失败关闭；未用新重试或忽略错误替代恢复契约。
- 状态读取持有激活锁再取得配置锁，确保镜像一致；预览在两锁内捕获基线，随后释放锁再执行远端预览。
- 五个RPC通过原根pub(super)重导出保持实际crate范围；后台监控与RPC分发继续使用同一路径。内部类型、字段、守卫及助手保持私有。
- 三项测试分别验证补偿后的有界拒绝、缺失前镜像失败关闭、取消时恢复磁盘和暂存代际，不将取消/失败用例改成成功路径。
- 原全量同步说明误挂在目录激活助手前，已归还实际service_sync_provider_manifests；补齐部分参数/返回说明，业务表达式未改。

## 行为、可见性与测试保全证据

1. 全部14文件重新做AST扫描，314原节点唯一且可解析，无遗漏、重复、新增业务节点或空遗留impl。
2. 312节点严格词法相同；另外两项仅因pub(super)使rustfmt把泛型声明换行，各加入一个合法尾逗号。没有函数体、参数类型、字面量或断言变化。
3. 两项格式差异按精确函数名及原泛型序列验证，只允许该声明大于号前增加一个逗号；不得把规则扩展为忽略任意逗号。
4. 保留可见性的第二次词法比对仅允许49处声明及18处明确字段调整，并限定同两项格式差异。其余可见性与字段形状相同。
5. 原127个crate可见顶层项均保留，26个迁出项明确重导出，101个仍定义于根；未扩为外部API。
6. 两个常量保持唯一且相同，所有派生、序列化、测试属性保留。47项测试函数体全部严格等价，无删除、复制、忽略或断言弱化。
7. 测试实际发现47项，累计12项迁移、35项保留原路径；唯一夹具仍由原两项消费者使用。
8. 新产物没有include、file、line、column、module_path或env位置敏感宏，也没有候选路径兜底；取消测试继续经既有资源解析器读取当前可信清单。
9. 未修改供应商资源、Index、Schema、同步端点、信任配置、持久服务实现、OAuth协议或运行产物；未触发真实凭据在线授权、在线推理及桌面交互验收。

## 两项精确格式差异

| 函数 | 原token→当前token | 唯一差异 |
| --- | ---: | --- |
| mutate_public_ai_configuration_and_publish_in_transaction | 129→130 | Mutation, PrepareOutput, Output泛型声明末尾增加逗号 |
| publish_public_ai_workbench_with_session_reconciliation_and_commit | 132→133 | ReconcileSessions, Commit泛型声明末尾增加逗号 |

## 验证命令与结果

| 命令 | 结果 |
| --- | --- |
| rtk proxy cargo check -p vulcan-service-kernel --all-targets | 第六阶段4.17秒、第七阶段4.20秒，均通过并清理迁移导入警告 |
| rtk proxy cargo test -p vulcan-service-kernel --lib public_ai_settings:: -- --list | 两阶段均精确47项；当前12项路径迁移 |
| rtk proxy cargo test -p vulcan-service-kernel --lib public_ai_settings::publication::tests:: -- --test-threads=1 | 第六阶段直接回归1通过 |
| rtk proxy cargo test -p vulcan-service-kernel --lib public_ai_settings:: -- --test-threads=1 --quiet | 当前47通过、0失败、0忽略，1.34秒 |
| rtk proxy cargo test -p vulcan-service-kernel --lib -- --test-threads=1 --quiet | 788通过、0失败、0忽略，144.15秒；本轮一次通过 |
| rtk proxy cargo clippy -p vulcan-service-kernel --lib -- -D warnings | 未通过：仍仅workbench_refresh.rs:400的既有collapsible_if；Git对象8a61c080ce2818904c2340bf614e4a287ee2d324仍与基线一致 |
| rtk proxy cargo check -p vulcan-code-tauri --all-targets | 通过，10.66秒 |
| rtk proxy cargo test -p vulcan-service-kernel --doc -- --test-threads=1 | 通过，实际0项文档测试，42.04秒 |
| rtk proxy cargo fmt -p vulcan-service-kernel -- --check | 通过 |
| rtk git diff --check | 通过 |

保持Cargo默认并发4及测试单线程。严格Clippy既有问题不伪称通过、不越界修复。早先控制器连接拒绝记录继续保留；本轮若复现按真实结果补记，不删测或加重试代码掩盖。

## 内容摘要与后续充分性

当前根SHA-256为f3399cc62e6374b6b9a0dce6e70af44c1c66c84f5ca0c06d1326cce76e1e65f3；publication为79271646ba0d38603f9bdae7c2a5aba6716ba28de3e88b950c3f2b8c8a20c325；provider_synchronization为0ff9b6f6a68016b1feda835ad6010ce92aa70e08ffaa39d4561d35db30f27ccc。完整14产物摘要见生产映射。

全仓复扫仍为原名单剩余9个超大文件，没有新增超大产物。两个新模块各自持有完整事务生命周期，暂不拆成单步回调文件；设置根仍10774行，后续继续按定制供应商/模型、会话选择、媒体与工具设置、快照投影归集。02号不闭环，整体6／15、全局0／5，目标保持进行中。
