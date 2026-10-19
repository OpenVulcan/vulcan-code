import * as React from "react";
import { X } from "lucide-react";
import { cx } from "../behavior/core";

// PanelTabProps describe one closable tab: a select button and a separate close button inside one frame.
// PanelTabProps 描述一个可关闭页签：同一框内的选择按钮与独立的关闭按钮。
export interface PanelTabProps extends Omit<React.ComponentProps<"div">, "children" | "onSelect" | "title"> {
  // Whether this tab is the selected one.
  // 当前页签是否为选中页签。
  active: boolean;
  // Leading glyph shown before the label.
  // 显示在标签前的图形。
  icon?: React.ReactNode;
  // Visible label; long labels are truncated.
  // 可见标签；过长时截断。
  label: React.ReactNode;
  // Full title shown on hover.
  // 悬停时显示的完整标题。
  title: string;
  // Accessible name of the close button.
  // 关闭按钮的无障碍名称。
  closeLabel: string;
  // Marks the tab as the current drop destination while another tab is dragged over it.
  // 在其他页签拖到其上方时标记为当前放置目标。
  dropTarget?: boolean;
  // Disables both the select and the close button while the tab cannot be acted on.
  // 页签暂不可操作时同时禁用选择与关闭按钮。
  disabled?: boolean;
  // Selects this tab.
  // 选中当前页签。
  onSelect: () => void;
  // Closes this tab.
  // 关闭当前页签。
  onClose: () => void;
  // Extra attributes for the select button, such as data identifiers; a tabIndex here replaces the roving default.
  // 选择按钮的额外属性，例如数据标识；此处的 tabIndex 替换默认的游走焦点值。
  tabProps?: React.ComponentProps<"button"> & Record<`data-${string}`, string | undefined>;
}

// PanelTab renders a closable tab for a `role="tablist"` strip. The select button carries `role="tab"`; the close
// button stays a sibling so the two never nest. Arrow-key movement belongs to the owning tablist.
// PanelTab 为 `role="tablist"` 页签栏渲染可关闭页签。选择按钮承载 `role="tab"`；关闭按钮保持为兄弟节点，
// 两者永不嵌套。方向键移动由所属页签列表负责。
export function PanelTab({
  active,
  icon,
  label,
  title,
  closeLabel,
  dropTarget = false,
  disabled = false,
  onSelect,
  onClose,
  tabProps,
  className,
  ...frameProps
}: PanelTabProps) {
  return (
    <div
      {...frameProps}
      className={cx("vk-panel-tab", className)}
      data-active={active ? "" : undefined}
      data-drop={dropTarget ? "" : undefined}
      role="presentation"
    >
      {icon ? <span aria-hidden className="vk-panel-tab-glyph">{icon}</span> : null}
      <button
        tabIndex={active ? 0 : -1}
        {...tabProps}
        aria-selected={active}
        className="vk-panel-tab-label"
        disabled={disabled}
        onClick={onSelect}
        role="tab"
        title={title}
        type="button"
      >
        {label}
      </button>
      <button
        aria-label={closeLabel}
        className="vk-panel-tab-x"
        disabled={disabled}
        onClick={(event) => {
          event.stopPropagation();
          onClose();
        }}
        type="button"
      >
        <X />
      </button>
    </div>
  );
}
