// The page kit is the single structural vocabulary of host pages and plugin pages; styles live in styles/page-kit.css.
// 页面套件是宿主页面与插件页面唯一的结构词汇；样式位于 styles/page-kit.css。
export { PageActionBar, PageBody, PageTabPanel, PageTabs, PageToolbar, type PageActionBarProps, type PageBodyProps, type PageTabOption, type PageTabPanelProps, type PageToolbarProps } from "./page-layout"
export { PageGuide, PageHint, PageRow, PageSection, type PageGuideItem, type PageRowProps, type PageSectionProps } from "./page-section"
export { PageEmptyState, PageNotice, type PageEmptyStateProps, type PageNoticeProps, type PageNoticeTone } from "./page-feedback"
export {
  MetricCard, MetricGrid, PageDetails, PageFacts, PageItem, PageItemList, PageTable,
  type MetricCardIconName, type MetricCardProps, type PageDetailsProps, type PageFact, type PageFactsProps, type PageItemListProps, type PageItemProps,
  type PageTableColumn, type PageTableProps, type PageTableRow,
} from "./page-data"
