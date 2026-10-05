import { Badge } from "@/components/ui/badge";
import { Button } from "@/components/ui/button";
import { Input } from "@/components/ui/input";
import { Switch } from "@/components/ui/switch";
import { StreamPlaceholder } from "@/components/ui/stream-placeholder";
import { Dialog, DialogContent, DialogDescription, DialogTitle } from "@/components/ui/dialog";
import { StatusPill } from "@/components/shared/status";
import {
  MetricCard, MetricGrid, PageActionBar, PageBody, PageDetails, PageEmptyState, PageFacts, PageGuide, PageHint, PageItem, PageItemList,
  PageNotice, PageRow, PageSection, PageTable, PageTabPanel, PageTabs, PageToolbar,
} from "@/components/page";
import {
  ActivityHeatmap, ActivityHeatmapLegend, ChartLegend, EmptyState, ErrorState, HelpTooltip, PluginMetadataLink, SelectControl,
  TextDocumentDialog, TimeSeriesLineChart,
} from "@/components/plugins/plugin-host-components";
import { BrowserArea } from "@/components/features/browser/browser-area";
import { ClientBrowserArea } from "@/components/features/browser/client-browser-area";

// PLUGIN_UI_SDK_COMPONENTS is the single list of host components a module mount receives as `sdk.components`.
// PLUGIN_UI_SDK_COMPONENTS 是模块挂载通过 `sdk.components` 获得的唯一宿主组件清单。
// The page kit entries are the same components host pages use, so plugin pages share structure and theme;
// docs/contracts/plugin-ui-authoring.md lists them for plugin authors and must change with this list.
// 页面套件条目与宿主页面使用的组件相同，使插件页面共享结构与主题；
// docs/contracts/plugin-ui-authoring.md 为插件作者列出它们，必须随此清单同步变更。
export const PLUGIN_UI_SDK_COMPONENTS = {
  // Page kit: layout. / 页面套件：布局。
  PageBody,
  PageToolbar,
  PageTabs,
  PageTabPanel,
  PageActionBar,
  // Page kit: sections and rows. / 页面套件：区块与行。
  PageSection,
  PageRow,
  PageHint,
  PageGuide,
  // Page kit: feedback. / 页面套件：反馈。
  PageNotice,
  PageEmptyState,
  // Page kit: data. / 页面套件：数据。
  PageFacts,
  PageDetails,
  MetricCard,
  MetricGrid,
  PageItemList,
  PageItem,
  PageTable,
  // Controls. / 控件。
  Badge,
  Button,
  Input,
  Switch,
  SelectControl,
  StatusPill,
  HelpTooltip,
  StreamPlaceholder,
  PluginMetadataLink,
  // Dialogs. / 对话框。
  Dialog,
  DialogContent,
  DialogTitle,
  DialogDescription,
  TextDocumentDialog,
  // Charts. / 图表。
  ActivityHeatmap,
  ActivityHeatmapLegend,
  TimeSeriesLineChart,
  ChartLegend,
  // Chart-area placeholders; full pages use PageEmptyState and PageNotice instead.
  // 图表区占位；完整页面改用 PageEmptyState 与 PageNotice。
  EmptyState,
  ErrorState,
  // Host-owned browser projections for the bundled browser plugin.
  // 内置浏览器插件使用的宿主自有浏览器投影。
  CoreBrowserArea: BrowserArea,
  ClientBrowserArea,
} as const;

// PluginUiSdkComponents is the component surface type of `sdk.components`.
// PluginUiSdkComponents 是 `sdk.components` 的组件接口类型。
export type PluginUiSdkComponents = typeof PLUGIN_UI_SDK_COMPONENTS;
