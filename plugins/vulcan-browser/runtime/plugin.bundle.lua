-- M contains the exact browser tool hooks declared by this plugin.
-- M 包含此插件声明的精确浏览器工具 Hook。
local M = {}

-- Encode a successful hook result for the managed Lua plugin bridge.
-- 为受管 Lua 插件桥接层编码成功的 Hook 结果。
--
-- result is the hook response; returns the JSON envelope expected by the host.
-- result 是 Hook 响应；返回宿主要求的 JSON 信封。
local function success(result)
    return vulcan.json.encode({ ok = true, result = result })
end

-- Read the complete current Playwright MCP configuration through the plugin-owned host bridge.
-- 通过插件专属宿主桥读取当前完整的 Playwright MCP 配置。
--
-- args is the validated plugin action frame and has no fields for this read; returns one success envelope.
-- args 是已校验的插件 Action 帧，此读取不使用其中字段；返回一个成功信封。
function M.settings_get(args)
    return success(vulcan.host.call("vulcan.plugin.config.get", {}))
end

-- Persist a complete schema-validated Playwright MCP configuration atomically in Core.
-- 在 Core 中原子持久化一份经 Schema 校验的完整 Playwright MCP 配置。
--
-- args.arguments is the full settings object; returns the persisted object in a success envelope.
-- args.arguments 是完整配置对象；返回成功信封中的已持久化对象。
function M.settings_update(args)
    if type(args) ~= "table" or type(args.arguments) ~= "table" then
        error("Browser settings update requires complete arguments")
    end
    local updated = vulcan.host.call("vulcan.plugin.config.update", { value = args.arguments })
    return success(updated)
end

-- Decode the exact host-owned hook invocation frame.
-- 解码由宿主持有的精确 Hook 调用帧。
--
-- args contains invocation_json; returns its decoded JSON object.
-- args 包含 invocation_json；返回解码后的 JSON 对象。
local function decode_invocation(args)
    if type(args.invocation_json) ~= "string" or args.invocation_json == "" then
        error("Browser hook invocation_json is required")
    end
    return vulcan.json.decode(args.invocation_json)
end

-- Accept Playwright's optional filename while promising image output from VulcanCode.
-- 接受 Playwright 的可选文件名，同时保证 VulcanCode 返回图像内容。
--
-- args carries the registration frame; returns the public screenshot definition.
-- args 携带注册帧；返回公开的截图工具定义。
function M.screenshot_registration(args)
    -- invocation is the immutable host hook frame.
    -- invocation 是不可变的宿主 Hook 帧。
    local invocation = decode_invocation(args)
    -- payload contains the original tool identity and definition.
    -- payload 包含原始工具身份和定义。
    local payload = invocation.payload.value
    if type(payload) ~= "table" or type(payload.definition) ~= "table" then
        error("Browser screenshot registration definition is unavailable")
    end
    -- schema is the original MCP argument schema, copied into this Lua frame.
    -- schema 是复制进此 Lua 帧的原始 MCP 参数 Schema。
    local schema = payload.definition.parameters_schema
    if type(schema) ~= "table" or schema.type ~= "object" or type(schema.properties) ~= "table" then
        error("Browser screenshot argument schema is invalid")
    end
    if schema.properties.filename == nil then
        error("Browser screenshot filename contract changed upstream")
    end
    -- The model may still supply a filename; keep it valid so the invocation hook can
    -- remove it before upstream Playwright suppresses the image response.
    -- 模型仍可能提供文件名；保留其合法性，让调用 Hook 在上游 Playwright 因此
    -- 抑制图像响应之前将其移除。
    schema.properties.filename.description =
        "Optional compatibility argument. VulcanCode ignores it and returns the screenshot image directly."
    schema.additionalProperties = false
    -- The registration text reports the same timeout read when a new MCP connection starts.
    -- 注册文案报告新 MCP 连接启动时读取的同一超时值。
    local config = vulcan.host.call("vulcan.plugin.config.get", {})
    if type(config) ~= "table" or type(config.actionTimeoutMs) ~= "string" then
        error("Browser action timeout configuration is unavailable")
    end
    payload.definition.description = payload.definition.description
        .. " VulcanCode returns the screenshot as image content; an optional filename is ignored."
        .. " Configured action timeout: " .. config.actionTimeoutMs .. " ms."
    return success({ payload = { value = payload } })
