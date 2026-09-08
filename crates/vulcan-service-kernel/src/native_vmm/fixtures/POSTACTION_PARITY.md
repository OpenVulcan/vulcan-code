# 外置业务载荷与历史预算对照

## PreCheck 混合历史

- 生成入口：`precheck_oracle_test.go`，在同一只读覆盖层中映射为外置 `internal/app/usecase/native_precheck_oracle_test.go`，同时保留 PostAction 映射以共享测试预算类型。
- 通过外置真实清洗、业务序列化、历史预算及上下文方法生成 `precheck_history.json`：四个混合状态上下文、十个正数预算边界。存储行交付使用外置既有测试桩，不代表外置物理存储验收。
- 在外置仓库执行 `rtk proxy go test -overlay <覆盖层文件> -p 4 -parallel 1 -count 1 ./internal/app/usecase -run '^TestNativeParityPrecheckOracle$' -v`，环境变量 `VULCAN_PRECHECK_ORACLE_OUTPUT` 指定输出。独立再生成使用私有临时文件，不覆盖编译输入。
- 原生入口：`real_controller_precheck_history_policy_preserves_mixed_turns` 经真实客户端入库、后台队列、公开策略保存与实际 L1 捕获，前两条完成提炼、后两条保留待提炼；完整正文字节及历史身份均比较。仅将样本来源序号映射为实际接纳的 Turn ID，不归一掉转义差异。
- 首轮真实对照失败：两个 RAW_TURN 正文不一致，预算 244、289、334 各多选一条历史。共享业务 JSON 转义修复后，完整目标 1 项通过（17.86 秒）；DETAILS、字面反斜杠、原始持久回合、待分析队列、调用方隔离与准入冻结均保留。
- 2026-09-07 独立再生成 1 项通过（外置包 0.040 秒），两份 SHA256 均为 `824729a3c3f10f762d42063d505b826bc935da7a8135b7ebff6da30d780592b0`。这不是全部 PreCheck 阶段、完整宿主启动或真实模型质量验收。

## PostAction 业务载荷

- 基线：外置仓库 `D:/openvulcan/VulcanMemoryMesh`，提交 `82f05fa5e3c7657e770fefd84e220fba5654ce07`。
- 生成入口：同目录 `postaction_oracle_test.go`，通过 Go 的只读覆盖层作为外置 `internal/app/usecase/native_parity_oracle_test.go` 编译。直接调用外置真实包中的 `buildPostActionTurnPayload`、`estimatePostActionTextBudget`、`trimHistoryTurnsByBudget`，没有复制或改写算法。目标优先分支按已核对的 `buildTurnAnalysisInputWithResolvedTime` 执行。
- 覆盖层把上述虚拟路径映射到本目录测试文件；生成命令在外置仓库执行：`rtk proxy go test -overlay <覆盖层文件> -p 4 -parallel 1 -count 1 ./internal/app/usecase -run '^TestNativeParityPostActionOracle$' -v`。环境变量 `VULCAN_PARITY_ORACLE_OUTPUT` 指定本目录 `postaction_payload.json` 的绝对路径。
- 本轮覆盖层文件：`C:/Users/lingeasy/AppData/Local/Temp/vmm-postaction-parity-20260907-01/overlay.json`。覆盖层配置是测试工具参数，不是外置源码修改或应用运行依赖。
- 生成结果：九条真实外置载荷、六十三个历史选择结果。包含中英文、中间追问与应答、缺失最终回答、HTML 运算符、Unicode 行／段分隔符、字面反斜杠 Unicode 转义、JSON／控制字符及较长混合正文。三条参考详情按时间排序，预算覆盖禁用、目标不足、目标恰好耗尽、仅余一个单位、最新详情边界与全部详情边界。
- 样本输入属于准入后的业务对话；不代替独立的文本清洗样本、正常宿主准入及物理存储验收。只使用合成内容，不访问模型、用户会话或用户配置。
- 原生入口：`postaction_parity_tests.rs` 调用生产 `render_llm_input`，比较 JSON 语义、最终实际 `raw_turn` 字节的 Token 成本和准确历史身份。使用 JSON 流解码器的消费边界截取原字节，不通过解码后重序列化抹掉转义成本。
- 允许映射仅为外置数字 Turn ID 到原生 `vmm-turn-数字`；不归一掉内容、预算或历史集合差异。JSON 对象成员顺序不是业务差异，但必须保留相同成本和原字段含义。
- 验证命令：`rtk proxy cargo test -p vulcan-service-kernel --lib postaction_parity_tests -- --test-threads=1 --nocapture`。此对照不是全包测试、完整宿主验收、真实模型质量验收或完整审核轮次。
- 最后实际生成与独立重新生成的 SHA256 一致：`be751edcd87137c0aadbcede47c132ecd8932a2e17f211e00bde2d794a3404f8`。首轮发现 HTML／Unicode 分隔符导致成本和历史选择差异；修正原生目标与最终输出的五种 Go JSON 转义后，两项生产投影对照全部通过。
