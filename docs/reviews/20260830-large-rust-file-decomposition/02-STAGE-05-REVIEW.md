# 02：第四至第五阶段拆分审核

## 范围与状态

- 日期：2026-08-31。本轮为OAuth和连接凭据生命周期的阶段审核，不是02号最终闭环或全局零问题轮次。
- 进入本轮时根14925行，OAuth归集后14515行，连接生命周期归集后12793行；本轮根减少2132行。
- 当前10个Rust产物合计16666物理行，包含新增模块接线、明确导入和参数/返回说明。累计迁出59个生产节点、8测试、1夹具；原314节点仍完整。
- [前三阶段审核](02-STAGE-03-REVIEW.md)继续保留历史验证事实；当前全量映射见[生产映射](02-PRODUCTION-MOVES.md)与[测试映射](02-TEST-MOVES.md)。
- 02号仍进行中，整体逐文件6／15，全局0／5尚未开始。

## 第四阶段：OAuth完整生命周期

### 理解与为何拆分

OAuth由两个请求类型、启动/查询/取消/提交四个RPC和唯一重新授权秘密回滚组成，属于具有进程内事务身份的授权生命周期；不是普通静态密钥编辑，也不是供应商网络原语的重写。

当前源码确认启动选择精确provider/service/profile，读取Manifest声明并生成完整授权绑定。提交要求事务已成功，并根据当前声明重新计算绑定后才取得材料。新连接复用唯一凭据连接提交入口；已有连接重新授权保留秘密修订、目录验证、配置保存、完整响应投影和失败补偿。

### 拆分与保留边界

- 新增oauth.rs，482行，归集7个原生产节点。两个类型与四个RPC通过原根明确重导出，实际crate可见范围不变；十个请求字段仍私有。
- 新增与重新授权两个分支完整保留；已有秘密按期望修订替换，缺失秘密按明确可选状态创建；补偿继续推进修订或删除本次补建秘密。
- 不把状态查询、取消及短助手分别切碎；不搬入或复制底层OAuth协议、浏览器回调监听、Token刷新实现。
- 原连接请求协议拒绝测试同时覆盖OAuth/新增/更新，留在原跨入口测试层，不人为拆成三份或删除OAuth断言。
- 补齐RPC参数/返回双语说明；提交函数原说明只提新连接，已纠正为包含重新授权，不改变实际代码路径。

### 阶段复核

OAuth完成时，目标包全部目标检查通过；314节点、47测试、2常量词法严格相同。单独运行三种连接请求拒绝selectedProtocol的原用例1通过。六个迁移crate可见项原路径保留；清理OAuth残留导入，未触发真实账号授权。

## 第五阶段：连接与凭据生命周期

### 理解与为何拆分

连接新增、更新、默认账号、移除、同步、用量查询共享一个精确连接身份与配置事务边界。它们的四个请求类型、精确注册表查找、主秘密与管理秘密补偿、配置引用清理应由同一领域持有。

新增继续覆盖目录发现与保存的同一事务；Conversation与ProviderTools按声明入口类型分别验证目录或用量。更新根据真实身份变化决定是否重新验证；管理秘密使用Unchanged/Created/Replaced/Removed四态及完整前镜像。移除清理精确账号引用并显式处理默认账号替换；单连接同步只修正未来会话的全局账号选择，不擅自改变现有会话租约。

### 拆分与保留边界

- 新增connections.rs，1786行，归集26个生产节点，包括四个请求类型、六个RPC、管理秘密枚举与三个方法，以及相关验证、补偿和清理助手。
- 类型与impl一起迁移，原根不留空impl；没有复制管理密钥变更类型或以布尔组合替代其闭合四态。
- add_public_ai_connection_with_secret与apply_connection_update向父域开放供OAuth复用；remove_connection_from_configuration向父域开放供定制供应商删除复用；其余私有助手继续私有。
- 更新请求九字段在子模块改为pub(super)，保持原设置域可见范围，供OAuth构造；其他连接请求字段保持私有。四个类型与六个RPC保留原根crate可见路径。
- 原精确账号清理回归归入connections/tests.rs，134行；覆盖两种媒体后端、媒体选项及原生扩展集合，仅清除指定账号的引用，不清空无关账号。
- 原三种请求协议拒绝回归仍留跨入口测试；没有为减少根行数移动所有测试。
- 将误挂在validate_connection_credential前的“更新连接”说明归还service_update_public_ai_connection；补全缺失的参数/返回说明，函数体及断言不变。

### 是否已经足够

OAuth482行与连接1786行各自包含可完整阅读的生命周期，暂不再拆成每个秘密操作一个文件。02根仍有207个生产节点和39项跨层测试，定制供应商/模型、会话选择、媒体/工具设置、事务发布和快照投影尚未归集，因此整体拆分明显不足，仍须继续。