end

-- Remove an optional filename before upstream Playwright decides whether to return image content.
-- 在上游 Playwright 决定是否返回图像内容前移除可选文件名。
--
-- args carries the trusted session invocation; returns unchanged valid arguments.
-- args 携带可信会话调用；返回未改动的有效参数。
function M.screenshot_before(args)
    -- invocation is the exact session-scoped host hook frame.
    -- invocation 是精确的会话范围宿主 Hook 帧。
    local invocation = decode_invocation(args)
    -- payload contains the original tool identity and call arguments.
    -- payload 包含原始工具身份和调用参数。
    local payload = invocation.payload.value
    if type(payload) ~= "table" or type(payload.arguments) ~= "table" then
        error("Browser screenshot arguments are unavailable")
    end
    payload.arguments.filename = nil
    return success({ payload = { value = payload } })
end

-- Validates the immutable origin of Playwright's unsafe code tool.
-- 校验 Playwright 不安全代码工具的不可变来源身份。
local function require_unsafe_run_code_tool(tool)
    if type(tool) ~= "table"
        or tool.source_kind ~= "mcp"
        or tool.namespace ~= "vulcan-browser:playwright"
        or tool.original_name ~= "browser_run_code_unsafe" then
        error("browser_run_code_unsafe hook received an unexpected tool origin")
    end
end

-- Removes the unsafe code tool from the model-visible catalog while preserving its origin.
-- 从模型可见目录移除不安全代码工具，同时保留其来源身份。
function M.unsafe_run_code_registration(args)
    local invocation = decode_invocation(args)
    local payload = invocation.payload.value
    if type(payload) ~= "table" or type(payload.definition) ~= "table" then
        error("browser_run_code_unsafe registration definition is unavailable")
    end
    require_unsafe_run_code_tool(payload.tool)
    payload.definition.ai_visible = false
    return success({ payload = { value = payload } })
end

-- Removes the exact unsafe tool candidate from the prepared catalog after the visibility transform.
-- 在可见性转换后，从准备好的目录移除精确的不安全工具候选。
function M.unsafe_run_code_registration_block(args)
    local invocation = decode_invocation(args)
    local payload = invocation.payload.value
    if type(payload) ~= "table" or type(payload.definition) ~= "table" then
        error("browser_run_code_unsafe registration definition is unavailable")
    end
    require_unsafe_run_code_tool(payload.tool)
    return success({ decision = "block" })
end

-- Blocks every explicit or stale cached invocation before the MCP process can execute it.
-- 在 MCP 进程执行前阻断所有显式调用或旧缓存调用。
function M.unsafe_run_code_before(args)
    local invocation = decode_invocation(args)
    local payload = invocation.payload.value
    if type(payload) ~= "table" or type(payload.arguments) ~= "table" then
        error("browser_run_code_unsafe invocation arguments are unavailable")
    end
    require_unsafe_run_code_tool(payload.tool)
    return success({ decision = "block" })
end

-- Reject generic plugin-tool dispatch because this plugin only provides hooks and MCP tools.
-- 拒绝通用插件工具调度，因为此插件只提供 Hook 与 MCP 工具。
--
-- args is an unsupported dispatch request; this function always raises an error.
-- args 是不支持的调度请求；此函数始终抛出错误。
function M.dispatch(args)
    error("Vulcan Browser does not expose Lua-dispatched AI tools")
end

-- Close the stateless plugin lease without browser-side effects.
-- 关闭无状态插件租约，不对浏览器产生副作用。
--
-- args is the host shutdown frame; returns a closed marker.
-- args 是宿主关闭帧；返回已关闭标记。
function M.shutdown(args)
    return success({ closed = true })
end

return M
