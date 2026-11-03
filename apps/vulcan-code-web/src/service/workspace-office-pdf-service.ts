import { serviceClient } from "@/service/webui-service-runtime";
import type { WorkspaceOfficePdf } from "@/types";

// loadWorkspaceOfficePdf asks the service to convert the Office document at an absolute path to a cached PDF. It lives
// apart from the snapshot service so only the lazily loaded Office viewer carries it.
// loadWorkspaceOfficePdf 请求服务将绝对路径处的 Office 文档转换为缓存的 PDF。它独立于快照服务，使其只随延迟加载的
// Office 查看器加载。
export async function loadWorkspaceOfficePdf(filePath: string): Promise<WorkspaceOfficePdf> {
  return serviceClient.request<WorkspaceOfficePdf>("workspace.files.officePdf", { filePath });
}
