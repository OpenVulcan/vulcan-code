import { WorkspaceDirectoryLayout, WorkspaceDirectoryMenu } from "@/components/workspace/workspace-directory-layout";
import { Palette, Sparkles } from "lucide-react"
import { useRef, useState, type CSSProperties, type ReactNode, type MouseEvent, type KeyboardEvent } from "react"

import { UsagePanel, type UsagePanelViewModel } from "@/components/product/usage"
import { Alert, AlertDescription, AlertTitle } from "@/components/ui/alert"
import {
  AlertDialog,
  AlertDialogAction,
  AlertDialogCancel,
  AlertDialogContent,
  AlertDialogDescription,
  AlertDialogFooter,
  AlertDialogHeader,
  AlertDialogTitle,
} from "@/components/ui/alert-dialog"
import { Badge } from "@/components/ui/badge"
import { Button, IconButton } from "@/components/ui/button"
import {
  Card,
  CardContent,
  CardDescription,
  CardHeader,
  CardTitle,
} from "@/components/ui/card"
import { Checkbox } from "@/components/ui/checkbox"
import {
  FilterChip,
  FilterChipGroup,
  SegmentedControl,
  SegmentedControlItem,
} from "@/components/ui/choice-controls"
import {
  Dialog,
  DialogContent,
  DialogDescription,
  DialogFooter,
  DialogHeader,
  DialogTitle,
} from "@/components/ui/dialog"
import {
  ContextMenu,
  ContextMenuCheckboxItem,
  ContextMenuContent,
  ContextMenuItem,
  ContextMenuSeparator,
  ContextMenuShortcut,
  ContextMenuSub,
  ContextMenuSubContent,
  ContextMenuSubTrigger,
  ContextMenuTrigger,
} from "@/components/ui/context-menu"
import {
  DropdownMenu,
  DropdownMenuCheckboxItem,
  DropdownMenuContent,
  DropdownMenuItem,
  DropdownMenuLabel,
  DropdownMenuRadioGroup,
  DropdownMenuRadioItem,
  DropdownMenuSeparator,
  DropdownMenuShortcut,
  DropdownMenuSub,
  DropdownMenuSubContent,
  DropdownMenuSubTrigger,
  DropdownMenuTrigger,
} from "@/components/ui/dropdown-menu"
import {
  Empty,
  EmptyDescription,
  EmptyHeader,
  EmptyMedia,
  EmptyTitle,
} from "@/components/ui/empty"
import { Field, FieldDescription, FieldError, FieldLabel } from "@/components/ui/field"
import { Input } from "@/components/ui/input"
import { NavItem, TreeRow } from "@/components/ui/navigation-item"
import { HoverCard, HoverCardContent, HoverCardTrigger } from "@/components/ui/hover-card"
import { Popover, PopoverContent, PopoverDescription, PopoverHeader, PopoverTitle, PopoverTrigger } from "@/components/ui/popover"
import { Progress } from "@/components/ui/progress"
import { RadioGroup, RadioGroupItem } from "@/components/ui/radio-group"
import {
  Select,
  SelectContent,
  SelectItem,
  SelectTrigger,
  SelectValue,
} from "@/components/ui/select"
import {
  Sheet,
  SheetContent,
  SheetDescription,
  SheetFooter,
  SheetHeader,
  SheetTitle,
} from "@/components/ui/sheet"
import { Spinner } from "@/components/ui/spinner"
import { Surface } from "@/components/ui/surface"
import { Switch } from "@/components/ui/switch"
import { Tabs, TabsContent, TabsList, TabsTrigger } from "@/components/ui/tabs"
import { Textarea } from "@/components/ui/textarea"
import {
  Tooltip,
  TooltipContent,
  TooltipTrigger,
} from "@/components/ui/tooltip"
import { useI18n } from "@/i18n"
import { StatusPill } from "@/components/shared/status"

// VisualLabSectionProps describes one independently reviewable component family.
// VisualLabSectionProps 描述一个可独立验收的组件分组。
interface VisualLabSectionProps {
  // Hide inactive categories while keeping uncontrolled demo input state mounted.
  // 隐藏非当前分类，同时保留已挂载演示输入框的非受控状态。
  hidden: boolean
  // Visible examples rendered inside the section.
  // 分组内部展示的可见示例。
  children: ReactNode
  // Short explanation of the component family's responsibility.
  // 对组件分组职责的简短说明。
  description: string
  // Visible section title.
  // 分组显示标题。
  title: string
}

// VisualToken describes one semantic foundation token shown in the lab.
// VisualToken 描述视觉实验室展示的单个语义基础变量。
interface VisualToken {
  // CSS color expression used by the preview swatch.
  // 预览色块使用的 CSS 颜色表达式。
  color: string
  // Product-facing semantic token label.
  // 面向产品语义的变量名称。
  label: string
  // CSS custom property name.
  // CSS 自定义属性名称。
  token: string
}

