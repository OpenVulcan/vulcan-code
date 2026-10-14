// Generated from Rust by vulcan-service-kernel `generated_ts_constants`; do not edit.
// 由 vulcan-service-kernel `generated_ts_constants` 从 Rust 生成；请勿手动编辑。

/** Maximum UTF-8 byte length of one V1 client action identity.
 * 单个 V1 客户端动作身份的最大 UTF-8 字节长度。 */
export const CLIENT_ACTION_ID_MAX_BYTES = 256;

/** Maximum resource references accepted by one V1 resource list request.
 * 单个 V1 资源列表请求接受的最大资源引用数。 */
export const SESSION_RESOURCE_LIST_V1_MAX_ITEMS = 256;

/** Service hard limit on the bytes of one project terminal input batch.
 * 服务端对单个项目终端输入批次字节数的硬限制。 */
export const PROJECT_TERMINAL_INPUT_BATCH_MAX_BYTES = 65536;

/** Service hard limit on the input events carried by one project terminal batch.
 * 服务端对单个项目终端批次承载输入事件数的硬限制。 */
export const PROJECT_TERMINAL_INPUT_BATCH_MAX_EVENTS = 256;

/** Most data rows one ui-* answer component renders; larger fences stay code.
 * 单个 ui-* 回答组件渲染的最多数据行数；更大的围栏保持为代码。 */
export const ANSWER_COMPONENT_MAX_ROWS = 128;

/** Most value series one ui-chart carries, equal to the chart palette size.
 * 单个 ui-chart 承载的最多数值系列数，等于图表配色数量。 */
export const ANSWER_CHART_MAX_SERIES = 8;

/** Most lines one ui-code excerpt quotes; a longer range keeps the fence as code.
 * 单个 ui-code 摘录引用的最多行数；更长的范围使围栏保持为代码。 */
export const ANSWER_CODE_MAX_LINES = 200;

/** Most distinct ui-code excerpts one reply captures, also the row limit of one fence.
 * 单条回答抓取的最多不同 ui-code 摘录数，也是单个围栏的行数上限。 */
export const ANSWER_CODE_MAX_EXCERPTS = 16;

/** System default answer mode for users who never chose one.
 * 从未选择回答模式的用户所用的系统默认回答模式。 */
export const DEFAULT_ANSWER_MODE = "interactive";
