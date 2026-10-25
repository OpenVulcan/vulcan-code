import { serviceClient } from "@/service/webui-service-runtime";
import type {
  EnvironmentVariableNameCheck,
  EnvironmentVariableSaveRequest,
  EnvironmentVariablesSnapshot,
  SessionEnvironmentSnapshot,
} from "@/types";

// loadEnvironmentVariables reads the stored global and project variables and the naming rules.
// loadEnvironmentVariables 读取已存储的全局与项目变量及命名规则。
export async function loadEnvironmentVariables(): Promise<EnvironmentVariablesSnapshot> {
  return serviceClient.request<EnvironmentVariablesSnapshot>("service_environment_variables", {});
}

// saveEnvironmentVariable creates or updates one variable; leaving value out keeps the stored one.
// saveEnvironmentVariable 创建或更新一条变量；省略 value 时保留已存储的值。
export async function saveEnvironmentVariable(request: EnvironmentVariableSaveRequest): Promise<EnvironmentVariablesSnapshot> {
  return serviceClient.request<EnvironmentVariablesSnapshot>("service_save_environment_variable", { request });
}

// deleteEnvironmentVariable removes one stored variable.
// deleteEnvironmentVariable 删除一条已存储变量。
export async function deleteEnvironmentVariable(id: string): Promise<EnvironmentVariablesSnapshot> {
  return serviceClient.request<EnvironmentVariablesSnapshot>("service_delete_environment_variable", { id });
}

// checkEnvironmentVariableName reports whether a name can be stored and whether the system defines it.
// checkEnvironmentVariableName 报告名称能否存储以及系统是否已定义该名称。
export async function checkEnvironmentVariableName(name: string): Promise<EnvironmentVariableNameCheck> {
  return serviceClient.request<EnvironmentVariableNameCheck>("service_check_environment_variable_name", { name });
}

// loadSessionEnvironment reads the variables one session keeps itself.
// loadSessionEnvironment 读取一个会话自身保存的变量。
export async function loadSessionEnvironment(sessionId: string): Promise<SessionEnvironmentSnapshot> {
  return serviceClient.request<SessionEnvironmentSnapshot>("service_session_environment", { sessionId });
}

// deleteSessionEnvironmentVariable removes one variable a session keeps itself.
// deleteSessionEnvironmentVariable 删除会话自身保存的一个变量。
export async function deleteSessionEnvironmentVariable(sessionId: string, name: string): Promise<SessionEnvironmentSnapshot> {
  return serviceClient.request<SessionEnvironmentSnapshot>("service_delete_session_environment_variable", { sessionId, name });
}
