import type { MouseEvent, ReactNode } from "react";
import { Info, ListChecks, ShieldAlert } from "lucide-react";

import {
  Tooltip,
  TooltipContent,
  TooltipTrigger,
} from "@/ui-kit/primitives/tooltip";

/** Presentation contract shared by session and backend model-extension selectors.
 * 会话与后端模型扩展选择器共享的展示契约。 */
export interface ProviderToolExtensionModelCardProps {
  /** Whether this model is selected in the owning settings scope.
   * 当前模型是否已在所属设置范围中选中。 */
  checked: boolean;
  /** Whether the model checkbox is unavailable.
   * 模型勾选框是否不可用。 */
  checkboxDisabled: boolean;
  /** Handles one explicit model-selection change.
   * 处理一次明确的模型选择变化。 */
  onCheckedChange: (checked: boolean) => void;
  /** Human-readable model title.
   * 面向用户的模型标题。 */
  title: string;
  /** Reviewed model behavior summary shown by the information tooltip.
   * 由说明 Tooltip 显示的已审核模型行为说明。 */
  summary: string;
  /** Optional model characteristics appended to the information tooltip.
   * 追加到说明 Tooltip 的可选模型特性。 */
  characteristics: string | null;
  /** Complete model constraints shown by the constraints tooltip.
   * 由约束 Tooltip 显示的完整模型约束。 */
  constraints: string;
  /** Complete model limitations shown by the limitations tooltip.
   * 由限制 Tooltip 显示的完整模型限制。 */
  limitations: string;
  /** Exact provider identifier owning the model.
   * 拥有该模型的精确供应商标识。 */
  providerId: string;
  /** Exact upstream model identifier used by the provider.
   * 供应商使用的精确上游模型标识。 */
  modelId: string;
  /** Account selector supplied by the owning settings surface.
   * 由所属设置界面提供的账号选择器。 */
  accountControl: ReactNode;
  /** Radio group name when the owner allows exactly one model; omitted for a multi-select list.
   * 所属方只允许一个模型时的单选组名；多选列表时省略。 */
  radioGroup?: string;
}

/** Renders one model-extension card with stable, full-width information sections.
 * 使用稳定的整行信息分区渲染一个模型扩展卡片。
 *
 * @param props shared display facts and owner-controlled selection actions.
 *              共享展示事实与由所有者控制的选择操作。
 * @returns one model card used unchanged by both settings surfaces.
 *          一个由两个设置界面原样复用的模型卡片。 */
export function ProviderToolExtensionModelCard(
  props: ProviderToolExtensionModelCardProps,
) {
  /** Toggles the card unless the click belongs to a dedicated interactive control.
   * 仅在点击不属于独立交互控件时切换卡片勾选状态。 */
  const handleCardClick = (event: MouseEvent<HTMLElement>) => {
    if (
      props.checkboxDisabled
      || !(event.target instanceof Element)
      || event.target.closest(
        '[data-extension-card-selection], [data-extension-card-section="account"], button',
      )
    ) {
      return;
    }
    props.onCheckedChange(!props.checked);
  };

  return (
    <article
      aria-label={props.title}
      className="vk-ext-card"
      data-disabled={props.checkboxDisabled ? "true" : undefined}
      onClick={handleCardClick}
    >
      <label data-extension-card-selection>
        {/* The native checkbox owns keyboard selection; the account form retains independent semantics.
            原生复选框持有键盘选择；账号表单保留独立语义。 */}
        <input
          aria-label={props.title}
          checked={props.checked}
          disabled={props.checkboxDisabled}
          className={props.radioGroup ? "vk-radio" : "vk-check"}
          name={props.radioGroup}
          onChange={(event) => props.onCheckedChange(event.currentTarget.checked)}
          type={props.radioGroup ? "radio" : "checkbox"}
        />
      </label>
      <div
        className="vk-ext-card-body"
        data-extension-card-content
      >
        <div
          className="vk-ext-card-title"
          data-extension-card-row="title"
        >
          <strong title={props.title}>{props.title}</strong>
          <span className="vk-ext-card-hints">
            <ProviderToolExtensionProfileTooltip
              accessibleLabel={props.summary}
              icon={<Info aria-hidden="true" size={14} />}
              kind="summary"
            >
              <span>{props.summary}</span>
              {props.characteristics ? <span>{props.characteristics}</span> : null}
            </ProviderToolExtensionProfileTooltip>
            <ProviderToolExtensionProfileTooltip
              accessibleLabel={props.constraints}
              icon={<ListChecks aria-hidden="true" size={14} />}
              kind="constraints"
            >
              <span>{props.constraints}</span>
            </ProviderToolExtensionProfileTooltip>
            <ProviderToolExtensionProfileTooltip
              accessibleLabel={props.limitations}
              icon={<ShieldAlert aria-hidden="true" size={14} />}
              kind="limitations"
            >
              <span>{props.limitations}</span>
            </ProviderToolExtensionProfileTooltip>
          </span>
        </div>
        <div
          className="vk-ext-card-row"
          data-extension-card-row="controls"
        >
          <span
            className="vk-ext-card-id"
            data-extension-card-identity
            title={`${props.providerId}/${props.modelId}`}
          >
            {props.providerId}/{props.modelId}
          </span>
          <div
            className="vk-ext-card-account"
            data-extension-card-section="account"
          >
            {props.accountControl}
          </div>
        </div>
      </div>
    </article>
  );
}

/** Tooltip trigger contract for one reviewed model-profile section.
 * 单个已审核模型档案分区的 Tooltip 触发器契约。 */
interface ProviderToolExtensionProfileTooltipProps {
  /** Full accessible text for the icon-only trigger.
   * 纯图标触发器使用的完整无障碍文本。 */
  accessibleLabel: string;
  /** Tooltip body content.
   * Tooltip 正文内容。 */
  children: ReactNode;
  /** Semantic icon rendered inside the trigger.
   * 触发器内部渲染的语义图标。 */
  icon: ReactNode;
  /** Stable profile section represented by the icon.
   * 图标表示的稳定档案分区。 */
  kind: "summary" | "constraints" | "limitations";
}

/** Renders one icon-only trigger with a bounded, readable profile tooltip.
 * 渲染一个纯图标触发器及宽度受限且可读的档案 Tooltip。
 *
 * @param props accessible trigger facts and tooltip content.
 *              无障碍触发事实与 Tooltip 内容。
 * @returns one profile tooltip placed after the model title.
 *          一个放置在模型标题后的档案 Tooltip。 */
function ProviderToolExtensionProfileTooltip(
  props: ProviderToolExtensionProfileTooltipProps,
) {
  return (
    <Tooltip>
      <TooltipTrigger asChild>
        <button
          aria-label={props.accessibleLabel}
          className="vk-ext-card-hint"
          data-kind={props.kind}
          data-extension-profile-tooltip={props.kind}
          type="button"
        >
          {props.icon}
        </button>
      </TooltipTrigger>
      <TooltipContent
        className="vk-ext-card-tip"
        side="top"
        sideOffset={6}
      >
        {props.children}
      </TooltipContent>
    </Tooltip>
  );
}
