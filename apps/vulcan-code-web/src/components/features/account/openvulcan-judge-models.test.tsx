// @vitest-environment jsdom
import { act } from "react";
import { createRoot, type Root } from "react-dom/client";
import { afterEach, beforeEach, expect, it, vi } from "vitest";
import { loadOpenVulcanEdgeUsage } from "@/service/openvulcan-account-service";
import { OpenVulcanJudgeModels } from "./openvulcan-judge-models";

vi.mock("@/i18n", () => ({
  useI18n: () => ({ locale: "en-US", t: (key: string) => key }),
  formatMessage: (template: string, values: Record<string, string>) => `${template}:${Object.values(values).join(",")}`,
}));
vi.mock("@/service/openvulcan-account-service", () => ({ loadOpenVulcanEdgeUsage: vi.fn() }));

// host and root mount the actual judge model table with a mocked host boundary.
// host 与 root 挂载真实的裁定模型表，并模拟宿主边界。
let host: HTMLDivElement;
let root: Root;

beforeEach(() => {
  vi.stubGlobal("IS_REACT_ACT_ENVIRONMENT", true);
  vi.resetAllMocks();
  host = document.createElement("div");
  document.body.append(host);
  root = createRoot(host);
});
afterEach(async () => {
  await act(async () => root.unmount());
  host.remove();
  vi.unstubAllGlobals();
});

// rowCells returns each body row's cells' own text, excluding nested failure notes.
// rowCells 返回每个表体行中单元格自身的文本，不含嵌套的失败说明。
function rowCells(): string[][] {
  return Array.from(host.querySelectorAll("tbody tr"), (row) => (
    Array.from(row.querySelectorAll("td"), (cell) => cell.firstChild?.textContent ?? "")
  ));
}

// Every model is typed as a tool judging model; a retired model keeps its calls and failures show under the count.
// 每个模型的类型都是工具裁定模型；已停用的模型保留其调用，失败数显示在计数下方。
it("lists the judge models with their type, state and the account's calls", async () => {
  vi.mocked(loadOpenVulcanEdgeUsage).mockResolvedValueOnce({
    hours: 720,
    models: [
      { model_id: "judge-mini", enabled: true, requests: 1200, items: 4800, errors: 3, last_hour: "2026-10-24T08:00:00Z" },
      { model_id: "judge-old", enabled: false, requests: 2, items: 2, errors: 0, last_hour: "2026-10-01T00:00:00Z" },
      { model_id: "judge-unused", enabled: true, requests: 0, items: 0, errors: 0 },
    ],
  });
  await act(async () => root.render(<OpenVulcanJudgeModels />));
  expect(loadOpenVulcanEdgeUsage).toHaveBeenCalledOnce();
  const rows = rowCells();
  expect(rows.map((cells) => cells[0])).toEqual(["judge-mini", "judge-old", "judge-unused"]);
  expect(rows.every((cells) => cells[1] === "openvulcan.judgeModels.kind")).toBe(true);
  expect(rows.map((cells) => cells[2])).toEqual([
    "openvulcan.judgeModels.enabled", "openvulcan.judgeModels.retired", "openvulcan.judgeModels.enabled",
  ]);
  expect(rows[0].slice(3, 5)).toEqual(["1,200", "4,800"]);
  expect(host.querySelectorAll("tbody small")).toHaveLength(1);
  expect(rows[2][5]).toBe("—");
  expect(host.querySelector(".openvulcan-usage-header > span")?.textContent).toBe("openvulcan.judgeModels.windowDays:30");
});

// A failed read shows the error with a working refresh, and an empty deployment shows the empty state.
// 读取失败时显示错误并可刷新；部署没有模型时显示空状态。
it("reports a failed read and shows the empty state after a refresh", async () => {
  vi.mocked(loadOpenVulcanEdgeUsage)
    .mockRejectedValueOnce(new Error("OpenVulcan refused the edge usage with HTTP 503 `edge_unavailable`."))
    .mockResolvedValueOnce({ hours: 720, models: [] });
  await act(async () => root.render(<OpenVulcanJudgeModels />));
  expect(host.textContent).toContain("openvulcan.judgeModels.failed");
  expect(host.querySelector("table")).toBeNull();
  await act(async () => host.querySelector<HTMLButtonElement>(".openvulcan-usage-header > button")!.click());
  expect(loadOpenVulcanEdgeUsage).toHaveBeenCalledTimes(2);
  expect(host.textContent).not.toContain("openvulcan.judgeModels.failed");
  expect(host.textContent).toContain("openvulcan.judgeModels.empty");
});
