# 外置业务载荷与历史预算对照

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
