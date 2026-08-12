// acquireVisibleTerminalDisplayListener retains one asynchronously created topic listener only while its surface is visible.
// acquireVisibleTerminalDisplayListener 仅在终端表面仍可见时保留一个异步创建的主题监听器。
//
// Parameters / 参数:
// - `subscribe`: Creates the service topic listener and resolves its disposer. / 创建服务主题监听器并返回其释放函数。
// - `isCancelled`: Reports whether the owning surface was hidden or unmounted while subscribing. / 报告所属表面是否在订阅期间被隐藏或卸载。
// - `retain`: Stores the disposer for the still-visible surface cleanup path. / 为仍可见表面的清理路径保存释放函数。
//
// Returns / 返回值:
// Returns true when service attachment may continue, or false after releasing a listener that resolved too late.
// 当服务挂接可以继续时返回 true；若监听器返回过晚并已被释放，则返回 false。
export async function acquireVisibleTerminalDisplayListener(
  subscribe: () => Promise<() => void>,
  isCancelled: () => boolean,
  retain: (unsubscribe: () => void) => void,
): Promise<boolean> {
  // unsubscribe is the exact disposer returned by the completed browser topic setup.
  // unsubscribe 是已完成的浏览器主题建立所返回的精确释放函数。
  const unsubscribe = await subscribe();
  if (isCancelled()) {
    unsubscribe();
    return false;
  }
  retain(unsubscribe);
  return true;
}