## 全量结构与测试保全审核

1. 当前10个Rust产物重新进行AST扫描：314原节点唯一且全部可解析，无遗漏、重复、新增生产节点或空遗留impl。
2. 对266生产节点、47测试及1夹具逐token核验：全部严格等价；未改变字面量、错误、逻辑分支、参数、返回类型或测试断言。两常量均保全。
3. 单独保留可见性进行第二次比对，仅允许明确的30处节点声明和16处字段可见性调整；无额外差异。16字段分别为媒体候选7字段和连接更新请求9字段。
4. 127个原crate可见顶层项均存在；20项已通过根重导出，其余仍定义在根。原pub(super)与子模块pub(crate)均为crate范围，没有新增对外接口。
5. 原派生和序列化属性完全相同；RPC泛型入口依赖Serialize/Deserialize而不使用类型路径猜测，搬迁不改变JSON协议。
6. 三处类型重导出分组带明确unused_imports预期，仅保护原路径的契约保留，不广泛压制代码警告。其他迁移遗留导入已清理。
7. 47测试全部可发现，累计8项迁移、39项原路径保留；测试属性和忽略状态不变，唯一夹具仍保全。此轮未删除或弱化任何测试。
8. 新模块不包含include、file、line、column、module_path或env位置敏感宏，也无猜测相对路径适配。
9. 事务取得/释放位置、秘密修订比较、补偿顺序、目录与用量清理、发布前响应构建均与基线一致。普通、工具专用、显式会话撤销三种发布策略没有合并。
10. 本阶段不修改系统供应商资源、索引、Schema、底层OAuth原语或输出产物，不触发真实凭据在线验收。

## 实际验证

| 命令 | 结果 |
| --- | --- |
| rtk proxy cargo check -p vulcan-service-kernel --all-targets | 第四阶段4.79秒、第五阶段4.34秒，均通过且清理了迁移导入警告 |
| rtk proxy cargo test -p vulcan-service-kernel --lib public_ai_settings:: -- --list | 第四、第五阶段均精确发现47项，当前8项路径迁移 |
| rtk proxy cargo test -p vulcan-service-kernel --lib public_ai_settings::inference_settings_route_tests::connection_requests_reject_user_selected_protocol -- --exact --test-threads=1 | OAuth阶段单项1通过 |
| rtk proxy cargo test -p vulcan-service-kernel --lib public_ai_settings:: -- --test-threads=1 --quiet | 当前47通过、0失败、0忽略，1.33秒 |
| rtk proxy cargo test -p vulcan-service-kernel --lib -- --test-threads=1 --quiet | 788通过、0失败、0忽略，143.10秒；本轮未复现上轮控制器连接拒绝 |
| rtk proxy cargo clippy -p vulcan-service-kernel --lib -- -D warnings | 未通过：仍仅workbench_refresh.rs:400的既有collapsible_if；当前Git对象仍匹配冻结基线 |
| rtk proxy cargo check -p vulcan-code-tauri --all-targets | 通过，10.50秒 |
| rtk proxy cargo test -p vulcan-service-kernel --doc -- --test-threads=1 | 通过，实际0项文档测试，36.47秒 |
| rtk proxy cargo fmt -p vulcan-service-kernel -- --check | 通过 |
| rtk git diff --check | 通过 |

使用默认Cargo编译并发4及测试单线程。上一检查点的控制器连接拒绝与严格Clippy既有警告继续保留在[基线问题记录](BASELINE_ISSUES.md)；不删测、跳过或用新重试代码遮蔽。

## 当前内容摘要与后续

当前根SHA-256为258660b1fb5bb96c5b02a4d10b6d87a65a59addebac812c90a1187d2e2bdb915；OAuth为6f125025bfa6b1d00f0b9387d086411e1dbc52a1ca62617d3c33b17bd19abda5；连接生命周期为e7633100400e27ca9e7cfa360dbe197f8e2c44f57946b194b88688027daa81ab；完整10文件摘要见生产映射。

完成本批验证后继续归集02的其余领域，不能将部分职责完成改成整个目标完成。当前仍6／15、全局0／5，计划与目标保持进行中。

## 本轮阶段审核结论

- OAuth及连接生命周期的结构迁移和测试对应关系已完成本轮审核；314节点与47测试仍完整，未发现迁移引入的语义差异。
- 本轮788项包级回归一次通过，未修改任何测试断言或控制器夹具；上一轮连接拒绝记录继续保留，不宣称环境波动已永久解决。
- 全仓复扫仍只有原名单剩余9个超大文件，没有新增超大产物；其余8个未开始目标行数不变。
- 严格Clippy仍存在已证实未改动的基线警告，不能写成严格静态检查通过；未越界修改该文件。
