import * as React from "react";
import { InlineDisclosure } from "@/components/ui/disclosure";
import { useI18n } from "@/i18n";

// MaintenanceGuide answers, in plain words, when a maintenance tool is needed, how it is used and what it brings.
// MaintenanceGuide 用平实语言回答维护工具何时需要、如何使用以及带来什么。
export interface MaintenanceGuide {
  // when describes the symptom or situation that makes this tool relevant.
  // when 描述使该工具有意义的症状或情境。
  when: string;
  // how describes the steps a user takes inside this section.
  // how 描述用户在此区块中的操作步骤。
  how: string;
  // effect describes the outcome and any cost of using it.
  // effect 描述使用后的结果及代价。
  effect: string;
}

// MaintenanceSectionProps describes one titled maintenance block with its explanation.
// MaintenanceSectionProps 描述一个带标题及说明的维护区块。
interface MaintenanceSectionProps {
  // title names the concern, summary states its purpose in one sentence.
  // title 命名关注点，summary 用一句话说明用途。
  title: string;
  summary: string;
  // guide is the when/how/effect explanation; compact layouts fold it behind a quiet link.
  // guide 是何时／如何／效果说明；紧凑布局将其收进低调链接。
  guide?: MaintenanceGuide;
  // compact folds the guide for narrow surfaces such as the plugin drawer.
  // compact 在插件抽屉等窄界面中收起说明。
  compact?: boolean;
  // actions are section-level controls such as re-checking, shown beside the heading.
  // actions 是区块级控件（例如重新检查），显示在标题旁。
  actions?: React.ReactNode;
  // anchor marks the block for in-drawer navigation.
  // anchor 标记区块以供抽屉内导航。
  anchor?: string;
  children: React.ReactNode;
}

// MaintenanceGuideList renders the three guide answers as a labelled definition list.
// MaintenanceGuideList 以带标签的定义列表渲染三项说明。
function MaintenanceGuideList(props: { guide: MaintenanceGuide }) {
  const { t } = useI18n();
  return (
    <dl className="plugin-maintenance-guide">
      <div><dt>{t("pluginMaintenance.guide.when")}</dt><dd>{props.guide.when}</dd></div>
      <div><dt>{t("pluginMaintenance.guide.how")}</dt><dd>{props.guide.how}</dd></div>
      <div><dt>{t("pluginMaintenance.guide.effect")}</dt><dd>{props.guide.effect}</dd></div>
    </dl>
  );
}

// MaintenanceSection renders a maintenance block: heading, one-line purpose, explanation, then content.
// MaintenanceSection 渲染维护区块：标题、一句话用途、说明，然后是内容。
// Returns: a labelled section; the guide is always reachable, folded only in compact layouts.
// 返回值：带标签的区块；说明始终可达，仅在紧凑布局中收起。
export function MaintenanceSection(props: MaintenanceSectionProps) {
  const { t } = useI18n();
  const headingId = React.useId();
  return (
    <section aria-labelledby={headingId} className="plugin-maintenance-block" data-anchor={props.anchor} data-compact={props.compact ? "true" : undefined}>
      <header className="plugin-maintenance-block-header">
        <div className="plugin-maintenance-block-heading">
          <h3 id={headingId}>{props.title}</h3>
          <p>{props.summary}</p>
        </div>
        {props.actions ? <div className="plugin-maintenance-block-actions">{props.actions}</div> : null}
      </header>
      {props.guide && !props.compact ? <MaintenanceGuideList guide={props.guide} /> : null}
      {props.guide && props.compact ? <InlineDisclosure className="plugin-maintenance-guide-toggle" label={t("pluginMaintenance.guide.toggle")}>
        <MaintenanceGuideList guide={props.guide} />
      </InlineDisclosure> : null}
      <div className="plugin-maintenance-block-body">{props.children}</div>
    </section>
  );
}

