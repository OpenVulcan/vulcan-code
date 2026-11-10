// projectPathKey normalizes one project folder path for equality checks, so picker text and service
// snapshot text describing the same folder compare equal across separators, trailing slashes and case.
// projectPathKey 归一化单个项目文件夹路径用于相等比较，使选择器文本与服务工作台快照中描述同一文件夹的文本
// 在分隔符、末尾斜杠与大小写上比较相等。
export function projectPathKey(path: string): string {
  return path.replace(/\\/g, "/").replace(/\/+$/g, "").toLocaleLowerCase() || "/";
}
