import { describe, expect, it } from "vitest";
import { splitTerminalPaths, terminalPathFrom } from "./terminal-paths";

const found = (text: string) => splitTerminalPaths(text).filter((piece) => typeof piece !== "string");

describe("splitTerminalPaths", () => {
  it("finds compiler locations in the formats common tools print", () => {
    expect(found("  --> crates/app/src/lib.rs:12:5")).toEqual([{ text: "crates/app/src/lib.rs:12:5", path: "crates/app/src/lib.rs", lines: { start: 12, end: 12 } }]);
    expect(found("src\\app.ts(4,9): error TS2322")).toEqual([{ text: "src\\app.ts(4,9)", path: "src\\app.ts", lines: { start: 4, end: 4 } }]);
    expect(found("D:\\repo\\a\\b.py:7 failed")[0]).toMatchObject({ path: "D:\\repo\\a\\b.py", lines: { start: 7, end: 7 } });
    expect(found("\tmodified:   docs/guide.md")).toEqual([{ text: "docs/guide.md", path: "docs/guide.md", lines: null }]);
    expect(found("see ./src/x.min.js.")).toEqual([{ text: "./src/x.min.js", path: "./src/x.min.js", lines: null }]);
  });

  it("keeps the text around each location and ignores bare names, URLs, versions and internal frames", () => {
    expect(splitTerminalPaths("error in src/a.ts:3 now")).toEqual(["error in ", { text: "src/a.ts:3", path: "src/a.ts", lines: { start: 3, end: 3 } }, " now"]);
    for (const text of ["README.md", "https://x.dev/a.js", "v1.2/3.4", "node:internal/modules/run_main:101:5", "file:///D:/x/a.js:3:4", "src/app"]) {
      expect(found(text)).toEqual([]);
    }
  });
});

describe("terminalPathFrom", () => {
  it("places relative paths under an absolute working directory in its separator style", () => {
    expect(terminalPathFrom("D:\\repo\\apps\\web", "src/a.ts")).toBe("D:\\repo\\apps\\web\\src/a.ts");
    expect(terminalPathFrom("/home/u/repo/", "./lib/b.rs")).toBe("/home/u/repo/lib/b.rs");
  });

  it("leaves absolute paths, and paths without a usable directory, as printed", () => {
    expect(terminalPathFrom("/home/u/repo", "/etc/x.conf")).toBe("/etc/x.conf");
    expect(terminalPathFrom(null, "src/a.ts")).toBe("src/a.ts");
    expect(terminalPathFrom(".", "src/a.ts")).toBe("src/a.ts");
  });
});