// MaintenanceItemProps describes one actionable finding, such as an interrupted operation.
// MaintenanceItemProps 描述一个可操作的发现项，例如中断的操作。
interface MaintenanceItemProps {
  // icon is a decorative glyph matching the operation kind.
  // icon 是与操作类型对应的装饰图标。
  icon: React.ReactNode;
  // title states what happened in user terms; description explains the current state and the way out.
  // title 以用户语言说明发生了什么；description 说明当前状态与解决方式。
  title: React.ReactNode;
  description: React.ReactNode;
  // label is the accessible name, kept stable for assistive technology and tests.
  // label 是无障碍名称，为辅助技术与测试保持稳定。
  label: string;
  // tone colours the icon: attention for unresolved work, neutral for in-progress work.
  // tone 为图标着色：attention 表示未解决工作，neutral 表示进行中工作。
  tone?: "attention" | "neutral";
  actions?: React.ReactNode;
  children?: React.ReactNode;
}

// MaintenanceItem renders one finding row with its explanation on the left and actions on the right.
// MaintenanceItem 渲染一个发现项行，左侧为说明，右侧为操作。
export function MaintenanceItem(props: MaintenanceItemProps) {
  return (
    <article aria-label={props.label} className="plugin-maintenance-item" data-tone={props.tone ?? "attention"}>
      <span aria-hidden="true" className="plugin-maintenance-item-icon">{props.icon}</span>
      <div className="plugin-maintenance-item-main">
        <p className="plugin-maintenance-item-title">{props.title}</p>
        <p className="plugin-maintenance-item-description">{props.description}</p>
        {props.children}
      </div>
      {props.actions ? <div className="plugin-maintenance-item-actions">{props.actions}</div> : null}
    </article>
  );
}

// TechnicalDetails folds raw identifiers behind a quiet disclosure; they stay in the DOM for search and copying.
// TechnicalDetails 将原始标识收进低调的展开项；内容保留在 DOM 中以便查找与复制。
// Parameters: entries are label/value pairs in display order; extra appends free-form evidence.
// 参数：entries 是按显示顺序排列的标签／值对；extra 追加自由格式证据。
export function TechnicalDetails(props: { entries: ReadonlyArray<readonly [string, React.ReactNode]>; extra?: React.ReactNode }) {
  const { t } = useI18n();
  return (
    <InlineDisclosure className="plugin-maintenance-technical" keepMounted label={t("pluginMaintenance.technical")}>
      <dl className="plugin-maintenance-technical-list">
        {props.entries.map(([label, value]) => <div key={label}><dt>{label}</dt><dd>{value}</dd></div>)}
      </dl>
      {props.extra}
    </InlineDisclosure>
  );
}

// MaintenanceNotice renders a short status message; tone selects the semantic colour and live-region role.
// MaintenanceNotice 渲染简短状态消息；tone 选择语义颜色与实时区域角色。
export function MaintenanceNotice(props: { tone: "warning" | "error" | "success" | "info"; children: React.ReactNode; actions?: React.ReactNode }) {
  return (
    <div className="plugin-maintenance-notice" data-tone={props.tone} role={props.tone === "error" ? "alert" : "status"}>
      <div className="plugin-maintenance-notice-text">{props.children}</div>
      {props.actions ? <div className="plugin-maintenance-notice-actions">{props.actions}</div> : null}
    </div>
  );
}

// RecoveryReport is what one recovery reader tells its container: still loading, and how many rows it shows.
// RecoveryReport 是单个恢复读取器告知容器的信息：是否仍在加载，以及显示多少行。
export interface RecoveryReport {
  // loading is true until the first read of this reader settles.
  // loading 在此读取器首次读取完成前为真。
  loading: boolean;
  // visible counts rendered rows and notices, so the container can show an all-clear state.
  // visible 统计已渲染的行与提示，使容器可显示一切正常状态。
  visible: number;
}

// useRecoveryReport forwards a reader's report only when it changes, keeping parents free of render loops.
// useRecoveryReport 仅在报告变化时转发，避免父级渲染循环。
export function useRecoveryReport(onReport: ((report: RecoveryReport) => void) | undefined, loading: boolean, visible: number): void {
  const callback = React.useRef(onReport);
  React.useEffect(() => { callback.current = onReport; });
  React.useEffect(() => { callback.current?.({ loading, visible }); }, [loading, visible]);
}
