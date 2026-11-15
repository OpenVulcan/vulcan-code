import { AppWindow } from "lucide-react";
import { useI18n, type TranslationKey } from "@/i18n";
import { ToolFrame, type ToolCardProps } from "../tool-card";
import { numberOf, textOf } from "../tool-call";
import { JsonView } from "../views/json-view";
import { ErrorNote, MarkdownResult } from "../views/result-views";

// The public MCP name is built from the Vulcan Browser plugin server namespace and the original tool name.
// 公开 MCP 名称由 Vulcan Browser 插件的服务命名空间和原始工具名组成。
const PLAYWRIGHT_PREFIX = "mcp_vulcan_browser_playwright_";

// These are the Playwright actions exposed by Core and their localized labels.
// 这些是 Core 对外提供的 Playwright 动作及其本地化标签。
const ACTION_LABELS = {
  browser_navigate: "toolCard.browser.navigate",
  browser_navigate_back: "browser.back",
  browser_snapshot: "toolCard.browser.snapshot",
  browser_take_screenshot: "toolCard.browser.screenshot",
  browser_click: "toolCard.browser.click",
  browser_type: "toolCard.browser.type",
  browser_press_key: "toolCard.browser.pressKey",
  browser_hover: "toolCard.browser.hover",
  browser_select_option: "toolCard.browser.selectOption",
  browser_fill_form: "toolCard.browser.fillForm",
  browser_wait_for: "toolCard.browser.waitFor",
  browser_tabs: "browser.browserPages",
  browser_handle_dialog: "toolCard.browser.handleDialog",
  browser_console_messages: "toolCard.browser.console",
  browser_network_requests: "toolCard.browser.network",
  browser_resize: "browser.resizeViewport",
} as const satisfies Record<string, TranslationKey>;

// BrowserAction identifies a supported action on the conversation's Playwright browser.
// BrowserAction 标识会话的 Playwright 浏览器支持的动作。
type BrowserAction = keyof typeof ACTION_LABELS;

// Playwright adds terminal color codes to action logs. Strip only SGR styling so the diagnostic stays readable.
// Playwright 会给动作日志加终端配色代码。仅移除 SGR 样式，以保留可读的诊断。
const TERMINAL_STYLE = new RegExp(`${String.fromCharCode(27)}\\[[0-9;]*m`, "g");

/** Returns the known Playwright action for a canonical public tool name, or null for unrelated tools.
 * 根据规范公开工具名返回已知的 Playwright 动作，无关工具返回 null。 */
export function browserToolAction(name: string): BrowserAction | null {
  if (!name.startsWith(PLAYWRIGHT_PREFIX)) return null;
  // The suffix is checked against the approved actions before reaching the card.
  // 后缀经已批准动作列表校验后才进入工具卡。
  const action = name.slice(PLAYWRIGHT_PREFIX.length);
  return Object.hasOwn(ACTION_LABELS, action) ? action as BrowserAction : null;
}

/** Reads the label key for an action and its arguments, including the tab and dialog sub-actions.
 * 根据动作及其参数返回文案键，包含标签页和弹窗的具体操作。 */
function actionLabel(action: BrowserAction, input: ToolCardProps["call"]["input"]): TranslationKey {
  if (action === "browser_click" && input.doubleClick === true) return "toolCard.browser.doubleClick";
  if (action === "browser_handle_dialog") {
    if (input.accept === true) return "toolCard.browser.acceptDialog";
    if (input.accept === false) return "toolCard.browser.dismissDialog";
  }
  if (action === "browser_tabs") {
    switch (input.action) {
      case "new": return "browser.newTab";
      case "close": return "browser.closePage";
      case "select": return "toolCard.browser.selectTab";
      case "list": return "toolCard.browser.listTabs";
    }
  }
  return ACTION_LABELS[action];
}

/** Returns the target summary from the documented arguments for this action and locale.
 * 根据该动作的已声明参数及语言返回目标摘要。 */
function actionTarget(action: BrowserAction, input: ToolCardProps["call"]["input"], locale: string): string {
  switch (action) {
    case "browser_navigate": return textOf(input.url) ?? "";
    case "browser_click":
    case "browser_type":
    case "browser_hover":
    case "browser_select_option":
    case "browser_take_screenshot":
      // `element` is the optional readable description of `target`. Typed text stays in the arguments.
      // `element` 是 `target` 的可选可读描述。输入正文留在参数区。
      return textOf(input.element) ?? textOf(input.target) ?? "";
    case "browser_snapshot": return textOf(input.target) ?? "";
    case "browser_press_key": return textOf(input.key) ?? "";
    case "browser_console_messages": return textOf(input.level) ?? "";
    case "browser_network_requests": return textOf(input.filter) ?? "";
    case "browser_tabs": {
      // Tab index is zero-based, including when its value is zero.
      // 标签页索引从零开始，有值为零的情况。
      const index = numberOf(input.index);
      if (input.action === "select" || input.action === "close") return index === null ? "" : `#${index}`;
      return input.action === "new" ? textOf(input.url) ?? "" : "";
    }
    case "browser_resize": {
      // Dimensions are displayed together only when both were supplied.
      // 两个尺寸都已提供时才一起显示。
      const width = numberOf(input.width);
      const height = numberOf(input.height);
      return width === null || height === null ? "" : `${width} × ${height}`;
    }
    case "browser_wait_for": {
      // `time`, `text`, and `textGone` are explicit wait conditions accepted by the tool.
      // `time`、`text` 和 `textGone` 是工具接受的显式等待条件。
      const time = numberOf(input.time);
      return [
        time === null ? null : new Intl.NumberFormat(locale, { style: "unit", unit: "second", unitDisplay: "short" }).format(time),
        textOf(input.text), textOf(input.textGone),
      ].filter(Boolean).join(" · ");
    }
    default: return "";
  }
}

/** Renders the browser action, lifecycle, and actual result from the conversation event.
 * 根据会话事件渲染浏览器动作、生命周期及实际结果。 */
export function BrowserCard(props: ToolCardProps) {
  // Localized headings and units follow the current WebUI locale.
  // 本地化标题和单位使用当前 WebUI 语言。
  const { locale, t } = useI18n();
  // The tool resolver only routes approved Playwright actions to this card.
  // 工具路由器仅将已批准的 Playwright 动作交给此卡片。
  const { call } = props;
  const action = browserToolAction(call.name);
  if (action === null) return null;
  // The result uses the original conversation projection. The transcript owns screenshot previews.
  // 结果使用会话原始投影。截图预览由会话记录负责。
  const output = call.modelOutput;
  // The target names the operation in the heading; complete arguments are available on demand.
  // 目标用于标题中说明操作对象，完整参数可按需展开。
  const target = actionTarget(action, call.input, locale);
  const hasArguments = Object.keys(call.input).length > 0;
  return (
    <ToolFrame {...props} icon={AppWindow} state={call.state} target={target} targetTitle={target} verb={t(actionLabel(action, call.input))}>
      {output || hasArguments ? (
        <>
          {output && (call.state === "failed" ? <ErrorNote text={output.replace(TERMINAL_STYLE, "")} /> : <MarkdownResult text={output} />)}
          {hasArguments ? (
            <details className="vk-browser-tool-arguments vk-model-more">
              <summary>{t("toolCard.generic.input")}</summary>
              <JsonView value={call.input} />
            </details>
          ) : null}
        </>
      ) : null}
    </ToolFrame>
  );
}
