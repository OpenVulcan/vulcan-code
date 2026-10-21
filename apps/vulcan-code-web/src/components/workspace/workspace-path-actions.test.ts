import { describe, expect, it } from "vitest";
import { relativeToWorkspace } from "./workspace-path-actions";

describe("relativeToWorkspace", () => {
  it("returns the project-relative path with forward slashes", () => {
    expect(relativeToWorkspace("D:\\repo\\app", "D:\\repo\\app\\src\\main.ts")).toBe("src/main.ts");
    expect(relativeToWorkspace("/home/u/app/", "/home/u/app/src/main.ts")).toBe("src/main.ts");
  });
  it("ignores case only for Windows paths", () => {
    expect(relativeToWorkspace("d:/Repo/App", "D:\\repo\\app\\a.ts")).toBe("a.ts");
    expect(relativeToWorkspace("/home/u/App", "/home/u/app/a.ts")).toBeNull();
  });
  it("rejects paths outside the project, sibling prefixes and a missing project", () => {
    expect(relativeToWorkspace("D:\\repo\\app", "D:\\repo\\app-two\\a.ts")).toBeNull();
    expect(relativeToWorkspace("D:\\repo\\app", "D:\\repo\\app")).toBeNull();
    expect(relativeToWorkspace(null, "D:\\repo\\app\\a.ts")).toBeNull();
  });
});
