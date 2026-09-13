import React, { type ReactNode } from "react";

// RenderErrorBoundaryProps provides content and a caller-owned localized recovery surface.
// RenderErrorBoundaryProps 提供正文与调用方持有的本地化恢复界面。
interface RenderErrorBoundaryProps {
  // children contains the independently recoverable application region.
  // children 包含可独立恢复的应用区域。
  children: ReactNode;
  // fallback renders a failure without resetting unrelated application state.
  // fallback 渲染失败界面，而不重置无关应用状态。
  fallback: (error: Error) => ReactNode;
  // resetKey clears a retained failure on navigation without remounting healthy content.
  // resetKey 在导航时清除保留的失败，而不重挂载正常内容。
  resetKey?: string;
}

// RenderErrorBoundaryState retains the error until the owner mounts a new region identity.
// RenderErrorBoundaryState 保留错误，直到所属层挂载新的区域标识。
interface RenderErrorBoundaryState {
  // error is null while the child subtree is healthy.
  // error 在子树正常时为 null。
  error: Error | null;
}

// RenderErrorBoundary isolates a rejected lazy import or render failure from sibling navigation.
// RenderErrorBoundary 将延迟导入拒绝或渲染失败与相邻导航隔离。
// Parameters: props supplies children and its localized recovery renderer. Returns: a React boundary instance.
// 参数：props 提供子树及其本地化恢复渲染器。返回值：React 错误边界实例。
export class RenderErrorBoundary extends React.Component<RenderErrorBoundaryProps, RenderErrorBoundaryState> {
  // state begins healthy and never fabricates a successful retry of React.lazy's cached rejection.
  // state 初始正常，绝不把 React.lazy 缓存的拒绝伪装为重试成功。
  state: RenderErrorBoundaryState = { error: null };

  // getDerivedStateFromError converts the thrown value into the explicit failure presentation.
  // getDerivedStateFromError 将抛出的值转换为明确的失败展示。
  // Parameters: error is supplied by React. Returns: retained error state.
  // 参数：error 由 React 提供。返回值：保留的错误状态。
  static getDerivedStateFromError(error: unknown): RenderErrorBoundaryState {
    return { error: error instanceof Error ? error : new Error(String(error)) };
  }

  // componentDidUpdate releases an old region failure after a deliberate navigation change.
  // componentDidUpdate 在明确导航变化后释放旧区域的失败。
  // Parameters: previous contains the last committed props. Returns nothing.
  // 参数：previous 包含上一次已提交属性。无返回值。
  componentDidUpdate(previous: RenderErrorBoundaryProps): void {
    if (this.state.error && previous.resetKey !== this.props.resetKey) this.setState({ error: null });
  }

  // render returns healthy children or the caller-owned failure UI without retrying side effects.
  // render 返回正常子树或调用方持有的失败界面，不重试副作用。
  // Returns: the selected React subtree; no parameters are accepted.
  // 返回值：所选 React 子树；不接收参数。
  render(): ReactNode {
    return this.state.error ? this.props.fallback(this.state.error) : this.props.children;
  }
}
