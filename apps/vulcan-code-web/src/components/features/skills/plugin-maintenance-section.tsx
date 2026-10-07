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
