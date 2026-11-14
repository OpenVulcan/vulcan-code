import { describe, expect, it } from "vitest";
import { appendComposerFileReferences, splitComposerFileReferences } from "./composer-file-references";

describe("composer file reference projection", () => {
  it("preserves arbitrary prompt text and exact Windows, Unix and URI paths", () => {
    // The authored text can itself contain the normal-looking heading and a list.
    // 用户原文可以包含同名标题和列表。
    const text = 'Write an example:\n\nReferenced files:\n[\n  "example.chm"\n]';
    // Preserve every path character, including an escaped line break in a Unix filename.
    // 保留路径中的所有字符，包括 Unix 文件名中转义的换行。
    const references = ["E:\\Docs\\新 说明.chm", "/tmp/a\nb.txt", "https://example.test/manual", "\\\\server\\manual"];
    expect(splitComposerFileReferences(appendComposerFileReferences(text, [...references, references[0]])))
      .toEqual({ text, references });
  });

  it.each([
    'Referenced files:\n[\n  "ordinary example"\n]',
    'Sample\n\nReferenced files:\n["D:/docs/guide.chm"]',
    'Referenced files:\n[\n  "D:/docs/guide.chm"\n]\nAnd another line.',
    'Read this line about Referenced files:\n["D:/docs/guide.chm"]',
  ])("keeps noncanonical reference lookalikes verbatim", (text) => {
    expect(splitComposerFileReferences(text)).toEqual({ text, references: [] });
  });
});
