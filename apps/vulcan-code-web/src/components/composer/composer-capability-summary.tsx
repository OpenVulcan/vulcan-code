import * as React from "react";
import { AlertTriangle, ChevronRight, Settings2 } from "lucide-react";
import { ProviderToolCapabilityIcon } from "@/components/shared/provider-tool-capability-icon";
import { useI18n } from "@/i18n";
import { cn } from "@/lib/utils";
import type { PublicAiProviderToolCapabilityKind } from "@/types";

// ComposerCapabilitySummaryItem is one resolved capability row shown by the hover summary.
// ComposerCapabilitySummaryItem 是悬浮摘要中展示的一条已解析能力行。
export interface ComposerCapabilitySummaryItem {
  // Closed provider capability identity.
  // 封闭的供应商能力标识。
  capability: PublicAiProviderToolCapabilityKind;
  // Localized capability label.
  // 本地化能力名称。
  label: string;
  // Existing capability color token expression.
  // 既有的能力颜色令牌表达式。
  color: string;
  // Whether the session currently enables this capability.
  // 当前会话是否启用该能力。
  enabled: boolean;
  // Whether a stale or missing selection blocks this capability.
  // 过期或缺失的选择是否阻塞该能力。
  conflict: boolean;
  // Selected model or target count, when the session has one.
  // 会话已有选择时的所选模型或目标数量。
  detail: string | null;
}

// ComposerCapabilitySummary renders the read-only hover summary for the composer capability trigger.
// ComposerCapabilitySummary 渲染输入区能力触发器的只读悬浮摘要。
// Parameters: props supplies load status and resolved rows; returns the card body without owning any session state.
// 参数：props 提供加载状态与已解析行；返回卡片主体，不持有任何会话状态。
export function ComposerCapabilitySummary(props: {
  // Load state of the authoritative settings read.
  // 权威设置读取的加载状态。
  status: "loading" | "ready" | "failed";
  // Resolved capability rows in display order.
  // 按展示顺序排列的已解析能力行。
  items: ComposerCapabilitySummaryItem[];
  // Opens the capability editor dialog from the footer action.
  // 从底部操作打开能力编辑对话框。
  onConfigure: () => void;
}) {
  const { t } = useI18n();
  const enabledItems = props.items.filter((item) => item.enabled || item.conflict);
  const disabledItems = props.items.filter((item) => !item.enabled && !item.conflict);
  const enabledCount = props.items.filter((item) => item.enabled).length;
  const chipList = React.useRef<HTMLUListElement>(null);
  // chipColumns is 3 when every disabled label fits a third of the card, otherwise 2; cells stay equal either way.
  // chipColumns 在所有未启用名称都能放入三分之一卡片宽时为 3，否则为 2；两种情况下单元格都等宽。
  const [chipColumns, setChipColumns] = React.useState(3);
  const disabledSignature = disabledItems.map((item) => item.label).join("\n");
  React.useLayoutEffect(() => {
    const list = chipList.current;
    if (!list) return;
    const measure = () => setChipColumns(capabilityChipColumns(list));
    measure();
    if (typeof ResizeObserver === "undefined") return;
    const observer = new ResizeObserver(measure);
    observer.observe(list);
    return () => observer.disconnect();
  }, [disabledSignature]);
  const enabledRatio = props.items.length ? enabledCount / props.items.length : 0;
  return (
    <div className="composer-capability-summary">
      <div className="composer-capability-summary-header">
        <div className="context-usage-card-header">
          <strong>{t("composer.capability.extended")}</strong>
          {props.status === "ready" ? (
            <span>
              {t("composer.capability.enabledCount")
                .replace("{enabled}", String(enabledCount))
                .replace("{total}", String(props.items.length))}
            </span>
          ) : null}
        </div>
        {props.status === "ready" ? (
          <div aria-hidden="true" className="composer-capability-summary-meter">
            <span style={{ "--capability-enabled-ratio": enabledRatio } as React.CSSProperties} />
          </div>
        ) : null}
      </div>
      {props.status !== "ready" ? (
        <p className={cn("composer-capability-summary-empty", props.status === "failed" && "is-warning")}>
          {t(props.status === "failed" ? "composer.capability.loadFailed" : "composer.capability.loading")}
        </p>
      ) : (
        <>
          {enabledItems.length > 0 ? (
            <ul className="composer-capability-summary-list">
              {enabledItems.map((item) => (
                <li className={cn("composer-capability-summary-row composer-capability-marquee-host", item.conflict && "is-conflict")} key={item.capability}>
                  <CapabilitySummaryIcon item={item} tile />
                  <span className="composer-capability-summary-text">
                    <span className="composer-capability-summary-label"><OverflowMarquee text={item.label} /></span>
                    {item.conflict || item.detail ? (
                      <span className="composer-capability-summary-detail">
                        {item.conflict ? <AlertTriangle aria-hidden="true" size={11} /> : null}
                        <OverflowMarquee text={item.conflict ? t("composer.capability.conflict.tooltip") : item.detail ?? ""} />
                      </span>
                    ) : null}
                  </span>
                </li>
              ))}
            </ul>
          ) : (
            <p className="composer-capability-summary-empty">{t("composer.capability.tooltipEmpty")}</p>
          )}
          {disabledItems.length > 0 ? (
            <div className="composer-capability-summary-section">
              <div className="composer-capability-summary-section-title">{t("settings.providerTools.disabled")}</div>
              <ul
                className="composer-capability-summary-chips"
                ref={chipList}
                style={{ "--capability-chip-columns": chipColumns } as React.CSSProperties}
              >
                {disabledItems.map((item) => (
                  <li className="composer-capability-summary-chip composer-capability-marquee-host" key={item.capability}>
                    <CapabilitySummaryIcon item={item} />
                    <OverflowMarquee text={item.label} />
                  </li>
                ))}
              </ul>
            </div>
          ) : null}
        </>
      )}
      <div className="composer-capability-summary-footer">
        <button className="composer-capability-summary-action" onClick={props.onConfigure} type="button">
          <Settings2 aria-hidden="true" size={13} />
          <span>{t("composer.capability.tooltipTitle")}</span>
          <ChevronRight aria-hidden="true" className="composer-capability-summary-action-arrow" size={13} />
        </button>
      </div>
    </div>
  );
}

