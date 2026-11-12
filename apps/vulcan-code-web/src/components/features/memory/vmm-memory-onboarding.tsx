import * as React from "react"
import type { VmmMemorySnapshot } from "@/types"
import { loadCloudMemorySetup } from "@/service/cloud-memory-service"
import { loadPublicAiSettings } from "@/service/settings-actions-service"
import { loadMemorySettings } from "@/service/markdown-memory-service"
import { MemoryArea } from "./memory-area"
import { vmmRequiredInferenceReady } from "./vmm-inference-settings"
import type { MemoryActionProps } from "./use-memory-area-controller"

// Remembers only that the browser offered memory setup, without storing account or model details.
// 只记录浏览器已提供记忆引导，不保存账号或模型详情。
const SEEN_KEY = "vulcan.cloudMemorySetup.seen"

// Opens the shared memory wizard once after sign-in when the user needs a memory model.
// 登录后在用户缺少记忆模型时打开一次共享记忆向导。
export default function VmmMemoryOnboarding({ snapshot, actions, onOpenCredentialManagement }: {
  snapshot: VmmMemorySnapshot
  actions: MemoryActionProps
  onOpenCredentialManagement: () => void
}) {
  const [open, setOpen] = React.useState(false)
  React.useEffect(() => {
    try {
      if (window.localStorage.getItem(SEEN_KEY) === "true") return
    } catch {
      return
    }
    let active = true
    void loadMemorySettings().then(async (memory) => {
      if (!active || memory.mode !== "vmm") return
      const ai = await loadPublicAiSettings()
      if (!active || vmmRequiredInferenceReady(ai)) return
      const cloud = await loadCloudMemorySetup()
      if (!active || cloud.selected || !cloud.catalog?.models.length) return
      try {
        window.localStorage.setItem(SEEN_KEY, "true")
      } catch {
        // A restricted browser still permits manual setup.
        // 浏览器存储受限时仍可手动设置。
      }
      setOpen(true)
    }).catch(() => {
      // Startup failures leave the manual Models and Services entry available.
      // 启动读取失败时仍保留模型与服务的手动入口。
    })
    return () => { active = false }
  }, [])
  if (!open) return null
  return <MemoryArea activeSection="overview" actions={actions} snapshot={snapshot}
    onOpenCredentialManagement={onOpenCredentialManagement} onSelectSection={() => undefined}
    setupOnlyOnClose={() => setOpen(false)} />
}
