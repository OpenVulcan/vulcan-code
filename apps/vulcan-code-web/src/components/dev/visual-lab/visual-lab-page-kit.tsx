import { useState } from "react"
import { Inbox, Puzzle } from "lucide-react"

import {
  MetricCard, MetricGrid, PageActionBar, PageDetails, PageEmptyState, PageFacts, PageGuide, PageHint, PageItem, PageItemList,
  PageNotice, PageRow, PageSection, PageTable, PageToolbar, type PageFact, type PageTableColumn, type PageTableRow,
} from "@/components/page"
import { Badge } from "@/components/ui/badge"
import { Button } from "@/components/ui/button"
import { Input } from "@/components/ui/input"
import { Switch } from "@/components/ui/switch"

import type { VisualLabDemoProps } from "./visual-lab-shared"

// TABLE_PAGE_SIZE is the synthetic table page length; pagination labels derive from it.
// TABLE_PAGE_SIZE 是合成表格的每页行数；分页文案由其派生。
const TABLE_PAGE_SIZE = 3

// VisualLabPageKit demonstrates every page kit component in the order a real page composes them.
// VisualLabPageKit 按真实页面的组合顺序演示每个页面套件组件。
// Parameters: props supplies copy and the local action recorder. Returns synthetic page sections only.
// 参数：props 提供文案与本地操作记录器。只返回合成的页面分区。
export function VisualLabPageKit({ text, onAction }: VisualLabDemoProps) {
  const [selectedRow, setSelectedRow] = useState("gpt")
  const [draftEnabled, setDraftEnabled] = useState(false)
  const [saving, setSaving] = useState(false)
  // facts is one fact set reused by the three PageFacts layouts.
  // facts 是三种 PageFacts 版式共用的事实集合。
  const facts: PageFact[] = [
    { label: text("版本", "Version"), value: "1.4.2" },
    { label: text("来源", "Source"), value: text("内置", "Built-in") },
    { label: text("权限", "Permissions"), value: text("2 项", "2 granted") },
    { label: text("最后错误", "Last error"), value: text("凭据已过期", "Credential expired"), tone: "attention" },
  ]
  const columns: PageTableColumn[] = [
    { key: "model", label: text("模型", "Model"), width: "34%" },
    { key: "provider", label: text("供应商", "Provider"), width: "22%" },
    { key: "requests", label: text("调用", "Requests"), numeric: true, width: "16%" },
    { key: "note", label: text("备注", "Note"), wrap: true },
    { key: "select", label: <span className="sr-only">{text("选择", "Select")}</span>, width: "88px" },
  ]
  const rows: PageTableRow[] = [
    { key: "gpt", model: "GPT-5.6 Terra", provider: "OpenAI", requests: "8,920", note: text("长备注会在固定版式内换行，而不是撑破表格。", "Long notes wrap inside the fixed layout instead of stretching the table.") },
    { key: "claude", model: "Claude Sonnet 5.5", provider: "Anthropic", requests: "3,560", note: "—" },
    { key: "local", model: "qwen3-coder-32b", provider: text("本地", "Local"), requests: "412", note: "—" },
  ].map(({ key, ...cells }) => ({
    key,
    selected: key === selectedRow,
    cells: { ...cells, select: <Button size="xs" variant="ghost" aria-pressed={key === selectedRow} onClick={() => setSelectedRow(key)}>{text("选择", "Select")}</Button> },
  }))
  // save simulates one pending save so the action bar's pending state can be reviewed.
  // save 模拟一次保存中状态，以便验收操作栏的等待状态。
  const save = () => {
    setSaving(true)
    window.setTimeout(() => { setSaving(false); setDraftEnabled(false); onAction(text("保存设置", "Save settings")) }, 900)
  }
  return (
    <>
      <PageSection
        title={text("页面结构", "Page structure")}
        description={text("宿主页面与插件页面使用同一套结构；只在列出的位置放置对应组件。", "Host and plugin pages share one structure; place each component only where listed.")}
        guide={<PageGuide items={[
          { term: "PageBody", detail: text("页面根容器，负责纵向间距与可读宽度。", "Page root; owns vertical rhythm and readable width.") },
          { term: "PageTabs · PageTabPanel", detail: text("页内分区切换；面板首次显示时挂载并保留状态。", "In-page section switch; panels mount on first view and keep state.") },
          { term: "PageToolbar", detail: text("搜索、筛选与页面级命令；命令放在 actions。", "Search, filters and page commands; commands go in actions.") },
          { term: "PageNotice", detail: text("页面级结果与错误；行内结果放在对应分区内。", "Page-level outcomes and errors; inline outcomes stay inside their section.") },
          { term: "PageSection", detail: text("唯一的内容卡片；设置行使用 rows 版式。", "The only content card; settings rows use the rows layout.") },
          { term: "PageActionBar", detail: text("存在未保存草稿时出现的底部保存栏。", "Bottom save bar shown while a draft is unsaved.") },
        ]} />}
      />
      <PageToolbar actions={<Button size="sm" variant="outline" onClick={() => onAction(text("刷新", "Refresh"))}>{text("刷新", "Refresh")}</Button>}>
        <Input aria-label={text("搜索", "Search")} density="compact" placeholder={text("搜索模型…", "Search models…")} />
        <Badge variant="secondary">{text("3 个结果", "3 results")}</Badge>
        <PageHint>{text("工具栏示例", "Toolbar sample")}</PageHint>
      </PageToolbar>
      <PageNotice tone="info" title={text("说明", "Info")}>{text("中性信息，例如数据范围或保留期限。", "Neutral information such as data scope or retention.")}</PageNotice>
      <PageNotice tone="success">{text("设置已保存。", "Settings saved.")}</PageNotice>
      <PageNotice tone="warning" title={text("部分可用", "Partially available")}>{text("费用暂缺，Token 事实仍可查看。", "Cost is unavailable; token facts remain visible.")}</PageNotice>
      <PageNotice
        tone="error"
        title={text("读取失败", "Load failed")}
        actions={<Button size="sm" variant="outline" onClick={() => onAction(text("重试", "Retry"))}>{text("重试", "Retry")}</Button>}
        details={"service unavailable: connection refused (127.0.0.1:0)"}
        detailsLabel={text("技术详情", "Technical details")}
      >
        {text("无法读取数据。错误必须给出可执行的下一步。", "The data could not be loaded. Errors always offer a next step.")}
      </PageNotice>
      <MetricGrid>
        <MetricCard icon="requests" label={text("调用", "Requests")} value="12,480" detail={text("包含重试", "Includes retries")} />
        <MetricCard icon="tokens" label="Token" value="8.94M" />
        <MetricCard icon="success" label={text("成功率", "Success rate")} value="98.6%" />
        <MetricCard icon="duration" label={text("平均耗时", "Average latency")} value="1.8s" />
      </MetricGrid>
      <PageSection title={text("事实列表", "Facts")} description={text("grid 用于概览，list 用于窄栏，tiles 用于少量重点数值。", "grid for overviews, list for narrow columns, tiles for a few key values.")}>
        <PageFacts items={facts} layout="grid" />
        <PageFacts items={facts} layout="list" />
        <PageFacts items={facts.slice(0, 3)} layout="tiles" />
        <PageDetails label={text("更多详情", "More details")} items={[{ label: "ID", value: "vulcan-usage-statistics" }, { label: text("路径", "Path"), value: "plugins/vulcan-usage-statistics" }]} />
      </PageSection>
      <PageSection title={text("条目列表", "Item list")} description={text("同类对象的列表；每项可带图标、元信息、状态色与操作。", "Lists of like objects; each item may carry an icon, meta, tone and actions.")}>
        <PageItemList heading={text("插件", "Plugins")}>
          <PageItem icon={<Puzzle />} title="Context7" description={text("文档检索", "Documentation lookup")} meta="1.2.0" tone="success"
            actions={<Button size="sm" variant="outline" onClick={() => onAction("Context7")}>{text("设置", "Settings")}</Button>} />
          <PageItem icon={<Puzzle />} title="RTK" description={text("命令输出压缩", "Command output compaction")} meta="0.9.1" tone="attention" />
          <PageItem icon={<Puzzle />} title={text("已停用插件", "Disabled plugin")} meta="—" tone="danger" />
        </PageItemList>
      </PageSection>
      <PageSection
        title={text("数据表格", "Data table")}
        description={text("固定版式配合列宽；数值列右对齐；长文本列显式允许换行；选中行有统一样式。", "Fixed layout with column widths; numeric columns align right; long text columns opt into wrapping; selected rows share one style.")}
        status={<Badge variant="secondary">{rows.length}</Badge>}
        footer={<>
          <PageHint>{text(`第 1 页 · 每页 ${TABLE_PAGE_SIZE} 行`, `Page 1 · ${TABLE_PAGE_SIZE} rows per page`)}</PageHint>
          <Button size="sm" variant="outline" disabled>{text("上一页", "Previous")}</Button>
          <Button size="sm" variant="outline" disabled>{text("下一页", "Next")}</Button>
        </>}
      >
        <PageTable columns={columns} rows={rows.slice(0, TABLE_PAGE_SIZE)} label={text("模型用量", "Model usage")} layout="fixed" minWidth={560} />
        <PageTable columns={columns} rows={[]} label={text("空表格", "Empty table")} empty={<PageEmptyState title={text("没有匹配的记录", "No matching records")} />} />
      </PageSection>
      <PageSection title={text("空状态", "Empty states")} description={text("inline 用于分区内部；panel 用于整页或整个面板没有内容。", "inline inside a section; panel when a whole page or panel has no content.")}>
        <PageEmptyState icon={<Inbox />} title={text("暂无记录", "No records yet")} description={text("无事实时不显示虚构的零。", "No invented zero when facts are absent.")} />
        <PageEmptyState size="panel" icon={<Inbox />} title={text("选择一个会话", "Select a conversation")}
          action={<Button size="sm" onClick={() => onAction(text("新建会话", "New conversation"))}>{text("新建会话", "New conversation")}</Button>} />
      </PageSection>
      <PageSection layout="rows" title={text("保存栏", "Save bar")} description={text("修改任意设置后出现底部保存栏；保存期间按钮进入等待状态。", "Changing a setting reveals the bottom save bar; buttons wait while saving.")}>
        <PageRow label={text("示例设置", "Sample setting")} description={text("切换以产生未保存草稿。", "Toggle to create an unsaved draft.")}>
          <Switch aria-label={text("示例设置", "Sample setting")} checked={draftEnabled} onCheckedChange={setDraftEnabled} />
        </PageRow>
      </PageSection>
      {draftEnabled ? (
        <PageActionBar pending={saving} actions={<>
          <Button size="sm" variant="outline" disabled={saving} onClick={() => setDraftEnabled(false)}>{text("放弃", "Discard")}</Button>
          <Button size="sm" disabled={saving} onClick={save}>{saving ? text("保存中…", "Saving…") : text("保存", "Save")}</Button>
        </>}>
          {text("有未保存的更改", "Unsaved changes")}
        </PageActionBar>
      ) : null}
    </>
  )
}