// capabilityChipColumns picks 3 equal columns when the widest natural label fits one, otherwise 2.
// capabilityChipColumns 在最宽的自然名称能放入单元格时选择 3 个等宽列，否则选择 2 列。
// Parameters: list is the rendered chip grid; returns the column count. Natural label widths do not depend on the
// current column count, so the decision is stable and cannot oscillate.
// 参数：list 是已渲染的标签网格；返回列数。自然名称宽度与当前列数无关，因此决策稳定，不会来回切换。
function capabilityChipColumns(list: HTMLUListElement): number {
  const chips = Array.from(list.querySelectorAll<HTMLElement>(".composer-capability-summary-chip"));
  const first = chips[0];
  if (!first) return 3;
  const listGap = parseFloat(getComputedStyle(list).columnGap) || 0;
  const chipStyle = getComputedStyle(first);
  const chipChrome = (parseFloat(chipStyle.paddingLeft) || 0) + (parseFloat(chipStyle.paddingRight) || 0)
    + (parseFloat(chipStyle.columnGap) || 0) + ((first.firstElementChild as HTMLElement | null)?.offsetWidth ?? 0);
  const widestLabel = Math.max(...chips.map((chip) => (
    chip.querySelector<HTMLElement>(".composer-capability-marquee > span")?.scrollWidth ?? 0
  )));
  const thirdCapacity = (list.clientWidth - listGap * 2) / 3 - chipChrome;
  return widestLabel <= thirdCapacity ? 3 : 2;
}

// OverflowMarquee shows one single-line text that ends in an ellipsis at rest and scrolls only while its host is hovered.
// OverflowMarquee 展示单行文本：静止时以省略号结尾，仅在外层 host 悬停时滚动。
// Parameters: text is the full value; returns a frame whose measured overflow drives the hover scroll. The scroll moves
// the text with margin instead of a composited transform, and no mask is applied, so glyphs stay pixel-crisp.
// 参数：text 是完整值；返回一个容器，其实测溢出量决定悬停滚动距离。滚动通过 margin 而不是合成层 transform
// 移动文本，也不使用遮罩，因此字形保持像素清晰。
function OverflowMarquee(props: { text: string }) {
  const frame = React.useRef<HTMLSpanElement>(null);
  const content = React.useRef<HTMLSpanElement>(null);
  // shift is the overflow distance in pixels; zero keeps the text static.
  // shift 是以像素计的溢出距离；为零时文本保持静止。
  const [shift, setShift] = React.useState(0);
  React.useLayoutEffect(() => {
    const frameElement = frame.current;
    const contentElement = content.current;
    if (!frameElement || !contentElement) return;
    const measure = () => setShift(Math.max(0, Math.ceil(contentElement.scrollWidth - frameElement.clientWidth)));
    measure();
    if (typeof ResizeObserver === "undefined") return;
    const observer = new ResizeObserver(measure);
    observer.observe(frameElement);
    return () => observer.disconnect();
  }, [props.text]);
  return (
    <span
      className="composer-capability-marquee"
      data-overflow={shift > 0 ? "true" : undefined}
      ref={frame}
      style={{
        "--marquee-shift": `${shift}px`,
        // Constant reading speed: pauses at both ends plus travel time proportional to the overflow.
        // 恒定阅读速度：两端停顿加上与溢出量成正比的移动时间。
        "--marquee-duration": `${(2.4 + shift / 18).toFixed(2)}s`,
      } as React.CSSProperties}
      title={shift > 0 ? props.text : undefined}
    >
      <span ref={content}>{props.text}</span>
    </span>
  );
}

// CapabilitySummaryIcon renders one capability glyph in its color, or muted when the capability is off.
// CapabilitySummaryIcon 以能力颜色渲染图标；能力关闭时以弱化色渲染。
// Parameters: tile wraps the glyph in a tinted square used by enabled rows.
// 参数：tile 将图标包在已启用行使用的着色方块中。
function CapabilitySummaryIcon(props: { item: ComposerCapabilitySummaryItem; tile?: boolean }) {
  return (
    <span
      aria-hidden="true"
      className={cn("composer-capability-colored-icon", props.item.enabled ? "enabled" : "disabled", props.tile && "tile")}
      style={{ "--capability-color": props.item.color } as React.CSSProperties}
    >
      <ProviderToolCapabilityIcon capability={props.item.capability} size={props.tile ? 14 : 13} />
    </span>
  );
}
