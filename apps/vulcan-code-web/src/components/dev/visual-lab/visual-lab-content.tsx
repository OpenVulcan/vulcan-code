import { useState, type KeyboardEvent, type MouseEvent } from "react"

import { PageHint, PageSection, PageTabPanel, PageTabs } from "@/components/page"
import { Card, CardDescription, CardHeader, CardTitle } from "@/ui-kit/primitives/card"
import { FilterChip, FilterChipGroup, SegmentedControl, SegmentedControlItem } from "@/ui-kit/primitives/choice-controls"
import { NavItem, TreeRow } from "@/ui-kit/primitives/navigation-item"
import { Progress } from "@/ui-kit/primitives/progress"
import { Surface } from "@/ui-kit/primitives/surface"

import { actionLabel, type VisualLabDemoProps } from "./visual-lab-shared"

// SURFACE_VARIANTS lists the surface primitive variants in review order.
// SURFACE_VARIANTS 按验收顺序列出表面原语变体。
const SURFACE_VARIANTS = ["default", "muted", "outlined", "selected"] as const

// VisualLabContent renders the card and surface primitives that page sections are built from.
// VisualLabContent 渲染构成页面分区的卡片与表面原语。
// Parameters: props supplies copy and the local action recorder. Returns page sections of surface samples.
// 参数：props 提供文案与本地操作记录器。返回表面样本的页面分区。
export function VisualLabContent({ text, onAction }: VisualLabDemoProps) {
  // activate records a click or Enter/Space on the interactive card sample.
  // activate 记录交互卡片样本上的点击或 Enter/空格。
  const activate = (event: MouseEvent<HTMLElement>) => onAction(actionLabel(event))
  const activateByKey = (event: KeyboardEvent<HTMLElement>) => {
    if (event.key !== "Enter" && event.key !== " ") return
    event.preventDefault()
    event.currentTarget.click()
  }
  return (
    <>
      <PageSection
        title={text("卡片", "Cards")}
        description={text("卡片只承载可独立理解的单元；页面分区本身已经是卡片，不再嵌套描边。", "Cards hold self-contained units; page sections are already cards, so avoid nesting another outline.")}
      >
        <div className="visual-lab-card-grid">
          <Card>
            <CardHeader>
              <CardTitle>{text("标准卡片", "Standard card")}</CardTitle>
              <CardDescription>{text("普通信息层级", "Ordinary information hierarchy")}</CardDescription>
            </CardHeader>
          </Card>
          <Card size="sm">
            <CardHeader>
              <CardTitle>{text("紧凑卡片", "Compact card")}</CardTitle>
              <CardDescription>{text("用于密集设置区域", "For dense settings surfaces")}</CardDescription>
            </CardHeader>
          </Card>
          <Card variant="interactive" tabIndex={0} role="button" onClick={activate} onKeyDown={activateByKey}>
            <CardHeader>
              <CardTitle>{text("可交互卡片", "Interactive card")}</CardTitle>
              <CardDescription>{text("悬停与键盘聚焦来自统一表面状态。", "Hover and keyboard focus come from the shared surface state.")}</CardDescription>
            </CardHeader>
          </Card>
          <Card variant="selected">
            <CardHeader>
              <CardTitle>{text("已选卡片", "Selected card")}</CardTitle>
              <CardDescription>{text("选中状态不再由页面自行定义。", "Selection is no longer restyled by each page.")}</CardDescription>
            </CardHeader>
          </Card>
        </div>
      </PageSection>
      <PageSection
        title={text("表面原语", "Surface primitives")}
        description={text("需要分组但不需要标题时使用表面；变体由语义参数选择。", "Use a surface to group without a heading; the variant is chosen semantically.")}
      >
        <div className="visual-lab-surface-grid">
          {SURFACE_VARIANTS.map((variant) => <Surface key={variant} className="visual-lab-surface-sample" variant={variant}>{variant}</Surface>)}
        </div>
      </PageSection>
    </>
  )
}

