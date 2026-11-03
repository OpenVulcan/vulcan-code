// @vitest-environment jsdom
import { act } from "react";
import { createRoot, type Root } from "react-dom/client";
import { afterEach, beforeEach, expect, it, vi } from "vitest";
import OfficeDocumentViewer from "./office-document-viewer";

vi.mock("@/i18n", () => ({ useI18n: () => ({ t: (key: string) => key }) }));
const service = vi.hoisted(() => ({ loadWorkspaceOfficePdf: vi.fn() }));
vi.mock("@/service/workspace-office-pdf-service", () => service);
vi.mock("@/resources/localMedia", () => ({ localMediaSourceUrl: (path: string) => `asset:${path}` }));
vi.mock("@/components/file-viewers/pdf-document-viewer", () => ({
  default: (props: { url: string }) => <div data-testid="pdf" data-url={props.url} />,
}));
const actions = vi.hoisted(() => ({ localFileActionsAvailable: vi.fn(() => false), requestWorkspaceFileAction: vi.fn() }));
vi.mock("@/components/workspace/workspace-file-actions", () => actions);

let host: HTMLDivElement;
let root: Root;

beforeEach(() => {
  vi.stubGlobal("IS_REACT_ACT_ENVIRONMENT", true);
  host = document.createElement("div");
  document.body.append(host);
  root = createRoot(host);
});

afterEach(async () => {
  await act(async () => root.unmount());
  host.remove();
  vi.unstubAllGlobals();
  vi.clearAllMocks();
});

// render shows the viewer for one deck after the conversion request settles.
// render 在转换请求完成后显示单个演示文稿的查看器。
async function render() {
  await act(async () => root.render(<OfficeDocumentViewer name="deck.pptx" path={"D:\\talks\\deck.pptx"} />));
}

it("shows the converted PDF in the PDF viewer", async () => {
  service.loadWorkspaceOfficePdf.mockResolvedValue({ status: "converted", pdfPath: "C:\\cache\\deck.pdf" });
  await render();
  expect(service.loadWorkspaceOfficePdf).toHaveBeenCalledExactlyOnceWith("D:\\talks\\deck.pptx");
  expect(host.querySelector("[data-testid=pdf]")?.getAttribute("data-url")).toBe("asset:C:\\cache\\deck.pdf");
});

it("asks for LibreOffice and offers the default program on the desktop when no converter is installed", async () => {
  service.loadWorkspaceOfficePdf.mockResolvedValue({ status: "converterMissing" });
  actions.localFileActionsAvailable.mockReturnValue(true);
  await render();
  expect(host.textContent).toContain("documentViewer.converterMissingTitle");
  await act(async () => host.querySelector("button")!.click());
  expect(actions.requestWorkspaceFileAction).toHaveBeenCalledExactlyOnceWith("D:\\talks\\deck.pptx", "open");
});

it("offers no default program on the web client and reports conversion errors", async () => {
  service.loadWorkspaceOfficePdf.mockResolvedValue({ status: "converterMissing" });
  actions.localFileActionsAvailable.mockReturnValue(false);
  await render();
  expect(host.querySelector("button")).toBeNull();
  await act(async () => root.unmount());
  root = createRoot(host);
  service.loadWorkspaceOfficePdf.mockRejectedValue(new Error("LibreOffice could not convert"));
  await render();
  expect(host.querySelector("[role=alert]")?.textContent).toContain("LibreOffice could not convert");
});