// VisualTokenSwatchProps identifies the semantic token rendered by one swatch.
// VisualTokenSwatchProps 标识单个色块所呈现的语义变量。
interface VisualTokenSwatchProps {
  // Semantic token rendered by this swatch.
  // 当前色块呈现的语义变量。
  token: VisualToken
}

// VISUAL_TOKENS is the first stable semantic surface palette exposed for review.
// VISUAL_TOKENS 是首批用于验收的稳定语义表面色板。
const VISUAL_TOKENS: VisualToken[] = [
  { label: "Page", token: "--vcn-surface-page", color: "var(--vcn-surface-page)" },
  { label: "Panel", token: "--vcn-surface-panel", color: "var(--vcn-surface-panel)" },
  { label: "Subtle", token: "--vcn-surface-subtle", color: "var(--vcn-surface-subtle)" },
  { label: "Floating", token: "--vcn-surface-floating", color: "var(--vcn-surface-floating)" },
  { label: "Toolbar", token: "--vcn-surface-toolbar", color: "var(--vcn-surface-toolbar)" },
  { label: "Border", token: "--vcn-border-default", color: "var(--vcn-border-default)" },
]

// visualLabText selects compact lab copy for the active locale.
// visualLabText 根据当前语言选择紧凑的实验室文案。
function visualLabText(locale: string, zh: string, en: string): string {
  return locale === "zh-CN" ? zh : en
}

// VisualLabSection renders one consistent review surface for a component family.
// VisualLabSection 为一个组件分组渲染统一的验收表面。
function VisualLabSection(props: VisualLabSectionProps) {
  return (
    <section className="visual-lab-section" hidden={props.hidden}>
      <header className="visual-lab-section-heading">
        <h3>{props.title}</h3>
        <p>{props.description}</p>
      </header>
      {props.children}
    </section>
  )
}

// VisualTokenSwatch renders one semantic token and its resolved product role.
// VisualTokenSwatch 渲染一个语义变量及其对应的产品角色。
function VisualTokenSwatch(props: VisualTokenSwatchProps) {
  return (
    <article className="visual-lab-token">
      <div
        className="visual-lab-token-swatch"
        style={{ "--visual-lab-token-color": props.token.color } as CSSProperties}
      />
      <div className="visual-lab-token-copy">
        <strong>{props.token.label}</strong>
        <code>{props.token.token}</code>
      </div>
    </article>
  )
}

