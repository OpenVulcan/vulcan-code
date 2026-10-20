import * as React from "react";
import { cx } from "../behavior/core";

// Table renders a data table inside a horizontally scrolling container.
// Table 在可横向滚动的容器中渲染数据表格。
export function Table({ className, ...props }: React.ComponentProps<"table">) {
  return (
    <div data-slot="table-container" className="vk-table-wrap">
      <table data-slot="table" className={cx("vk-table", className)} {...props} />
    </div>
  );
}

// TableHeader is the table head section.
// TableHeader 是表头区域。
export function TableHeader(props: React.ComponentProps<"thead">) {
  return <thead data-slot="table-header" {...props} />;
}

// TableBody is the table body section.
// TableBody 是表体区域。
export function TableBody(props: React.ComponentProps<"tbody">) {
  return <tbody data-slot="table-body" {...props} />;
}

// TableFooter is the table foot section.
// TableFooter 是表尾区域。
export function TableFooter({ className, ...props }: React.ComponentProps<"tfoot">) {
  return <tfoot data-slot="table-footer" className={cx("vk-table-foot", className)} {...props} />;
}

// TableRow is one row; data-state="selected" or aria-selected highlights it.
// TableRow 是一行；data-state="selected" 或 aria-selected 会高亮该行。
export function TableRow(props: React.ComponentProps<"tr">) {
  return <tr data-slot="table-row" {...props} />;
}

// TableHead is a column header cell.
// TableHead 是列标题单元格。
export function TableHead(props: React.ComponentProps<"th">) {
  return <th data-slot="table-head" {...props} />;
}

// TableCell is a body cell.
// TableCell 是表体单元格。
export function TableCell(props: React.ComponentProps<"td">) {
  return <td data-slot="table-cell" {...props} />;
}

// TableCaption describes the table below it.
// TableCaption 在表格下方说明表格。
export function TableCaption({ className, ...props }: React.ComponentProps<"caption">) {
  return <caption data-slot="table-caption" className={cx("vk-table-caption", className)} {...props} />;
}
