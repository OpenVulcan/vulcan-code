import { readFileSync } from "node:fs";
import ts from "typescript";
import { expect, it, vi } from "vitest";

// sourceFile parses the actual production callback bodies without mounting unrelated workspace services.
// sourceFile 解析实际生产回调函数体，不挂载无关工作台服务。
const sourceFile = ts.createSourceFile("workbench-layout.tsx", readFileSync(new URL("../workbench-layout.tsx", import.meta.url), "utf8"), ts.ScriptTarget.Latest, true, ts.ScriptKind.TSX);

// callback compiles exactly one production declaration with injected service/state observers.
// callback 编译单个准确生产声明，注入服务与状态观察器。
// name selects the existing declaration; dependencies supplies lexical values. Returns its real async closure.
// name 选择既有声明，dependencies 提供词法值；返回真实异步闭包。
function callback(name: string, dependencies: Record<string, unknown>): (...args: unknown[]) => Promise<void> {
  // declaration is resolved by its exact AST variable name, never a regex approximation of its body.
  // declaration 按准确 AST 变量名定位，不用正则近似函数体。
  let declaration: ts.VariableDeclaration | undefined;
  // visit finds the unique named declaration in the production component.
  // visit 在生产组件中查找唯一具名声明。
  function visit(node: ts.Node): void {
    if (ts.isVariableDeclaration(node) && node.name.getText(sourceFile) === name) declaration = node;
    ts.forEachChild(node, visit);
  }
  visit(sourceFile);
  expect(declaration).toBeDefined();
  // compiled preserves the callback and dependency list while removing only TypeScript annotations.
  // compiled 保留回调和依赖列表，仅删除 TypeScript 注解。
  const compiled = ts.transpileModule(`const ${declaration!.getText(sourceFile)};`, { compilerOptions: { target: ts.ScriptTarget.ES2022, module: ts.ModuleKind.None } }).outputText;
  return new Function(...Object.keys(dependencies), `${compiled}; return ${name};`)(...Object.values(dependencies));
}

for (const kind of ["archive", "remove"] as const) {
  // Verify duplicate confirmation and a missing authoritative snapshot cannot mutate local lifecycle state.
  // 验证重复确认与缺失权威快照不能变更本地生命周期状态。
  it(`${kind} confirmation excludes duplicate calls and preserves current state on a null response`, async () => {
    // finish delays the service response past a second synchronous confirmation click.
    // finish 将服务响应延迟至第二次同步确认点击之后。
    let finish!: (value: null) => void;
    // service is the isolated lifecycle RPC used by the exact production callback.
    // service 是准确生产回调调用的隔离生命周期 RPC。
    const service = vi.fn(() => new Promise<null>((resolve) => { finish = resolve; }));
    // apply and target observe forbidden preview updates and premature dialog closure.
    // apply 和 target 观察不应发生的预览更新与提前关闭。
    const apply = vi.fn();
    const target = vi.fn();
    // inFlight models the persistent ref owned by the production component across repeated callbacks.
    // inFlight 模拟生产组件在重复回调之间持有的持久引用。
    const inFlight = { current: false };
    // handler retains production validation, confirmation and failure branches unchanged.
    // handler 保留不变的生产校验、确认及失败分支。
    const handler = callback(kind === "archive" ? "handleArchiveProjectConfirm" : "handleRemoveProjectConfirm", {
      React: { useCallback: (value: unknown) => value }, i18n: { t: (key: string) => key }, toast: { error: vi.fn(), success: vi.fn() },
      workbenchState: { conversations: [{ id: "existing", projectId: "p" }] },
      archiveProjectTarget: { id: "p" }, removeProjectTarget: { id: "p" }, archiveProjectSubmitting: false, removeProjectSubmitting: false,
      archiveProjectInFlightRef: inFlight, removeProjectInFlightRef: inFlight,
      setArchiveProjectSubmitting: vi.fn(), setRemoveProjectSubmitting: vi.fn(),
      archiveProjectConversations: service, removeProject: service,
      applyProjectActionSnapshot: apply, applyLifecycleResponseSnapshot: vi.fn(),
      setArchiveProjectTarget: target, setRemoveProjectTarget: target,
    });
    // pending spans the first confirmation; the second must synchronously return without another RPC.
    // pending 覆盖第一次确认；第二次必须同步返回而不再次调用 RPC。
    const pending = handler();
    await handler();
    expect(service).toHaveBeenCalledOnce();
    finish(null);
    await pending;
    expect(apply).not.toHaveBeenCalled();
    expect(target).not.toHaveBeenCalled();
    expect(inFlight.current).toBe(false);
  });
}

// A missing conversation archive snapshot must not apply a stale whole-workbench preview.
// 会话归档快照缺失时不得应用旧的完整工作台预览。
it("retains the selected conversation when archive returns no authoritative snapshot", async () => {
  // pending and apply observe request unlock and absence of local snapshot writes.
  // pending 和 apply 观察请求解锁及无本地快照写入。
  const pending = new Set<string>();
  const apply = vi.fn();
  // handler executes the actual conversation menu archive branch with a null service result.
  // handler 使用空服务结果执行真实会话菜单归档分支。
  const handler = callback("handleConversationMenuAction", {
    React: { useCallback: (value: unknown) => value }, i18n: { t: (key: string) => key }, toast: { error: vi.fn(), success: vi.fn() },
    workbenchState: { conversations: [] }, conversationArchivePendingIdsRef: { current: pending },
    archiveConversation: vi.fn().mockResolvedValue(null), applyProjectActionSnapshot: apply, applyLifecycleResponseSnapshot: vi.fn(),
  });
  await handler("archive", { id: "c", title: "Current" });
  expect(apply).not.toHaveBeenCalled();
  expect(pending.size).toBe(0);
});
