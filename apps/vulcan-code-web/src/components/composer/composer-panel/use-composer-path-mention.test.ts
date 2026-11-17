import { expect, it } from "vitest";
import type { WorkspaceFileEntrySummary } from "@/types";
import { PATH_MENTION_LIMIT, composerMentionAt, pathMentionAt, pathMentionSuggestions, pathMentionText, splitPathMention } from "./use-composer-path-mention";

it("finds the @ word around the caret only when the @ starts the word", () => {
  expect(pathMentionAt("see @src/ma now", 11)).toEqual({ start: 4, end: 11, query: "src/ma" });
  expect(pathMentionAt("see @src/main.ts", 8)).toEqual({ start: 4, end: 16, query: "src" });
  expect(pathMentionAt("@", 1)).toEqual({ start: 0, end: 1, query: "" });
  expect(pathMentionAt("mail a@b.com", 12)).toBeNull();
  expect(pathMentionAt("@src now", 8)).toBeNull();
});

it("splits the typed folder from the name being typed, with either slash", () => {
  expect(splitPathMention("ma")).toEqual({ directory: "", prefix: "ma" });
  expect(splitPathMention("src/components/Bu")).toEqual({ directory: "src/components", prefix: "Bu" });
  expect(splitPathMention("src\\lib\\")).toEqual({ directory: "src/lib", prefix: "" });
});

it("writes a mention with forward slashes, a slash after a folder and a space after a file", () => {
  expect(pathMentionText("src\\lib", true)).toBe("@src/lib/");
  expect(pathMentionText("src\\main.ts", false)).toBe("@src/main.ts ");
});

// entry builds one listing row with the fields the suggestions read.
// entry 构建一条只含建议所读字段的目录行。
function entry(name: string, isDirectory = false): WorkspaceFileEntrySummary {
  return { name, relativePath: name, isDirectory } as WorkspaceFileEntrySummary;
}

it("offers names starting with the typed text before names that only contain it, folders first", () => {
  const entries = [entry("readme.md"), entry("main.ts"), entry("domain", true), entry("main", true), entry("mainline.ts")];
  expect(pathMentionSuggestions(entries, "MAIN").map((row) => row.name)).toEqual(["main", "main.ts", "mainline.ts", "domain"]);
  expect(pathMentionSuggestions(entries, "").map((row) => row.name)).toEqual(["domain", "main", "main.ts", "mainline.ts", "readme.md"]);
  const many = Array.from({ length: PATH_MENTION_LIMIT + 3 }, (_, index) => entry(`f${index}`));
  expect(pathMentionSuggestions(many, "f")).toHaveLength(PATH_MENTION_LIMIT);
});

// Compound references edit only the plugin after #, and file paths retain the @ trigger.
// 复合引用只编辑 # 后的插件，文件路径保留 @ 触发。
it("keeps skill, plugin, and path references distinct", () => {
  const combined = "use @session-mcp#context";
  expect(composerMentionAt(combined, combined.length)).toEqual({
    start: combined.indexOf("#"), end: combined.length, query: "context", marker: "#",
  });
  expect(composerMentionAt("#", 1)).toEqual({ start: 0, end: 1, query: "", marker: "#" });
  expect(composerMentionAt("@src/file#name", 14)?.marker).toBe("@");
  expect(composerMentionAt("hello@domain.test", 17)).toBeNull();
});
