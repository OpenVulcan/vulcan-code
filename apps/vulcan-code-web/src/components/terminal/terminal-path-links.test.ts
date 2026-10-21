import { describe, expect, it, vi } from "vitest";
import type { IBufferLine } from "@xterm/xterm";
import { terminalPathLinks } from "./terminal-path-links";

// bufferLine builds a buffer line from cells given as [chars, width]; wide characters are followed by a
// zero-width cell, as xterm stores them.
const bufferLine = (cells: [string, number][]) => ({
  length: cells.length,
  getCell: (x: number) => ({ getChars: () => cells[x][0], getWidth: () => cells[x][1] }),
}) as unknown as IBufferLine;

const ascii = (text: string): [string, number][] => [...text].map((char) => [char, 1]);

describe("terminalPathLinks", () => {
  it("covers the cells of each location, including past wide characters", () => {
    const line = bufferLine([["错", 2], ["", 0], ...ascii(" src/a.ts:3 x"), ["", 1]]);
    const links = terminalPathLinks(line, 7, vi.fn());
    expect(links.map((link) => ({ text: link.text, range: link.range }))).toEqual([
      { text: "src/a.ts:3", range: { start: { x: 4, y: 7 }, end: { x: 13, y: 7 } } },
    ]);
  });

  it("opens only on a Ctrl or Cmd click", () => {
    const open = vi.fn();
    const [link] = terminalPathLinks(bufferLine(ascii("lib/b.rs(4,2)")), 1, open);
    link.activate({ ctrlKey: false, metaKey: false } as MouseEvent, link.text);
    expect(open).not.toHaveBeenCalled();
    link.activate({ ctrlKey: true, metaKey: false } as MouseEvent, link.text);
    link.activate({ ctrlKey: false, metaKey: true } as MouseEvent, link.text);
    expect(open.mock.calls).toEqual([["lib/b.rs", { start: 4, end: 4 }], ["lib/b.rs", { start: 4, end: 4 }]]);
  });
});