// VisualLabSettingsArea renders the first visible acceptance surface for the unified UI system.
// VisualLabSettingsArea 渲染统一 UI 体系首个可见的验收页面。
// Parameters: none; reads locale from context. Returns interactive local-only component samples and explicitly synthetic status data.
// 参数：无，从上下文读取语言。返回仅作用于本页的交互组件样本与明确合成的状态数据。
export function VisualLabSettingsArea() {
  // locale selects bilingual preview copy without introducing product state.
  // locale 仅用于选择双语预览文案，不引入产品状态。
  const { locale } = useI18n()
  // text binds the current locale to the compact copy selector.
  // text 将当前语言绑定到紧凑文案选择器。
  const text = (zh: string, en: string) => visualLabText(locale, zh, en)
  // Category navigation is local to the lab and never changes product configuration.
  // 分类导航仅作用于实验室，不修改产品配置。
  const [activeCategory, setActiveCategory] = useState("surfaces")
  // Each existing component family owns one localized directory entry.
  // 每个现有组件分组对应一个本地化目录入口。
  const categories = [
    { id: "surfaces", label: text("语义表面", "Semantic surfaces") },
    { id: "actions", label: text("按钮与状态", "Actions and status") },
    { id: "forms", label: text("表单控件", "Form controls") },
    { id: "content", label: text("内容表面", "Content surfaces") },
    { id: "modals", label: text("模态表面", "Modal surfaces") },
    { id: "usage", label: text("自定义产品组件 · 用量", "Product components · Usage") },
    { id: "navigation", label: text("导航与反馈", "Navigation and feedback") },
    { id: "menus", label: text("菜单与浮层", "Menus and overlays") },
  ]

  // dialogPreview identifies the modal surface currently opened from the fixed acceptance section.
  // dialogPreview 标识当前从固定验收区域打开的模态表面。
  const [dialogPreview, setDialogPreview] = useState<"dialog" | "alert" | "sheet" | null>(null)
  // labFilter stores the interactive filter-chip example without affecting product data.
  // labFilter 保存交互式筛选标签示例，不影响产品数据。
  const [labFilter, setLabFilter] = useState<"all" | "active" | "issues">("all")
  // previewAction records local demo feedback without invoking product services.
  // previewAction 记录本地演示反馈，不调用产品服务。
  const [lastAction, setLastAction] = useState<string | null>(null)
  // Parameters: event identifies the activated demo control; records its accessible label and returns no value, without invoking product services.
  // 参数：event 标识被触发的演示控件；记录其可访问标签，不返回值，也不调用产品服务。
  const previewAction = (event: MouseEvent<HTMLElement>) => setLastAction(event.currentTarget.getAttribute("aria-label") ?? event.currentTarget.textContent ?? "")
  // Menu selections are controlled because Radix menu items do not own checked state.
  // 菜单选择采用受控状态，因为 Radix 菜单项不持有勾选状态。
  const [showActivity, setShowActivity] = useState(true)
  const [pinned, setPinned] = useState(true)
  const [menuDensity, setMenuDensity] = useState("balanced")
  // Local navigation demonstrates selection without pretending to change application routes.
  // 本地导航演示选中状态，不冒充应用路由切换。
  const [navigationSample, setNavigationSample] = useState("current")
  const [treeSample, setTreeSample] = useState(1)
  // modalTrigger preserves the actual launcher for controlled modal focus restoration.
  // modalTrigger 保留实际启动按钮，用于受控模态关闭后的焦点恢复。
  const modalTrigger = useRef<HTMLButtonElement | null>(null)
  // Parameters: kind selects the demo modal and event identifies its real launcher; records focus ownership, opens the modal and returns no value.
  // 参数：kind 选择演示模态，event 标识实际启动者；记录焦点归属并打开模态，不返回值。
  const openPreview = (kind: "dialog" | "alert" | "sheet", event: MouseEvent<HTMLButtonElement>) => {
    modalTrigger.current = event.currentTarget
    setDialogPreview(kind)
  }
  // Parameters: event is the primitive close-autofocus event; prevents default restoration, focuses the saved launcher and returns no value.
  // 参数：event 是原语关闭自动聚焦事件；阻止默认恢复并聚焦已保存启动者，不返回值。
  const restoreModalFocus = (event: Event) => { event.preventDefault(); modalTrigger.current?.focus() }
  // openContextMenuKeyboard exposes the same native context-menu event to focused keyboard users.
  // openContextMenuKeyboard 向已聚焦的键盘用户提供同一个原生上下文菜单事件。
  // Parameters: event is the focused trigger key event; recognized menu keys dispatch the native context-menu event and return no value.
  // 参数：event 是已聚焦触发器的键盘事件；识别菜单按键后派发原生上下文菜单事件，不返回值。
  const openContextMenuKeyboard = (event: KeyboardEvent<HTMLElement>) => {
    if (event.key !== "ContextMenu" && !(event.shiftKey && event.key === "F10")) return
    event.preventDefault()
    const rect = event.currentTarget.getBoundingClientRect()
    event.currentTarget.dispatchEvent(new window.MouseEvent("contextmenu", { bubbles: true, clientX: rect.left + 8, clientY: rect.top + 8 }))
  }
  // usagePanelSamples defines fixed state fixtures without connecting the lab to product services.
  // usagePanelSamples 定义固定状态样本，且不把视觉实验室连接到产品服务。
  const usagePanelSamples: UsagePanelViewModel[] = [
    {
      title: text("本月用量", "Monthly usage"),
      description: text("供应商上报事实与已知费用。", "Provider-reported facts and known costs."),
      state: "ready",
      metrics: [
        { id: "calls", label: text("真实调用", "Physical attempts"), value: "12,480", description: text("包含重试", "Includes retries") },
        { id: "coverage", label: text("覆盖率", "Coverage"), value: "98.6%" },
        { id: "tokens", label: "Token", value: "8,942,110" },
      ],
      tokenBreakdown: {
        title: text("Token 分解", "Token breakdown"),
        items: [
          { id: "input", label: text("输入", "Input"), value: "5,106,302" },
          { id: "output", label: text("输出", "Output"), value: "3,835,808" },
        ],
      },
      cost: {
        label: text("已知费用", "Known cost"),
        amount: "USD 42.7819",
        source: text("供应商上报", "Provider reported"),
      },
      modelsTitle: text("模型用量", "Model usage"),
      models: [
        { id: "model-a", label: "GPT-5.6 Terra", provider: "OpenAI", tokens: "6,340,110 Token", requests: text("8,920 次调用", "8,920 requests") },
        { id: "model-b", label: "Claude Sonnet 4.6", provider: "Anthropic", tokens: "2,602,000 Token", requests: text("3,560 次调用", "3,560 requests") },
      ],
    },
    { title: text("空状态", "Empty state"), description: text("无事实时不显示虚构的零。", "No invented zero when facts are absent."), state: "empty", stateMessage: text("暂无用量记录", "No usage records") },
    { title: text("加载状态", "Loading state"), state: "loading", stateMessage: text("正在读取原生用量…", "Loading native usage…") },
    { title: text("部分可用", "Partial state"), state: "partial", stateLabel: text("部分", "Partial"), stateMessage: text("费用暂缺，Token 事实仍可查看。", "Cost is unavailable; token facts remain visible."), metrics: [{ id: "partial-token", label: "Token", value: "438,210" }] },
    { title: text("错误状态", "Error state"), state: "error", stateMessage: text("无法读取用量，请检查服务状态。", "Usage could not be loaded. Check service status.") },
    { title: text("长数字与窄宽度", "Long values at narrow width"), state: "ready", metrics: [{ id: "long-token", label: text("精确 Token 合计", "Exact token total"), value: "9,223,372,036,854,775,807" }], cost: { label: text("精确费用", "Exact cost"), amount: "CNY 12345678901234567890.123456789", source: text("本地估算", "Locally estimated") } },
  ]

  return (
    <WorkspaceDirectoryLayout contentClassName="visual-lab" navigationMode="tabs" navigation={
      <WorkspaceDirectoryMenu ariaLabel={text("视觉实验室分类", "Visual lab categories")} heading={text("分类", "Categories")}>
        {categories.map((category) => <button key={category.id} type="button"
          className={`workspace-tool-panel-menu-row ${activeCategory === category.id ? "active" : ""}`}
          aria-current={activeCategory === category.id ? "page" : undefined}
          onClick={() => setActiveCategory(category.id)}>{category.label}</button>)}
      </WorkspaceDirectoryMenu>
    }>
      <header className="visual-lab-intro">
        <Badge variant="outline">
          <Palette />
          {text("统一视觉体系", "Unified visual system")}
        </Badge>
        <h2>{text("视觉实验室", "Visual Lab")}</h2>
        <p>
          {text(
            "这里使用真实组件展示合成样本。所有数字、错误、加载与进度都是演示数据；操作和选择仅影响本页，不会保存配置、复制链接或删除项目。",
            "Real components display synthetic samples here. All numbers, errors, loading and progress are demo data. Actions and selections affect this page only; they do not save settings, copy links or delete projects.",
          )}
        </p>
        <p role="status" className="break-words">{lastAction ? `${text("已演示", "Demo action")}: ${lastAction}` : text("请选择任意示例检查交互状态。", "Choose an example to review its interaction state.")}</p>
      </header>

      <div className="visual-lab-grid">
        <VisualLabSection
          description={text("页面、面板、浮层、弱背景和工具栏的基础层级。", "Base hierarchy for pages, panels, floating surfaces, subtle regions, and toolbars.")}
          hidden={activeCategory !== "surfaces"}
          title={text("语义表面", "Semantic surfaces")}
        >
          <div className="visual-lab-token-grid">
            {VISUAL_TOKENS.map((token) => <VisualTokenSwatch key={token.token} token={token} />)}
          </div>
        </VisualLabSection>

        <VisualLabSection
          description={text("普通操作只选择明确等级和尺寸，不在业务页面重新设计。", "Ordinary actions select an approved hierarchy and size without page-owned skinning.")}
          hidden={activeCategory !== "actions"}
          title={text("按钮与状态", "Actions and status")}
        >
          <div className="visual-lab-stack">
            <div className="visual-lab-row">
              <Button onClick={previewAction}>{text("主要操作", "Primary")}</Button>
              <Button onClick={previewAction} variant="secondary">{text("次要操作", "Secondary")}</Button>
              <Button onClick={previewAction} variant="outline">{text("描边操作", "Outline")}</Button>
              <Button onClick={previewAction} variant="ghost">{text("弱操作", "Ghost")}</Button>
              <Button onClick={previewAction} variant="destructive">{text("危险操作", "Danger")}</Button>
            </div>
            <div className="visual-lab-row">
              <Button onClick={previewAction} size="xs">XS</Button>
              <Button onClick={previewAction} size="sm">SM</Button>
              <Button onClick={previewAction} size="default">MD</Button>
              <Button onClick={previewAction} size="lg">LG</Button>
              <Button disabled>{text("禁用", "Disabled")}</Button>
              <Tooltip>
                <TooltipTrigger asChild>
                  <IconButton onClick={previewAction} aria-label={text("提示示例", "Tooltip example")}>
                    <Sparkles />
                  </IconButton>
                </TooltipTrigger>
                <TooltipContent>{text("统一提示层", "Unified tooltip surface")}</TooltipContent>
              </Tooltip>
            </div>
            <div className="visual-lab-row">
              <IconButton onClick={previewAction} aria-label={text("超小图标按钮", "Extra-small icon button")} size="xs"><Sparkles /></IconButton>
              <IconButton onClick={previewAction} aria-label={text("小图标按钮", "Small icon button")} size="sm"><Sparkles /></IconButton>
              <IconButton onClick={previewAction} aria-label={text("中图标按钮", "Medium icon button")} size="md"><Sparkles /></IconButton>
              <IconButton onClick={previewAction} aria-label={text("大图标按钮", "Large icon button")} size="lg"><Sparkles /></IconButton>
              <IconButton onClick={previewAction} aria-label={text("危险图标按钮", "Destructive icon button")} variant="destructive"><Sparkles /></IconButton>
              <IconButton aria-label={text("禁用图标按钮", "Disabled icon button")} disabled><Sparkles /></IconButton>
            </div>
            <div className="visual-lab-row">
              <Badge>{text("默认", "Default")}</Badge>
              <Badge variant="secondary">{text("次要", "Secondary")}</Badge>
              <Badge variant="outline">{text("描边", "Outline")}</Badge>
              <Badge variant="destructive">{text("危险", "Danger")}</Badge>
              <Spinner aria-label={text("加载演示", "Loading demo")} />
            </div>
          </div>
        </VisualLabSection>

        <VisualLabSection
          description={text("输入、选择和布尔控件使用同一密度与状态语言。", "Inputs, selections, and boolean controls share one density and state language.")}
          hidden={activeCategory !== "forms"}
          title={text("表单控件", "Form controls")}
        >
          <div className="visual-lab-stack">
            <Field>
              <FieldLabel htmlFor="visual-lab-default-input">{text("普通输入", "Default input")}</FieldLabel>
              <Input defaultValue={text("可编辑内容", "Editable value")} id="visual-lab-default-input" />
              <FieldDescription>{text("标签、说明和控件使用统一间距。", "Label, description, and control use shared spacing.")}</FieldDescription>
            </Field>
            <div className="visual-lab-row visual-lab-control-sizes">
              <Input aria-label={text("紧凑输入", "Compact input")} defaultValue={text("紧凑", "Compact")} density="compact" />
              <Input aria-label={text("默认输入", "Default input")} defaultValue={text("默认", "Default")} />
              <Input aria-label={text("大型输入", "Large input")} defaultValue={text("大型", "Large")} density="large" />
            </div>
            <Field data-invalid="true">
              <FieldLabel htmlFor="visual-lab-invalid-input">{text("错误状态", "Invalid state")}</FieldLabel>
              <Input aria-invalid="true" aria-describedby="visual-lab-input-error" defaultValue={text("无效内容", "Invalid value")} id="visual-lab-invalid-input" />
              <FieldError id="visual-lab-input-error">{text("这里显示统一的字段错误信息。", "A unified field error appears here.")}</FieldError>
            </Field>
            <Field>
              <FieldLabel htmlFor="visual-lab-textarea">{text("多行输入", "Multiline input")}</FieldLabel>
              <Textarea defaultValue={text("用于检查字体、边框、内边距和换行。", "Checks typography, borders, padding, and wrapping.")} id="visual-lab-textarea" />
            </Field>
            <Select defaultValue="balanced">
              <SelectTrigger aria-label={text("选择密度", "Select density")}>
                <SelectValue placeholder={text("选择密度", "Select density")} />
              </SelectTrigger>
              <SelectContent>
                <SelectItem value="compact">{text("紧凑", "Compact")}</SelectItem>
                <SelectItem value="balanced">{text("均衡", "Balanced")}</SelectItem>
                <SelectItem value="comfortable">{text("宽松", "Comfortable")}</SelectItem>
              </SelectContent>
            </Select>
            <div className="visual-lab-row">
              <label className="visual-lab-choice-line">
                <Checkbox defaultChecked />
                {text("复选框", "Checkbox")}
              </label>
              <label className="visual-lab-choice-line">
                <Switch defaultChecked />
                {text("开关", "Switch")}
              </label>
            </div>
            <RadioGroup aria-label={text("演示模式", "Demo mode")} defaultValue="standard">
              <label className="visual-lab-choice-line">
                <RadioGroupItem value="standard" />
                {text("标准模式", "Standard mode")}
              </label>
              <label className="visual-lab-choice-line">
                <RadioGroupItem value="focused" />
                {text("聚焦模式", "Focused mode")}
              </label>
            </RadioGroup>
          </div>
        </VisualLabSection>

        <VisualLabSection
          description={text("普通内容表面统一使用相同的边框、圆角、间距和状态。", "Ordinary content surfaces share borders, radii, spacing, and status treatments.")}
          hidden={activeCategory !== "content"}
          title={text("内容表面", "Content surfaces")}
        >
          <div className="visual-lab-card-grid">
            <Card>
              <CardHeader>
                <CardTitle>{text("标准卡片", "Standard card")}</CardTitle>
                <CardDescription>{text("普通信息层级", "Ordinary information hierarchy")}</CardDescription>
              </CardHeader>
              <CardContent className="visual-lab-caption">
                {text("业务组件负责内容，基础表面由统一体系提供。", "Product components own content while the unified system owns the base surface.")}
              </CardContent>
            </Card>
            <Card size="sm">
              <CardHeader>
                <CardTitle>{text("紧凑卡片", "Compact card")}</CardTitle>
                <CardDescription>{text("用于密集设置区域", "For dense settings surfaces")}</CardDescription>
              </CardHeader>
            </Card>
            <Card variant="interactive" tabIndex={0} role="button" onClick={previewAction} onKeyDown={(event) => {
              if (event.key === "Enter" || event.key === " ") { event.preventDefault(); event.currentTarget.click() }
            }}>
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
          <div className="visual-lab-surface-grid">
            <Surface className="visual-lab-surface-sample" variant="default">Default</Surface>
            <Surface className="visual-lab-surface-sample" variant="muted">Muted</Surface>
            <Surface className="visual-lab-surface-sample" variant="outlined">Outlined</Surface>
            <Surface className="visual-lab-surface-sample" variant="selected">Selected</Surface>
          </div>
          <div className="visual-lab-row">
            <StatusPill label={text("就绪", "Ready")} status="ready" />
            <StatusPill label={text("等待", "Pending")} status="pending" />
            <StatusPill label={text("警告", "Warning")} status="warning" />
            <StatusPill label={text("错误", "Error")} status="error" />
          </div>
          <Alert>
            <AlertTitle>{text("普通提示", "Informational alert")}</AlertTitle>
            <AlertDescription>{text("用于确认边框、正文和图标层级。", "Checks border, body copy, and icon hierarchy.")}</AlertDescription>
          </Alert>
          <Alert variant="warning">
            <AlertTitle>{text("注意事项", "Warning alert")}</AlertTitle>
            <AlertDescription>{text("警告使用稳定语义色，不与危险错误混用。", "Warnings use a stable semantic color distinct from destructive errors.")}</AlertDescription>
          </Alert>
          <Alert variant="destructive">
            <AlertTitle>{text("危险提示", "Destructive alert")}</AlertTitle>
            <AlertDescription>{text("危险语义必须保持清晰但不过度抢眼。", "Danger must remain clear without overwhelming the interface.")}</AlertDescription>
          </Alert>
          <Empty variant="outlined">
            <EmptyHeader>
              <EmptyMedia variant="icon"><Palette /></EmptyMedia>
              <EmptyTitle>{text("描边空状态", "Outlined empty state")}</EmptyTitle>
              <EmptyDescription>{text("空状态的表面由语义参数明确选择。", "The empty-state surface is selected through a semantic variant.")}</EmptyDescription>
            </EmptyHeader>
          </Empty>
        </VisualLabSection>

        <VisualLabSection
          description={text("普通弹窗、确认弹窗和侧边抽屉使用统一表面、尺寸与操作区。", "Dialogs, confirmations, and sheets share surface, sizing, and action-area contracts.")}
          hidden={activeCategory !== "modals"}
          title={text("模态表面", "Modal surfaces")}
        >
          <div className="visual-lab-row">
            <Button onClick={(event) => openPreview("dialog", event)} variant="outline">{text("普通弹窗", "Dialog")}</Button>
            <Button onClick={(event) => openPreview("alert", event)} variant="outline">{text("确认弹窗", "Confirmation")}</Button>
            <Button onClick={(event) => openPreview("sheet", event)} variant="outline">{text("侧边抽屉", "Sheet")}</Button>
          </div>
          <Dialog open={dialogPreview === "dialog"} onOpenChange={(open) => setDialogPreview(open ? "dialog" : null)}>
            <DialogContent size="md" onCloseAutoFocus={restoreModalFocus}>
              <DialogHeader>
                <DialogTitle>{text("标准表单弹窗", "Standard form dialog")}</DialogTitle>
                <DialogDescription>{text("检查标题、说明、内容间距、关闭按钮和底部操作区。", "Review title, description, spacing, close action, and footer.")}</DialogDescription>
              </DialogHeader>
              <Field>
                <FieldLabel htmlFor="visual-lab-dialog-input">{text("示例字段", "Example field")}</FieldLabel>
                <Input defaultValue={text("示例内容", "Example value")} id="visual-lab-dialog-input" />
              </Field>
              <DialogFooter>
                <Button onClick={() => setDialogPreview(null)} variant="outline">{text("取消", "Cancel")}</Button>
                <Button onClick={(event) => { previewAction(event); setDialogPreview(null) }}>{text("完成演示", "Finish demo")}</Button>
              </DialogFooter>
            </DialogContent>
          </Dialog>
          <AlertDialog open={dialogPreview === "alert"} onOpenChange={(open) => setDialogPreview(open ? "alert" : null)}>
            <AlertDialogContent onCloseAutoFocus={restoreModalFocus}>
              <AlertDialogHeader>
                <AlertDialogTitle>{text("确认当前操作？", "Confirm this action?")}</AlertDialogTitle>
                <AlertDialogDescription>{text("确认弹窗只承载需要明确决定的操作。", "Confirmations are reserved for explicit decisions.")}</AlertDialogDescription>
              </AlertDialogHeader>
              <AlertDialogFooter>
                <AlertDialogCancel>{text("取消", "Cancel")}</AlertDialogCancel>
                <AlertDialogAction onClick={previewAction}>{text("确认演示", "Confirm demo")}</AlertDialogAction>
              </AlertDialogFooter>
            </AlertDialogContent>
          </AlertDialog>
          <Sheet open={dialogPreview === "sheet"} onOpenChange={(open) => setDialogPreview(open ? "sheet" : null)}>
            <SheetContent size="md" onCloseAutoFocus={restoreModalFocus}>
              <SheetHeader>
                <SheetTitle>{text("标准侧边抽屉", "Standard side sheet")}</SheetTitle>
                <SheetDescription>{text("用于需要保留主界面上下文的辅助编辑。", "Used for auxiliary editing that preserves page context.")}</SheetDescription>
              </SheetHeader>
              <div className="px-4 text-sm text-muted-foreground">{text("抽屉内容区域", "Sheet content area")}</div>
              <SheetFooter>
                <Button onClick={(event) => { previewAction(event); setDialogPreview(null) }}>{text("完成演示", "Finish demo")}</Button>
              </SheetFooter>
            </SheetContent>
          </Sheet>
        </VisualLabSection>

        <VisualLabSection
          description={text("产品组件只接收确定的视图模型；统一验证正常、空、加载、部分、错误、长数字与窄宽度。", "Product components consume determined view models only; ready, empty, loading, partial, error, long-value, and narrow states are reviewed together.")}
          hidden={activeCategory !== "usage"}
          title={text("自定义产品组件 · 用量", "Product components · Usage")}
        >
          <div className="grid items-start gap-3 xl:grid-cols-2">
            {usagePanelSamples.map((sample, index) => (
              <div className={index === usagePanelSamples.length - 1 ? "max-w-[280px]" : undefined} key={`${sample.state}-${sample.title}`}>
                <UsagePanel model={sample} />
              </div>
            ))}
          </div>
        </VisualLabSection>

        <VisualLabSection
          description={text("导航模式和进度状态用于后续统一前的固定对照。", "Navigation and progress states provide a stable reference for later unification.")}
          hidden={activeCategory !== "navigation"}
          title={text("导航与反馈", "Navigation and feedback")}
        >
          <div className="visual-lab-stack">
            <SegmentedControl aria-label={text("密度演示", "Density demo")} defaultValue="balanced">
              <SegmentedControlItem value="compact">{text("紧凑", "Compact")}</SegmentedControlItem>
              <SegmentedControlItem value="balanced">{text("均衡", "Balanced")}</SegmentedControlItem>
              <SegmentedControlItem value="comfortable">{text("宽松", "Comfortable")}</SegmentedControlItem>
            </SegmentedControl>
            <FilterChipGroup aria-label={text("状态筛选", "Status filter")}>
              <FilterChip onClick={() => setLabFilter("all")} selected={labFilter === "all"}>{text("全部", "All")}</FilterChip>
              <FilterChip onClick={() => setLabFilter("active")} selected={labFilter === "active"}>{text("启用", "Active")}</FilterChip>
              <FilterChip onClick={() => setLabFilter("issues")} selected={labFilter === "issues"}>{text("问题", "Issues")}</FilterChip>
            </FilterChipGroup>
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
          </div>
          <Tabs defaultValue="overview">
            <TabsList>
              <TabsTrigger value="overview">{text("概览", "Overview")}</TabsTrigger>
              <TabsTrigger value="details">{text("详情", "Details")}</TabsTrigger>
            </TabsList>
            <TabsContent value="overview">
              <div className="visual-lab-progress">
                <span className="visual-lab-caption">{text("处理中 64%", "Processing 64%")}</span>
                <Progress aria-label={text("演示进度", "Demo progress")} value={64} />
              </div>
            </TabsContent>
            <TabsContent value="details">
              <p className="visual-lab-caption">{text("详情页签内容。", "Details tab content.")}</p>
            </TabsContent>
          </Tabs>
          <Empty>
            <EmptyHeader>
              <EmptyMedia variant="icon"><Palette /></EmptyMedia>
              <EmptyTitle>{text("空状态标题", "Empty state title")}</EmptyTitle>
              <EmptyDescription>{text("空状态也必须来自统一视觉体系。", "Empty states also belong to the unified visual system.")}</EmptyDescription>
            </EmptyHeader>
          </Empty>
        </VisualLabSection>

        <VisualLabSection
          description={text("下拉菜单、右键菜单与轻量浮层共享表面、密度、选中状态和碰撞翻转规则。", "Dropdowns, context menus, and lightweight overlays share surfaces, density, selection states, and collision handling.")}
          hidden={activeCategory !== "menus"}
          title={text("菜单与浮层", "Menus and overlays")}
        >
          <div className="visual-lab-stack">
            <div className="visual-lab-row">
              <DropdownMenu>
                <DropdownMenuTrigger asChild>
                  <Button variant="outline">{text("打开下拉菜单", "Open dropdown")}</Button>
                </DropdownMenuTrigger>
                <DropdownMenuContent width="md">
                  <DropdownMenuLabel>{text("显示设置", "Display settings")}</DropdownMenuLabel>
                  <DropdownMenuCheckboxItem checked={showActivity} onCheckedChange={(checked) => setShowActivity(checked === true)}>{text("显示活动状态", "Show activity")}</DropdownMenuCheckboxItem>
                  <DropdownMenuRadioGroup value={menuDensity} onValueChange={setMenuDensity}>
                    <DropdownMenuRadioItem value="compact">{text("紧凑密度", "Compact density")}</DropdownMenuRadioItem>
                    <DropdownMenuRadioItem value="balanced">{text("均衡密度", "Balanced density")}</DropdownMenuRadioItem>
                  </DropdownMenuRadioGroup>
                  <DropdownMenuSeparator />
                  <DropdownMenuSub>
                    <DropdownMenuSubTrigger>{text("更多操作", "More actions")}</DropdownMenuSubTrigger>
                    <DropdownMenuSubContent>
                      <DropdownMenuItem onSelect={(event) => setLastAction((event.currentTarget as HTMLElement).textContent ?? "")}>{text("复制链接", "Copy link")}<DropdownMenuShortcut>Ctrl+C</DropdownMenuShortcut></DropdownMenuItem>
                      <DropdownMenuItem variant="destructive" onSelect={(event) => setLastAction((event.currentTarget as HTMLElement).textContent ?? "")}>{text("删除项目", "Delete project")}</DropdownMenuItem>
                    </DropdownMenuSubContent>
                  </DropdownMenuSub>
                </DropdownMenuContent>
              </DropdownMenu>

              <Popover>
                <PopoverTrigger asChild>
                  <Button variant="outline">Popover</Button>
                </PopoverTrigger>
                <PopoverContent width="md">
                  <PopoverHeader>
                    <PopoverTitle>{text("轻量信息面板", "Lightweight panel")}</PopoverTitle>
                    <PopoverDescription>{text("用于少量说明或紧凑操作，不承担完整弹窗职责。", "For concise guidance or compact actions, not full dialog workflows.")}</PopoverDescription>
                  </PopoverHeader>
                </PopoverContent>
              </Popover>

              <HoverCard>
                <HoverCardTrigger asChild>
                  <Button variant="ghost">HoverCard</Button>
                </HoverCardTrigger>
                <HoverCardContent width="md">
                  <strong>{text("悬停详情", "Hover details")}</strong>
                  <p className="visual-lab-caption">{text("检查延迟、驻留、边界翻转和长文本换行。", "Review delay, persistence, collision flipping, and long-text wrapping.")}</p>
                </HoverCardContent>
              </HoverCard>
            </div>

            <ContextMenu>
              <ContextMenuTrigger className="visual-lab-context-target focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-ring" tabIndex={0} onKeyDown={openContextMenuKeyboard}>
                {text("在此区域点击右键，检查上下文菜单", "Right-click this area to review the context menu")}
              </ContextMenuTrigger>
              <ContextMenuContent width="md">
                <ContextMenuItem onSelect={(event) => setLastAction((event.currentTarget as HTMLElement).textContent ?? "")}>{text("打开", "Open")}<ContextMenuShortcut>Enter</ContextMenuShortcut></ContextMenuItem>
                <ContextMenuCheckboxItem checked={pinned} onCheckedChange={(checked) => setPinned(checked === true)}>{text("固定到侧栏", "Pin to sidebar")}</ContextMenuCheckboxItem>
                <ContextMenuSub>
                  <ContextMenuSubTrigger>{text("移动到", "Move to")}</ContextMenuSubTrigger>
                  <ContextMenuSubContent>
                    <ContextMenuItem onSelect={(event) => setLastAction((event.currentTarget as HTMLElement).textContent ?? "")}>{text("工作区一", "Workspace one")}</ContextMenuItem>
                    <ContextMenuItem onSelect={(event) => setLastAction((event.currentTarget as HTMLElement).textContent ?? "")}>{text("工作区二", "Workspace two")}</ContextMenuItem>
                  </ContextMenuSubContent>
                </ContextMenuSub>
                <ContextMenuSeparator />
                <ContextMenuItem variant="destructive" onSelect={(event) => setLastAction((event.currentTarget as HTMLElement).textContent ?? "")}>{text("删除", "Delete")}</ContextMenuItem>
              </ContextMenuContent>
            </ContextMenu>
          </div>
        </VisualLabSection>
      </div>
    </WorkspaceDirectoryLayout>
  )
}
