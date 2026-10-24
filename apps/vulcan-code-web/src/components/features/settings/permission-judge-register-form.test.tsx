// @vitest-environment jsdom
import { act } from "react";
import { createRoot, type Root } from "react-dom/client";
import { afterEach, beforeEach, expect, it, vi } from "vitest";
import type { PermissionJudgePackageInspection } from "@/types";

// The translator returns keys, so assertions name controls by catalog key instead of wording.
// 翻译函数返回键名，使断言以目录键而非文案定位控件。
vi.mock("@/i18n", async (importOriginal) => {
  const actual = await importOriginal<typeof import("@/i18n")>();
  const t = (key: string) => key;
  return { ...actual, useI18n: () => ({ locale: "en-US", setLocale: () => undefined, t }) };
});

// Every service call is synthetic; nothing reaches a running service.
// 所有服务调用均为合成；不会访问运行中的服务。
const service = vi.hoisted(() => ({ inspectPermissionJudgePackage: vi.fn() }));
vi.mock("@/service/permission-judge-service", () => service);
vi.mock("@/service/project-actions-service", () => ({ browseProjectDirectories: vi.fn() }));
// The directory browser has its own tests; this stand-in confirms one fixed folder when open.
// 目录浏览器有独立测试；此替身在打开时确认一个固定文件夹。
vi.mock("@/components/project/folder-picker-dialog", () => ({
  ProjectFolderPickerDialog: (props: { open: boolean; onConfirm: (path: string) => Promise<void> }) => props.open
    ? <button data-picker-confirm="" type="button" onClick={() => void props.onConfirm("D:/picked/delivery")}>pick</button>
    : null,
}));

const { PermissionJudgeRegisterForm } = await import("./permission-judge-register-form");

let host: HTMLDivElement;
let root: Root;

beforeEach(() => {
  host = document.createElement("div");
  document.body.append(host);
  root = createRoot(host);
  vi.stubGlobal("IS_REACT_ACT_ENVIRONMENT", true);
});
afterEach(async () => {
  await act(async () => root.unmount());
  host.remove();
  vi.unstubAllGlobals();
  vi.clearAllMocks();
});

// verified builds a successful check; `knownPackageId` decides whether the package is official.
// verified 构建一次成功检查；`knownPackageId` 决定模型包是否官方。
function verified(knownPackageId: string | null, registered = false): PermissionJudgePackageInspection {
  return {
    issue: null,
    package: { modelId: "m", knownPackageId, defaultName: "Pkg", policyVersion: "1", precision: "fp32", totalBytes: 1, registered },
  };
}

async function mount(onRegister = vi.fn(async () => undefined)) {
  await act(async () => root.render(<PermissionJudgeRegisterForm busy={null}
    knownPackages={[{ id: "official", displayName: "Official", variants: [] }]} onRegister={onRegister} />));
  return onRegister;
}

function pathInput(): HTMLInputElement {
  return host.querySelector<HTMLInputElement>('input[aria-label="settings.permissionJudge.registerPath"]')!;
}

function submit(): HTMLButtonElement {
  return Array.from(host.querySelectorAll("button")).find((button) => button.textContent === "settings.permissionJudge.registerSubmit")!;
}

function state(): string | null {
  return host.querySelector("[data-judge-package-state]")?.getAttribute("data-judge-package-state") ?? null;
}

// type sets the folder field the way a user does, then leaves the field.
// type 以用户方式填写文件夹输入框，然后离开输入框。
async function typeAndLeave(value: string) {
  await act(async () => {
    const setter = Object.getOwnPropertyDescriptor(HTMLInputElement.prototype, "value")!.set!;
    setter.call(pathInput(), value);
    pathInput().dispatchEvent(new Event("input", { bubbles: true }));
  });
  await act(async () => { pathInput().dispatchEvent(new FocusEvent("focusout", { bubbles: true })); });
}

