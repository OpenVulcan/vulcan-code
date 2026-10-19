import { WorkspaceDirectoryLayout, WorkspaceDirectoryMenu } from "@/components/workspace/workspace-directory-layout"
import { Palette } from "lucide-react"
import { useCallback, useState, type ComponentType } from "react"

import { PageBody, PageHint, PageTabPanel, PageToolbar } from "@/components/page"
import { Badge } from "@/ui-kit/primitives/badge"
import { useI18n } from "@/i18n"

import { VisualLabContent, VisualLabNavigation } from "./visual-lab/visual-lab-content"
import { VisualLabActions, VisualLabForms } from "./visual-lab/visual-lab-controls"
import { VisualLabFoundations } from "./visual-lab/visual-lab-foundations"
import { VisualLabMenus, VisualLabModals } from "./visual-lab/visual-lab-overlays"
import { VisualLabPageKit } from "./visual-lab/visual-lab-page-kit"
import { VisualLabPluginModule, VisualLabPluginSettings } from "./visual-lab/visual-lab-plugin-pages"
import { visualLabText, type VisualLabDemoProps, type VisualLabText } from "./visual-lab/visual-lab-shared"
import { VisualLabUsage } from "./visual-lab/visual-lab-usage"

// VisualLabCategory binds one directory entry to the demo component that renders its page sections.
// VisualLabCategory 将一个目录入口绑定到渲染其页面分区的演示组件。
interface VisualLabCategory {
  // id is the stable local category identity.
  // id 是稳定的本地分类标识。
  id: string
  // label picks the localized directory label.
  // label 选择本地化的目录标签。
  label: (text: VisualLabText) => string
  // Demo renders the category's page sections.
  // Demo 渲染该分类的页面分区。
  Demo: ComponentType<VisualLabDemoProps>
}

// VISUAL_LAB_CATEGORIES lists categories from foundations to composed pages to primitives.
// VISUAL_LAB_CATEGORIES 按基础、组合页面、原语的顺序列出分类。
const VISUAL_LAB_CATEGORIES: readonly VisualLabCategory[] = [
  { id: "foundations", label: (text) => text("视觉基础", "Foundations"), Demo: VisualLabFoundations },
  { id: "page-kit", label: (text) => text("页面套件", "Page kit"), Demo: VisualLabPageKit },
  { id: "plugin-settings", label: (text) => text("插件设置页", "Plugin settings page"), Demo: VisualLabPluginSettings },
  { id: "plugin-module", label: (text) => text("插件模块页", "Plugin module page"), Demo: VisualLabPluginModule },
  { id: "actions", label: (text) => text("按钮与状态", "Actions and status"), Demo: VisualLabActions },
  { id: "forms", label: (text) => text("表单控件", "Form controls"), Demo: VisualLabForms },
  { id: "content", label: (text) => text("内容表面", "Content surfaces"), Demo: VisualLabContent },
  { id: "navigation", label: (text) => text("导航与反馈", "Navigation and feedback"), Demo: VisualLabNavigation },
  { id: "modals", label: (text) => text("模态表面", "Modal surfaces"), Demo: VisualLabModals },
  { id: "menus", label: (text) => text("菜单与浮层", "Menus and overlays"), Demo: VisualLabMenus },
  { id: "usage", label: (text) => text("产品组件 · 用量", "Product components · Usage"), Demo: VisualLabUsage },
]

// VisualLabSettingsArea renders the acceptance surface of the unified UI system, built from the page kit itself.
// VisualLabSettingsArea 渲染统一 UI 体系的验收页面，页面本身即由页面套件构成。
// Parameters: none; reads locale from context. Returns interactive local-only samples and explicitly synthetic data.
// 参数：无，从上下文读取语言。返回仅作用于本页的交互样本与明确合成的数据。
export function VisualLabSettingsArea() {
  const { locale } = useI18n()
  // text is stable per locale so demos that memoize on it, such as the plugin settings page, keep their state.
  // text 按语言保持稳定，使依赖它做记忆化的演示（如插件设置页）保留状态。
  const text = useCallback<VisualLabText>((zh, en) => visualLabText(locale, zh, en), [locale])
  // Category navigation is local to the lab and never changes product configuration.
  // 分类导航仅作用于实验室，不修改产品配置。
  const [activeCategory, setActiveCategory] = useState(VISUAL_LAB_CATEGORIES[0].id)
  // lastAction records local demo feedback without invoking product services.
  // lastAction 记录本地演示反馈，不调用产品服务。
  const [lastAction, setLastAction] = useState<string | null>(null)
  return (
    <WorkspaceDirectoryLayout contentClassName="visual-lab" navigation={
      <WorkspaceDirectoryMenu ariaLabel={text("视觉实验室分类", "Visual lab categories")} heading={text("分类", "Categories")}>
        {VISUAL_LAB_CATEGORIES.map((category) => <button key={category.id} type="button"
          className={`workspace-tool-panel-menu-row ${activeCategory === category.id ? "active" : ""}`}
          aria-current={activeCategory === category.id ? "page" : undefined}
          onClick={() => setActiveCategory(category.id)}>{category.label(text)}</button>)}
      </WorkspaceDirectoryMenu>
    }>
      <PageBody>
        <PageToolbar>
          <Badge variant="outline"><Palette />{text("统一视觉体系", "Unified visual system")}</Badge>
          <PageHint>
            {text(
              "这里使用真实组件展示合成样本。所有数字、错误、加载与进度都是演示数据；操作和选择仅影响本页，不会保存配置、复制链接或删除项目。",
              "Real components display synthetic samples here. All numbers, errors, loading and progress are demo data. Actions and selections affect this page only; they do not save settings, copy links or delete projects.",
            )}
          </PageHint>
          <PageHint className="visual-lab-status" data-visual-lab-status="" role="status">
            {lastAction ? `${text("已演示", "Demo action")}: ${lastAction}` : text("请选择任意示例检查交互状态。", "Choose an example to review its interaction state.")}
          </PageHint>
        </PageToolbar>
        {VISUAL_LAB_CATEGORIES.map(({ id, Demo }) => (
          <PageTabPanel key={id} active={activeCategory === id}>
            <Demo text={text} onAction={setLastAction} />
          </PageTabPanel>
        ))}
      </PageBody>
    </WorkspaceDirectoryLayout>
  )
}
