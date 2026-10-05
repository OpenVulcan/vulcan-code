import * as React from "react";
import { PageDetails } from "@/components/page";
import { useI18n } from "@/i18n";

// TechnicalDetails folds raw identifiers behind the shared page disclosure; they stay in the DOM for search and copying.
// TechnicalDetails 将原始标识收进共享页面展开项；内容保留在 DOM 中以便查找与复制。
// Parameters: entries are label/value pairs in display order; extra appends free-form evidence.
// 参数：entries 是按显示顺序排列的标签／值对；extra 追加自由格式证据。
export function TechnicalDetails(props: { entries: ReadonlyArray<readonly [string, React.ReactNode]>; extra?: React.ReactNode }) {
  const { t } = useI18n();
  return (
    <PageDetails items={props.entries.map(([label, value]) => ({ key: label, label, value }))} label={t("pluginMaintenance.technical")}>
      {props.extra}
    </PageDetails>
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
