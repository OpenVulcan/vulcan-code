import { describe, expect, it } from "vitest";
import { formatMessage, translateActive } from "@/i18n";

import { buildV1ChangeSetFilePresentation } from "./v1-change-set-presentation";
import type { PersistentChangeSet } from "@/service/conversation-event-window-store";

/** Builds the regression ChangeSet for +140/-208 and persisted format metadata.
 * 构建覆盖 +140/-208 与持久化格式元数据的回归 ChangeSet。 */
function lifecycleChangeSet(): PersistentChangeSet {
  return {
    mode: "applied",
    summary: "Lifecycle regression",
    diagnostics: [{ level: "warning", message: "retained snippets" }],
    files: [
      {
        change: "create",
        path: "README.new.md",
        hunks: [],
        lifecycle_content: {
          mode: "full",
          content: Array.from({ length: 140 }, (_value, index) => `created ${index + 1}`).join("\n"),
          total_line_count: 140,
        },
        after_text_format: { encoding: "utf-8", line_ending: "LF" },
      },
      {
        change: "delete",
        path: "README.old.md",
        hunks: [],
        lifecycle_content: {
          mode: "truncated",
          head: "deleted 1\ndeleted 2",
          tail: "deleted 207\ndeleted 208",
          total_line_count: 208,
        },
        before_text_format: { encoding: "utf-8", line_ending: "LF" },
      },
    ],
  };
}
describe("V1 ChangeSet file presentation", () => {
  it("keeps +140/-208 totals consistent with retained and explicitly omitted rows", () => {
    const [created, deleted] = lifecycleChangeSet().files.map(buildV1ChangeSetFilePresentation);

    expect([created?.additions, deleted?.deletions]).toEqual([140, 208]);
    expect(created?.rows.map((row) => row.content)).toContain("created 1");
    expect(created?.rows).toHaveLength(140);
    expect(created?.rows.find((row) => row.kind === "omitted")).toBeUndefined();
    expect(deleted?.rows.map((row) => row.content)).toEqual([
      "deleted 1",
      "deleted 2",
      formatMessage(translateActive("toolDomain.changeSet.omittedLines"), { count: 204 }),
      "deleted 207",
      "deleted 208",
    ]);
    expect(deleted?.rows.reduce((total, row) => total + row.logicalLineCount, 0)).toBe(208);
    expect(deleted?.beforeFormat).toBe("utf-8 · LF");
  });

  it("renders modify hunks in producer order with real deleted and inserted lines", () => {
    const file = buildV1ChangeSetFilePresentation({
      change: "modify",
      path: "src/main.ts",
      hunks: [{
        before: "before context",
        after: "after context",
        deleted: [{ line: 10, content: "const oldValue = 1;" }],
        inserted: [{ line: 10, content: "const newValue = 2;" }],
      }],
      before_text_format: { encoding: "utf-8", line_ending: "LF" },
      after_text_format: { encoding: "utf-8", line_ending: "LF" },
    });

    expect(file.rows.map((row) => [row.kind, row.lineNumber, row.content])).toEqual([
      ["context", null, "before context"],
      ["deleted", 10, "const oldValue = 1;"],
      ["added", 10, "const newValue = 2;"],
      ["context", null, "after context"],
    ]);
    expect([file.additions, file.deletions]).toEqual([1, 1]);
    expect([file.beforeFormat, file.afterFormat]).toEqual(["utf-8 · LF", "utf-8 · LF"]);
  });

  it("keeps both rename paths and lets rename hunks own content changes", () => {
    const file = buildV1ChangeSetFilePresentation({
      change: "rename",
      old_path: "docs/old-name.md",
      new_path: "docs/new-name.md",
      hunks: [{
        before: "unchanged before",
        after: "unchanged after",
        deleted: [{ line: 4, content: "old heading" }],
        inserted: [{ line: 4, content: "new heading" }],
      }],
      lifecycle_content: {
        mode: "full",
        content: "old heading\n",
        total_line_count: 1,
      },
      before_text_format: { encoding: "utf-8", line_ending: "CRLF" },
      after_text_format: { encoding: "utf-8", line_ending: "LF" },
    });

    expect(file).toMatchObject({
      path: "docs/new-name.md",
      oldPath: "docs/old-name.md",
      newPath: "docs/new-name.md",
      displayPath: "docs/old-name.md → docs/new-name.md",
      additions: 1,
      deletions: 1,
      beforeFormat: "utf-8 · CRLF",
      afterFormat: "utf-8 · LF",
    });
    expect(file.rows.map((row) => [row.kind, row.content])).toEqual([
      ["context", "unchanged before"],
      ["deleted", "old heading"],
      ["added", "new heading"],
      ["context", "unchanged after"],
    ]);
  });

  it("renders pure rename lifecycle text without inventing line changes", () => {
    const file = buildV1ChangeSetFilePresentation({
      change: "rename",
      old_path: "src/old.ts",
      new_path: "src/new.ts",
      hunks: [],
      lifecycle_content: {
        mode: "full",
        content: "export const value = 1;\n",
        total_line_count: 1,
      },
    });

    expect(file.displayPath).toBe("src/old.ts → src/new.ts");
    expect([file.additions, file.deletions]).toEqual([0, 0]);
    expect(file.rows).toEqual([{
      kind: "context",
      lineNumber: 1,
      content: "export const value = 1;",
      logicalLineCount: 0,
    }]);
  });

  it("represents omitted lifecycle bodies explicitly without inventing file text", () => {
    const file = buildV1ChangeSetFilePresentation({
      change: "delete",
      path: "binary.dat",
      hunks: [],
      lifecycle_content: { mode: "omitted", total_line_count: 24 },
    });

    expect(file.rows).toEqual([{
      kind: "omitted",
      lineNumber: null,
      content: translateActive("toolDomain.changeSet.omittedByProducer"),
      logicalLineCount: 24,
    }]);
    expect(file.deletions).toBe(24);
  });
});
