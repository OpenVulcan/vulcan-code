# 部分缓存用量展示修复与审核记录

## 范围与根因

本次只处理会话 `5ffbbeaa-2936-4af1-bad9-192b0ba3586e` 的缓存统计缺失及同类展示问题。

已核对原始请求日志、规范 `api_stats`、终态 `turn_stats` 及当前源码：

- 实际路线为 `kimi / k3-256k / openai-chat-completions.v1`。
- 该轮 8 次成功请求中，首笔没有缓存计数；后续 7 笔同时包含 `usage.prompt_tokens_details.cached_tokens` 和相同数值的 `usage.cached_tokens`。现有转换器正确读取前者，规范事件已经保留 7 笔缓存值。
- 核心 `AggregatedUsage` 要求某维度全部样本已知才给出精确总量。首笔缺失使缓存、无缓存聚合保持未知。这是保护预算核算的既有契约，不能将缺失按零处理。
- 展示层以前只有精确总量，没有单独表达“已知部分累计和覆盖请求数”，因此把后续 7 笔已知事实一并隐藏。

故修复位于共享用量展示投影，不添加供应商字段猜测，不变更原始用量、预算规则、事件格式或持久化数据。

## 展示契约

1. 精确持久化总量优先；否则从当前回合规范 API 统计和最终子代理回流提取已知贡献。
2. 未知不是零；明确上报零值才计入已知样本。
3. 按用户后续确认，界面只显示累计数值和原有简短标签，不显示 `≥`、已知标签或请求覆盖说明；覆盖证据仍留在内部投影中，不改动原始用量和预算核算。
4. 缓存命中率只使用缓存与无缓存同时已知的同一批请求；禁止将部分缓存和完整输入分母混算，禁止跨不同请求配对。
5. 内部保留持久化请求数作为分母。按用户后续确认，空数值在界面默认显示为零，但不把零写回源数据，也不改变命中率的计算方式。
6. API 按身份及修订去重；最终子代理回流按子会话和子回合统计序号去重，首条可靠回流优先，与核心规则一致。API 用途不代表归属，压缩等用途仍计入所属会话。
7. 先应用直接及子代理范围的精确总量，再组合总计范围；即使子代理明细未加载，完整子代理统计桶也不会被丢弃。
8. 两条统计卡共享同一套投影、标记与比率计算。隐藏核算事件在可见性过滤之前纳入，展示证据不写回源事件。

脱敏真实样本的界面预期结果：缓存 `179,456`，无缓存 `16,090`，缓存命中率 `91.8%`。内部仍以已知的配对请求计算 `179456 / 195546`，不显示覆盖说明。输出 `1,089`、总量 `214,777` 不变。

## 文件覆盖清单

以下路径均相对于仓库根目录。

| 文件 | 变更及审核范围 |
| --- | --- |
| `apps/vulcan-code-web/src/service/conversation-usage-coverage.ts` | 新增纯视图覆盖投影、去重、范围组合及旧元数据边界校验 |
| `apps/vulcan-code-web/src/service/conversation-event-display-projection.ts` | 原生转录投影在过滤前收集核算证据 |
| `apps/vulcan-code-web/src/components/conversation/conversation-branch-presentation-v1.ts` | 主会话适配器附加独立展示证据，不替换精确总量 |
| `apps/vulcan-code-web/src/components/conversation/conversation-usage-display.tsx` | 共享数值格式与配对命中率；后续按用户确认去掉额外限定说明，空数值显示零 |
| `apps/vulcan-code-web/src/components/conversation/conversation-turn-stats-card.tsx` | 主会话统计卡接入已知贡献，保留缺失语义 |
| `apps/vulcan-code-web/src/components/conversation/conversation-v1-turn-stats-card.tsx` | 原生统计卡使用同一显示契约 |
| `apps/vulcan-code-web/src/styles/special-blocks.css` | 先新增覆盖说明样式，后续删除；恢复原布局，无最终变更 |
| `apps/vulcan-code-web/src/service/__fixtures__/kimi-partial-usage.ts` | 新增仅含脱敏用量及身份的真实回归样本，无提示词、端点、凭据 |
| `apps/vulcan-code-web/src/service/conversation-usage-coverage.test.ts` | 新增 11 项覆盖投影回归 |
| `apps/vulcan-code-web/src/components/conversation/conversation-partial-usage.test.tsx` | 新增 5 项真实投影及统计卡正文回归，包含简洁标签、零值展示和布局不变 |
| 本文件 | 新增根因、契约、验收与审核记录 |

只读核对：`crates/vulcan-session-core/src/v1/stats.rs`、`document.rs` 中的精确聚合与归属规则，以及窗口加载契约。以上核心文件没有修改。

## 验证结果

- 用户确认界面简化后，8 个相关前端测试文件共 96 项通过，其中新增 16 项；类型检查及生产构建再次通过。
- `rtk cargo test -p vulcan-session-core v1::stats::tests --lib`：8 项通过，原未知用量和目标预算语义保持不变。
- `rtk npm run lint`：通过。
- `rtk npm run check:ui-architecture`：通过。
- `rtk npm run build`：通过；Vite 仍报告超过 500 kB 的分包大小提示，本次未调整分包架构。
- `git diff --check`：通过。

## 连续审核记录

开发过程发现并修正的边界：精确子代理桶在明细未加载时不能丢失。第一版曾为部分值增加下界与覆盖标记，用户后续确认不展示这些额外信息，现已移除并更新回归断言。补充了未知子代理汇总场景的去重测试，避免精确统计桶覆盖测试结果而掩盖去重错误。

最终版本连续两轮人工源码审核：

1. **核算与归属轮：无新增问题。** 逐项对照原始字段、规范事件和核心统计契约，核对明确零值、全部未知、部分已知、完整已知、跨回合隔离、API 修订、子代理首条回流、用途归属和配对分母。
2. **集成与非回归轮：无新增问题。** 核对两条实际显示入口、隐藏元数据、分页缺失、旧摘要、标签截断、中文及英文显示、源事件不可变、无持久化写入、预算不变；运行上述测试、类型检查、架构检查与生产构建。

## 验证边界

- 旧会话已有规范 API 统计，不需重新调用模型或修复历史数据；加载本次前端后即可利用这些证据。
- 子代理若只回流了“未知”的汇总且没有可用分项证据，本次不会凭空恢复子会话的部分数值。
- 已验证两条统计卡的服务端渲染正文，未启动桌面版进行鼠标悬浮实机验收，未替换当前运行的桌面进程。
- 未提交代码，未修改供应商配置、会话文件或计费插件。
