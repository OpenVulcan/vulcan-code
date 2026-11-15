// @vitest-environment jsdom
import { act } from "react";
import { createRoot } from "react-dom/client";
import { expect, it, vi } from "vitest";
import { I18nProvider } from "@/i18n";
import { ProjectDirectoryRecovery } from "./project-directory-recovery";

// Mock only the side-effect boundary so React renders the actual recovery controls.
// 仅模拟副作用边界，React 仍渲染真实的恢复控件。
const check = vi.hoisted(() => vi.fn());
vi.mock("@/service/project-actions-service", () => ({
  checkProjectDirectory: check,
  browseProjectReplacement: vi.fn(),
  changeProjectDirectory: vi.fn(),
}));
vi.mock("@/service/webui-service-runtime", () => ({ hasServiceRuntime: () => true }));

// Verify that rechecking a restored path clears the warning without mutating the project.
// 验证重新检查恢复路径会清除警告，且不会更改项目。
it("displays the saved path and clears the unavailable state when the user rechecks", async () => {
  const mount = document.createElement("div");
  const root = createRoot(mount);
  const project = { id: "project", name: "Project", path: "E:/original/work" };
  const onChanged = vi.fn();
  check.mockResolvedValueOnce({ path: project.path, available: false })
    .mockResolvedValueOnce({ path: project.path, available: true });
  try {
    await act(async () => root.render(
      <I18nProvider initialLocale="en-US">
        <ProjectDirectoryRecovery project={project} revision="0" onChanged={onChanged} />
      </I18nProvider>,
    ));
    expect(mount.querySelector("[role=alert] code")?.textContent).toBe(project.path);
    const recheck = mount.querySelector("button");
    expect(recheck).toBeTruthy();
    await act(async () => recheck?.click());
    expect(check.mock.calls).toEqual([[project.id], [project.id]]);
    expect(mount.querySelector("[role=alert]")).toBeNull();
    expect(onChanged).not.toHaveBeenCalled();
  } finally {
    await act(async () => root.unmount());
  }
});
