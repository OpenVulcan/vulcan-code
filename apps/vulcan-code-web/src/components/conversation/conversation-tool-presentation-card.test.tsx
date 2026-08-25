import React from "react";
import { renderToStaticMarkup } from "react-dom/server";
import { describe, expect, it } from "vitest";

import type { FileChangePresentation } from "@/components/conversation/conversation-tool-presentations";
import { I18nProvider } from "@/i18n";
import { ToolPresentationCard } from "./conversation-tool-presentation-card";

// LIST_PRESENTATION represents one directory-list result whose target has no file extension.
// LIST_PRESENTATION 表示目标没有文件扩展名的目录列表结果。
const LIST_PRESENTATION: FileChangePresentation = {
  accessAction: "list",
  errorDetail: null,
  files: [{
    added: null,
    diffFile: null,
    displayPath: "D:/openvulcan",
    expandable: false,
    icon: { fallbackName: "openvulcan", iconName: null },
    id: "directory-list-target",
    operation: "mixed",
    operationIds: [],
    path: "D:/openvulcan",
    pathKind: "external",
    previousPath: null,
    readOnly: true,
    removed: null,
    resourceId: null,
    rows: [],
    textFormat: null,
  }],
  id: "directory-list",
  interactive: false,
  kind: "file_change",
  mode: "unknown",
  operation: "mixed",
  source: "native_file",
  state: "completed",
  totalAdded: null,
  totalRemoved: null,
};

describe("ToolPresentationCard file access icons", () => {
  // Directory-list rows already have a FolderTree operation icon and must not render a file-type fallback dot.
  // 目录列表行已经具有 FolderTree 操作图标，不应再渲染文件类型兜底圆点。
  it("hides the file-type icon slot for directory-list targets", () => {
    const markup = renderToStaticMarkup(
      <I18nProvider initialLocale="zh-CN">
        <ToolPresentationCard
          detailLoadEnabled={false}
          expanded={false}
          expandedToolPresentationIds={new Set()}
          presentation={LIST_PRESENTATION}
          requestProgress={null}
          onExpandedChange={() => undefined}
          onLoadToolDetail={async () => null}
        />
      </I18nProvider>,
    );

    expect(markup).toContain("查询文件列表");
    expect(markup).toContain("D:/openvulcan");
    expect(markup).not.toContain("file-change-icon-stack");
    expect(markup).not.toContain("file-change-fallback-icon");
  });

  // Read rows still use their file-type icon because the target is an actual file.
  // 读取行的目标是实际文件，因此仍应使用文件类型图标。
  it("keeps the file-type icon slot for file reads", () => {
    const markup = renderToStaticMarkup(
      <I18nProvider initialLocale="zh-CN">
        <ToolPresentationCard
          detailLoadEnabled={false}
          expanded={false}
          expandedToolPresentationIds={new Set()}
          presentation={{
            ...LIST_PRESENTATION,
            accessAction: "read",
            id: "file-read",
            files: [{
              ...LIST_PRESENTATION.files[0],
              displayPath: "README.md",
              icon: { fallbackName: "README.md", iconName: null },
              id: "file-read-target",
              path: "D:/openvulcan/README.md",
            }],
          }}
          requestProgress={null}
          onExpandedChange={() => undefined}
          onLoadToolDetail={async () => null}
        />
      </I18nProvider>,
    );

    expect(markup).toContain("file-change-icon-stack");
    expect(markup).toContain("file-change-fallback-icon");
  });
});
