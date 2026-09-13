import { createContext } from "react";

// CustomProviderMutationBoundary serializes all editors belonging to one provider dialog.
// CustomProviderMutationBoundary 串行化属于同一个供应商弹窗的全部编辑器。
export interface CustomProviderMutationBoundary {
  // Returns true only when this operation acquired the dialog-wide write lock.
  // 仅当当前操作获得弹窗范围写入锁时返回 true。
  acquire: () => boolean;
  // Releases the acquired lock after the exact operation settles.
  // 在精确操作结束后释放已获取的锁。
  release: () => void;
}

// Standalone cards retain their own local guard; nested editors additionally share this boundary.
// 独立卡片保留自身局部守卫；嵌套编辑器额外共享此边界。
export const CustomProviderMutationContext = createContext<CustomProviderMutationBoundary | null>(null);
