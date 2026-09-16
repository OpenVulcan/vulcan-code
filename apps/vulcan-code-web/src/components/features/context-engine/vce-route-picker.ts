// VCE-facing aliases for the shared exact-identity route picker helpers.
// 面向 VCE 的共享精确身份路由选择器助手别名。
//
// The key and option derivation is the shared public-AI inference contract rather
// than VCE behavior, so the implementation lives in
// `components/features/inference-route-picker.ts` and every consumer-owned route
// picker, including LuaSkills, uses the same one. These aliases keep the existing
// VCE call sites and their tests unchanged.
// 键与选项的推导属于共享公共 AI 推理契约，而非 VCE 行为，因此实现位于
// `components/features/inference-route-picker.ts`，包括 LuaSkills 在内的每个消费者自有
// 路由选择器共用同一份。这里的别名使既有 VCE 调用点及其测试保持不变。
export {
  inferenceRouteKey as vceInferenceRouteKey,
  inferenceRoutePickerOptions as vceRoutePickerOptions,
} from "@/components/features/inference-route-picker";
