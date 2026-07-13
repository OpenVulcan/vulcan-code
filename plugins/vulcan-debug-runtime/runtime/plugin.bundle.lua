-- Fixed Lua bridge for the host-owned debug protocol sidecar.
-- 面向宿主自有调试协议 sidecar 的固定 Lua 桥。

local M = {}
local sidecar = nil
local frame_marker = string.char(30)

-- Return the persistent managed Node session, opening it exactly once per System lease VM.
-- 返回持久受管 Node 会话，并在每个 System 租约 VM 中仅打开一次。
local function ensure_sidecar()
    if sidecar ~= nil then
        local status = sidecar:status()
        if not status.closed then
            return sidecar
        end
        error("debug sidecar process exited; the protocol session cannot be resumed")
    end

    sidecar = vulcan.process.session.open("debug-sidecar", {
        args = { "--stdio" },
        stdout_encoding = "utf-8",
        stderr_encoding = "utf-8",
        stdin_encoding = "utf-8",
        buffer_limit_bytes = 1024 * 1024,
    })
    return sidecar
end

-- Send one host-encoded command and return its exact single-line JSON response.
-- 发送一条宿主编码命令并返回其精确的单行 JSON 响应。
--
-- args carries the opaque payload_json and host timeout injected by the trusted host.
-- args 携带由可信宿主注入的不透明 payload_json 与宿主超时。
-- Returns the untouched sidecar response JSON text.
-- 返回未经改动的 sidecar 响应 JSON 文本。
function M.dispatch(args)
    local active = ensure_sidecar()
    if type(args.payload_json) ~= "string" or args.payload_json == "" then
        error("debug sidecar payload_json is required")
    end
    -- The host owns JSON encoding so empty objects never collapse into Lua arrays.
    -- JSON 编码由宿主持有，避免空对象退化为 Lua 数组。
    active:write(args.payload_json .. "\n")
    local timeout_ms = tonumber(args.host_timeout_ms) or 30000
    local output = active:read({
        timeout_ms = timeout_ms,
        max_bytes = 1024 * 1024,
        until_text = frame_marker,
    })
    if output.timed_out then
        error("debug sidecar response timed out")
    end
    if output.stdout == nil or output.stdout == "" then
        error("debug sidecar returned no response; stderr=" .. tostring(output.stderr or ""))
    end
    local frame = string.match(output.stdout, "^(.-)" .. frame_marker)
    if frame == nil or frame == "" then
        error("debug sidecar response did not contain one complete frame")
    end
    -- Return the exact frame for host-side decoding without a lossy Lua table round trip.
    -- 原样返回帧并交由宿主解码，避免有损的 Lua 表往返转换。
    return frame
end

-- Close the protocol transport and the persistent managed Node process.
-- 关闭协议传输与持久受管 Node 进程。
--
-- args carries the final opaque close payload and host timeout.
-- args 携带最终的不透明关闭载荷与宿主超时。
-- Returns the untouched close response JSON before terminating the process.
-- 在终止进程前返回未经改动的关闭响应 JSON。
function M.shutdown(args)
    if sidecar == nil then
        return '{"ok":true,"result":{"closed":true}}'
    end
    local response = M.dispatch(args)
    sidecar:close({ timeout_ms = tonumber(args.host_timeout_ms) or 5000 })
    sidecar = nil
    return response
end

return M
