// @vitest-environment jsdom
import { act } from "react";
import { createRoot, type Root } from "react-dom/client";
import { afterEach, beforeEach, expect, it, vi } from "vitest";
import { I18nProvider } from "@/i18n";
import type { EnvironmentValuesSubmitRequest, PendingEnvironmentRequestSnapshot, PendingQuestionSnapshot } from "@/types";
import { AskQuestionDialog } from "./ask-question-dialog";

// host and root isolate the real Ask surface rendering one value request.
// host 和 root 隔离渲染单条取值请求的真实 Ask 面板。
let host: HTMLDivElement;
let root: Root;
beforeEach(() => {
  host = document.createElement("div"); document.body.append(host); root = createRoot(host);
  vi.stubGlobal("IS_REACT_ACT_ENVIRONMENT", true);
});
afterEach(async () => { await act(async () => root.unmount()); host.remove(); vi.unstubAllGlobals(); });

// valueRequest builds a synthetic request for two variables.
// valueRequest 构建索取两个变量的合成请求。
function valueRequest(id: string, environment: Partial<PendingEnvironmentRequestSnapshot> = {}): PendingQuestionSnapshot {
  return {
    id, sessionId: "session-1", sequence: 1,
    questions: [{ question: "fallback", header: "Environment", multiple: false, options: [{ label: "Saved", description: "" }] }],
    environment: {
      purpose: "Deploy the preview",
      projectAvailable: true,
      variables: [
        { name: "VULCAN_ENV_TOKEN", description: "Deploy token", secret: true, scope: "project", definedBySystem: false },
        { name: "PATH_HINT", description: "", secret: false, scope: "session", definedBySystem: true },
      ],
      ...environment,
    },
  };
}

// mount renders the production Ask entry point; returns the callbacks after React commits.
// mount 渲染生产 Ask 入口；在 React 提交后返回回调。
async function mount(snapshot: PendingQuestionSnapshot) {
  const submitEnvironment = vi.fn(async (_request: EnvironmentValuesSubmitRequest) => {});
  const submit = vi.fn(async () => {});
  const reject = vi.fn(async (_id: string) => {});
  await act(async () => root.render(
    <I18nProvider initialLocale="en-US">
      <AskQuestionDialog onReject={reject} onSubmit={submit} onSubmitEnvironment={submitEnvironment} request={snapshot} />
    </I18nProvider>,
  ));
  return { submitEnvironment, submit, reject };
}

// field returns the value input of one requested variable.
// field 返回一个请求变量的值输入框。
function field(name: string): HTMLInputElement {
  const input = host.querySelector<HTMLInputElement>(`[data-environment-variable="${name}"] input`);
  if (!input) throw new Error(`Missing field: ${name}`);
  return input;
}

// type sets an input value the way React observes user typing.
// type 以 React 能观察到的用户输入方式设置输入框的值。
async function type(input: HTMLInputElement, value: string) {
  const setter = Object.getOwnPropertyDescriptor(HTMLInputElement.prototype, "value")!.set!;
  await act(async () => {
    setter.call(input, value);
    input.dispatchEvent(new Event("input", { bubbles: true }));
  });
}

// action finds a control by its stable action marker.
// action 按稳定操作标记查找控件。
function action(name: string): HTMLButtonElement {
  const match = host.querySelector<HTMLButtonElement>(`[data-environment-action="${name}"]`);
  if (!match) throw new Error(`Missing action: ${name}`);
  return match;
}

it("masks every field by default and reveals only the toggled one", async () => {
  await mount(valueRequest("mask"));
  expect(field("VULCAN_ENV_TOKEN").type).toBe("password");
  expect(field("PATH_HINT").type).toBe("password");
  const toggle = host.querySelector<HTMLButtonElement>('[data-environment-variable="PATH_HINT"] [data-reveal-toggle]')!;
  await act(async () => toggle.click());
  expect(field("PATH_HINT").type).toBe("text");
  expect(field("VULCAN_ENV_TOKEN").type).toBe("password");
  await act(async () => toggle.click());
  expect(field("PATH_HINT").type).toBe("password");
  expect(host.querySelector("[data-defined-by-system]")).not.toBeNull();
});

it("submits only filled values with their scopes, never through the question reply", async () => {
  const { submitEnvironment, submit } = await mount(valueRequest("save"));
  expect(action("save").disabled).toBe(true);
  await type(field("VULCAN_ENV_TOKEN"), "s3cret-value");
  expect(action("save").disabled).toBe(false);
  await act(async () => { action("save").click(); action("save").click(); });
  expect(submitEnvironment).toHaveBeenCalledExactlyOnceWith({
    requestId: "save",
    declined: false,
    values: [{ name: "VULCAN_ENV_TOKEN", value: "s3cret-value", scope: "project" }],
  });
  expect(submit).not.toHaveBeenCalled();
});

it("falls back to the session scope when the session has no project", async () => {
  const { submitEnvironment } = await mount(valueRequest("no-project", { projectAvailable: false }));
  await type(field("VULCAN_ENV_TOKEN"), "value");
  await act(async () => action("save").click());
  expect(submitEnvironment.mock.calls[0]?.[0].values).toEqual([{ name: "VULCAN_ENV_TOKEN", value: "value", scope: "session" }]);
});

it("leaves Esc to an open scope menu instead of starting the stop confirmation", async () => {
  await mount(valueRequest("menu"));
  const trigger = host.querySelector<HTMLButtonElement>('[data-environment-variable="PATH_HINT"] [role="combobox"]')!;
  await act(async () => trigger.click());
  expect(trigger.getAttribute("aria-expanded")).toBe("true");
  const panel = host.querySelector('[role="dialog"]')!;
  await act(async () => { panel.dispatchEvent(new KeyboardEvent("keydown", { key: "Escape", bubbles: true, cancelable: true })); });
  expect(host.querySelector('[data-environment-action="confirm-stop"]')).toBeNull();
});

it("declines without values and stops only after confirmation", async () => {
  const declined = await mount(valueRequest("decline"));
  await type(field("PATH_HINT"), "typed but declined");
  await act(async () => action("decline").click());
  expect(declined.submitEnvironment).toHaveBeenCalledExactlyOnceWith({ requestId: "decline", declined: true, values: [] });

  const stopped = await mount(valueRequest("stop"));
  await act(async () => action("stop").click());
  expect(stopped.reject).not.toHaveBeenCalled();
  await act(async () => action("confirm-stop").click());
  expect(stopped.reject).toHaveBeenCalledExactlyOnceWith("stop");
  expect(stopped.submitEnvironment).not.toHaveBeenCalled();
});