// A typed official folder is checked once on leaving the field and then added without a trust choice.
// 手动输入的官方文件夹在离开输入框时检查一次，随后无需信任选择即可添加。
it("checks a typed folder once and adds an official package", async () => {
  service.inspectPermissionJudgePackage.mockResolvedValue(verified("official"));
  const onRegister = await mount();
  expect(submit().disabled).toBe(true);
  await typeAndLeave(" D:/models/delivery ");
  await act(async () => { pathInput().dispatchEvent(new FocusEvent("focusout", { bubbles: true })); });
  expect(service.inspectPermissionJudgePackage).toHaveBeenCalledExactlyOnceWith("D:/models/delivery");
  expect(state()).toBe("official");
  expect(host.querySelector("[data-judge-register-custom]")).toBeNull();
  await act(async () => submit().click());
  expect(onRegister).toHaveBeenCalledExactlyOnceWith({ path: "D:/models/delivery", name: "", custom: false }, expect.any(Function));
});

// A custom package needs the explicit trust choice before it can be added.
// 自定义模型包必须先显式选择信任才能添加。
it("requires trust for a package that is not official", async () => {
  service.inspectPermissionJudgePackage.mockResolvedValue(verified(null));
  const onRegister = await mount();
  await typeAndLeave("D:/models/custom");
  expect(state()).toBe("custom");
  expect(submit().disabled).toBe(true);
  await act(async () => host.querySelector<HTMLButtonElement>("[data-judge-register-custom]")!.click());
  await act(async () => submit().click());
  expect(onRegister).toHaveBeenCalledExactlyOnceWith({ path: "D:/models/custom", name: "", custom: true }, expect.any(Function));
});

// A folder that is not a package, or one already added, cannot be submitted.
// 不是模型包的文件夹或已添加的模型包都不能提交。
it("blocks folders that fail the check or are already added", async () => {
  service.inspectPermissionJudgePackage.mockResolvedValueOnce({ issue: { code: "unreadable", detail: "MANIFEST.json" }, package: null });
  await mount();
  await typeAndLeave("D:/models");
  expect(state()).toBe("issue");
  expect(host.querySelector("[data-judge-package-issue]")?.getAttribute("data-judge-package-issue")).toBe("unreadable");
  expect(submit().disabled).toBe(true);
  service.inspectPermissionJudgePackage.mockResolvedValueOnce(verified("official", true));
  await typeAndLeave("D:/models/delivery");
  expect(state()).toBe("registered");
  expect(submit().disabled).toBe(true);
});

// Editing the folder after a check invalidates that check until the new folder is checked.
// 检查后修改文件夹会使该检查失效，直到新文件夹完成检查。
it("ignores a check that belongs to another folder", async () => {
  service.inspectPermissionJudgePackage.mockResolvedValue(verified("official"));
  await mount();
  await typeAndLeave("D:/models/delivery");
  expect(submit().disabled).toBe(false);
  await act(async () => {
    const setter = Object.getOwnPropertyDescriptor(HTMLInputElement.prototype, "value")!.set!;
    setter.call(pathInput(), "D:/models/other");
    pathInput().dispatchEvent(new Event("input", { bubbles: true }));
  });
  expect(state()).toBeNull();
  expect(submit().disabled).toBe(true);
});

// Picking a folder from the browser fills the field and checks that folder.
// 从浏览器选择文件夹会填入输入框并检查该文件夹。
it("fills and checks the folder picked from the browser", async () => {
  service.inspectPermissionJudgePackage.mockResolvedValue(verified("official"));
  await mount();
  await act(async () => host.querySelector<HTMLButtonElement>("[data-judge-register-browse]")!.click());
  await act(async () => host.querySelector<HTMLButtonElement>("[data-picker-confirm]")!.click());
  expect(pathInput().value).toBe("D:/picked/delivery");
  expect(service.inspectPermissionJudgePackage).toHaveBeenCalledExactlyOnceWith("D:/picked/delivery");
  expect(submit().disabled).toBe(false);
});
