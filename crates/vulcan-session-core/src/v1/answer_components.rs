//! Bounds of the `ui-*` answer components that assistant Markdown may contain.
//! 助手 Markdown 中可包含的 `ui-*` 回答组件的边界。
//!
//! The WebUI and the terminal client each render the same fences natively; these values are the
//! single authority both read (the WebUI through generated constants). A fence exceeding them is
//! shown as ordinary code instead of a component.
//! WebUI 与终端客户端各自原生渲染同一组围栏；以下数值是两端共同读取的唯一权威（WebUI 经生成常量读取）。
//! 超出边界的围栏按普通代码显示，而非组件。

/// Most data rows (items, categories, files) one component renders.
/// 单个组件渲染的最多数据行（条目、分类、文件）数。
pub const ANSWER_COMPONENT_MAX_ROWS: usize = 128;

/// Most value series one `ui-chart` carries; equals the number of chart palette colors.
/// 单个 `ui-chart` 承载的最多数值系列数；等于图表配色数量。
pub const ANSWER_CHART_MAX_SERIES: usize = 8;
