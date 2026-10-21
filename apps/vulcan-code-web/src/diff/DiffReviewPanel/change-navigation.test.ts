import { describe, expect, it } from "vitest";
import { diffChangeStepIndex, diffChangeTargets } from "./change-navigation";
import type { DiffFile, DiffLineKind } from "../types";

function fileWithHunks(...hunks: DiffLineKind[][]): DiffFile {
  return {
    hunks: hunks.map((kinds) => ({ lines: kinds.map((kind) => ({ kind, content: "", oldLine: null, newLine: null })) })),
  } as unknown as DiffFile;
}

describe("diffChangeTargets", () => {
  it("starts a change at each run of added or removed lines, with metadata kept inside its run", () => {
    const file = fileWithHunks(
      ["context", "removed", "added", "context", "added", "metadata", "added"],
      ["removed", "metadata", "context", "context"],
    );
    expect(diffChangeTargets(file)).toEqual([
      { hunkIndex: 0, lineIndex: 1 },
      { hunkIndex: 0, lineIndex: 4 },
      { hunkIndex: 1, lineIndex: 0 },
    ]);
    expect(diffChangeTargets(fileWithHunks())).toEqual([]);
  });
});

describe("diffChangeStepIndex", () => {
  const tops = [40, 400, 900];

  it("moves by one from the last step while the view has not moved, wrapping at both ends", () => {
    expect(diffChangeStepIndex(tops, 100, { index: 1, scrollTop: 101 }, 1)).toBe(2);
    expect(diffChangeStepIndex(tops, 100, { index: 2, scrollTop: 100 }, 1)).toBe(0);
    expect(diffChangeStepIndex(tops, 100, { index: 0, scrollTop: 100 }, -1)).toBe(2);
  });

  it("resumes from the view once it has scrolled: below its top going down, above it going up", () => {
    expect(diffChangeStepIndex(tops, 0, null, 1)).toBe(0);
    expect(diffChangeStepIndex(tops, 500, { index: 0, scrollTop: 0 }, 1)).toBe(2);
    expect(diffChangeStepIndex(tops, 500, null, -1)).toBe(1);
    expect(diffChangeStepIndex(tops, 1000, null, 1)).toBe(0);
    expect(diffChangeStepIndex(tops, 0, null, -1)).toBe(2);
    expect(diffChangeStepIndex([], 0, null, 1)).toBe(-1);
  });
});
