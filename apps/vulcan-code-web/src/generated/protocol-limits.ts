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

/** Parameterless notification a rendered HTML answer posts to close or restore its viewer.
 * 渲染后的 HTML 回答发送的无参数通知，用于关闭或还原其查看器。 */
export const HTML_ANSWER_CLOSE_MESSAGE = "vulcan-html-answer-close-v1";

/** Message type carrying only a rendered HTML answer's content height.
 * 仅携带渲染后 HTML 回答内容高度的消息类型。 */
export const HTML_ANSWER_HEIGHT_MESSAGE = "vulcan-html-answer-height-v1";
