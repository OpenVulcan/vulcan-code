import { describe, expect, it } from "vitest";
import { fileViewerKind } from "./file-viewer-kind";
import { sheetColumnName } from "./sheet-document-viewer";

describe("fileViewerKind", () => {
  it("picks the viewer from the extension, ignoring case", () => {
    expect(fileViewerKind("D:\\docs\\Report.PDF")).toBe("pdf");
    expect(fileViewerKind("/home/u/notes.docx")).toBe("docx");
    expect(fileViewerKind("/home/u/budget.xlsx")).toBe("sheet");
    expect(fileViewerKind("C:/data/legacy.xls")).toBe("sheet");
    expect(fileViewerKind("/tmp/shot.png")).toBe("image");
    expect(fileViewerKind("/tmp/clip.webm")).toBe("video");
  });

  it("returns null for files no viewer reads", () => {
    expect(fileViewerKind("/home/u/slides.pptx")).toBeNull();
    expect(fileViewerKind("/home/u/old.doc")).toBeNull();
    expect(fileViewerKind("/home/u/Makefile")).toBeNull();
    expect(fileViewerKind("/home/u/constructor")).toBeNull();
    expect(fileViewerKind("/home/u/x.toString")).toBeNull();
  });
});

describe("sheetColumnName", () => {
  it("names columns with spreadsheet letters", () => {
    expect(sheetColumnName(0)).toBe("A");
    expect(sheetColumnName(25)).toBe("Z");
    expect(sheetColumnName(26)).toBe("AA");
    expect(sheetColumnName(701)).toBe("ZZ");
    expect(sheetColumnName(702)).toBe("AAA");
  });
});