// VisualLabNavigation renders selection controls, navigation rows, page tabs and progress.
// VisualLabNavigation 渲染选择控件、导航行、页面标签与进度。
// Parameters: props supplies copy. Returns page sections with local-only navigation state.
// 参数：props 提供文案。返回仅持有本地导航状态的页面分区。
export function VisualLabNavigation({ text }: VisualLabDemoProps) {
  // Local navigation demonstrates selection without pretending to change application routes.
  // 本地导航演示选中状态，不冒充应用路由切换。
  const [filter, setFilter] = useState<"all" | "active" | "issues">("all")
  const [navigationSample, setNavigationSample] = useState("current")
  const [treeSample, setTreeSample] = useState(1)
  const [tab, setTab] = useState<"overview" | "details">("overview")
  return (
    <>
      <PageSection
        title={text("选择控件", "Selection controls")}
        description={text("分段控件切换互斥视图；筛选标签收窄列表内容。", "Segmented controls switch exclusive views; filter chips narrow list content.")}
      >
        <SegmentedControl aria-label={text("密度演示", "Density demo")} defaultValue="balanced">
          <SegmentedControlItem value="compact">{text("紧凑", "Compact")}</SegmentedControlItem>
          <SegmentedControlItem value="balanced">{text("均衡", "Balanced")}</SegmentedControlItem>
          <SegmentedControlItem value="comfortable">{text("宽松", "Comfortable")}</SegmentedControlItem>
        </SegmentedControl>
        <FilterChipGroup aria-label={text("状态筛选", "Status filter")}>
          <FilterChip onClick={() => setFilter("all")} selected={filter === "all"}>{text("全部", "All")}</FilterChip>
          <FilterChip onClick={() => setFilter("active")} selected={filter === "active"}>{text("启用", "Active")}</FilterChip>
          <FilterChip onClick={() => setFilter("issues")} selected={filter === "issues"}>{text("问题", "Issues")}</FilterChip>
        </FilterChipGroup>
      </PageSection>
      <PageSection
        title={text("导航行", "Navigation rows")}
        description={text("侧栏导航使用导航项；层级结构使用树行。", "Sidebars use navigation items; hierarchies use tree rows.")}
      >
        <div className="visual-lab-navigation-samples">
          <nav aria-label={text("导航项示例", "Navigation item example")}>
            <NavItem selected={navigationSample === "current"} onClick={() => setNavigationSample("current")}>{text("当前页面", "Current page")}</NavItem>
            <NavItem selected={navigationSample === "other"} onClick={() => setNavigationSample("other")}>{text("其他页面", "Other page")}</NavItem>
            <NavItem disabled>{text("禁用页面", "Disabled page")}</NavItem>
          </nav>
          <div role="group" aria-label={text("层级行外观示例", "Hierarchical row style examples")}>
            <TreeRow depth={0} role="button" selected={treeSample === 0} aria-pressed={treeSample === 0} onClick={() => setTreeSample(0)}>{text("工作区", "Workspace")}</TreeRow>
            <TreeRow depth={1} role="button" selected={treeSample === 1} aria-pressed={treeSample === 1} onClick={() => setTreeSample(1)}>{text("当前项目", "Current project")}</TreeRow>
            <TreeRow depth={2} role="button" selected={treeSample === 2} aria-pressed={treeSample === 2} onClick={() => setTreeSample(2)}>{text("子目录", "Nested folder")}</TreeRow>
          </div>
        </div>
      </PageSection>
      <PageSection
        title={text("页面标签", "Page tabs")}
        description={text("页内分区切换统一使用页面标签与标签面板，不再使用原语 Tabs。", "In-page section switches use page tabs with tab panels instead of the raw Tabs primitive.")}
      >
        <PageTabs
          ariaLabel={text("页面标签示例", "Page tab example")}
          value={tab}
          onValueChange={setTab}
          options={[{ value: "overview", label: text("概览", "Overview") }, { value: "details", label: text("详情", "Details"), count: 3 }]}
        />
        <PageTabPanel active={tab === "overview"}>
          <div className="visual-lab-progress">
            <PageHint>{text("处理中 64%", "Processing 64%")}</PageHint>
            <Progress aria-label={text("演示进度", "Demo progress")} value={64} />
          </div>
        </PageTabPanel>
        <PageTabPanel active={tab === "details"}>
          <PageHint>{text("面板首次显示时才挂载，之后保留状态。", "Panels mount on first view and keep their state afterwards.")}</PageHint>
        </PageTabPanel>
      </PageSection>
    </>
  )
}
