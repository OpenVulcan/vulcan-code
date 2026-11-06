import { expect, it } from "vitest";
import { parseUnifiedDiff } from "./parseDiff";

// Hunk bodies may start with `---`/`+++`; only rows beyond the header's counts are metadata.
// hunk 正文可以以 `---`/`+++` 开头；只有超出头部行数的行才是元数据。
it("keeps removed and added rows that start with --- or +++ inside a hunk", () => {
  const snapshot = parseUnifiedDiff(
    [
      "diff --git a/a.lua b/a.lua",
      "--- a/a.lua",
      "+++ b/a.lua",
      "@@ -1,3 +1,3 @@",
      "--- old comment",
      "+++ new heading",
      " keep",
      "----",
      "+---",
      "",
    ].join("\n"),
  );
  const lines = snapshot.files[0].hunks[0].lines.map((line) => [line.kind, line.content, line.oldLine, line.newLine]);
  expect(lines).toEqual([
    ["removed", "-- old comment", 1, null],
    ["added", "++ new heading", null, 1],
    ["context", "keep", 2, 2],
    ["removed", "---", 3, null],
    ["added", "---", null, 3],
  ]);
  expect(snapshot.files[0].additions).toBe(2);
  expect(snapshot.files[0].deletions).toBe(2);
});
