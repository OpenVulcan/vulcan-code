-- Integration harness for the fixed Vulcan Plugin allowlisted Lua module bridge.
-- 固定 Vulcan Plugin 白名单 Lua 模块桥接集成测试夹具。

-- runtime_vulcan retains the controlled runtime JSON implementation outside the bridge sandbox.
-- runtime_vulcan 在桥接沙箱外保留受控运行时 JSON 实现。
local runtime_vulcan = vulcan

-- windows_runtime records whether this fixture must exercise verbatim package-search normalization.
-- windows_runtime 记录当前夹具是否必须验证 verbatim 包搜索路径规范化。
local windows_runtime = package.config:sub(1, 1) == "\\"
if windows_runtime then
    -- verbatim_lua_template reproduces the exact Vulcan Plugin path that previously became a false UNC probe.
    -- verbatim_lua_template 复现此前会变成错误 UNC 探测的精确 Vulcan Plugin 路径。
    local verbatim_lua_template = [[\\?\D:\vulcan-plugin-bridge-path-probe\?.lua;]]
    -- verbatim_c_template covers the equivalent native-module search template.
    -- verbatim_c_template 覆盖对应的原生模块搜索模板。
    local verbatim_c_template = [[\\?\D:\vulcan-plugin-bridge-path-probe\?.dll;]]
    -- forward_verbatim_lua_template covers the slash spelling accepted by the same Windows boundary.
    -- forward_verbatim_lua_template 覆盖同一 Windows 边界接受的正斜杠写法。
    local forward_verbatim_lua_template = "//?/D:/vulcan-plugin-bridge-forward-probe/?.lua;"
    -- verbatim_unc_lua_template covers canonical Windows network-share Lua roots.
    -- verbatim_unc_lua_template 覆盖 Windows 规范化后的网络共享 Lua 根目录。
    local verbatim_unc_lua_template = [[\\?\UNC\server\share\?.lua;]]
    -- forward_verbatim_unc_c_template covers slash-form network-share native-module roots.
    -- forward_verbatim_unc_c_template 覆盖正斜杠形式的网络共享原生模块根目录。
    local forward_verbatim_unc_c_template = "//?/UNC/server/share/?.dll;"
    package.path = verbatim_lua_template
        .. forward_verbatim_lua_template
        .. verbatim_unc_lua_template
        .. package.path
    package.cpath = verbatim_c_template .. forward_verbatim_unc_c_template .. package.cpath
end

-- handle reads the exact production bridge source under test.
-- handle 读取待测试的精确生产桥源码。
local handle = assert(io.open(args.bridge_path, "rb"))

-- bridge_source is the immutable fixed bridge source.
-- bridge_source 是不可变固定桥源码。
local bridge_source = handle:read("*a")
handle:close()

-- bridge_loader compiles the fixed bridge in the trusted runtime environment.
-- bridge_loader 在可信运行时环境内编译固定桥。
local bridge_loader = assert(load(bridge_source, "@runtime/bridge.lua", "t"))

-- bridge is the exact trusted module returned by production source.
-- bridge 是生产源码返回的精确可信模块。
local bridge = bridge_loader()

if windows_runtime then
    assert(string.find(package.path, [[\\?\]], 1, true) == nil)
    assert(string.find(package.cpath, [[\\?\]], 1, true) == nil)
    assert(string.find(package.path, [[D:\vulcan-plugin-bridge-path-probe\?.lua]], 1, true) == 1)
    assert(string.find(package.cpath, [[D:\vulcan-plugin-bridge-path-probe\?.dll]], 1, true) == 1)
    assert(string.find(package.path, "D:/vulcan-plugin-bridge-forward-probe/?.lua", 1, true) ~= nil)
    assert(string.find(package.path, [[\\server\share\?.lua]], 1, true) ~= nil)
    assert(string.find(package.cpath, "//server/share/?.dll", 1, true) ~= nil)
end

-- bundle_source exercises declared access, undeclared rejection, random bytes, and sandbox globals.
-- bundle_source 覆盖已声明访问、未声明拒绝、随机字节与沙箱全局变量。
local bundle_source = [[
local random = vulcan.runtime.module("openssl.rand")
local M = {}

function M.dispatch(args)
    assert(require == nil)
    assert(package == nil)
    assert(io == nil)
    assert(os == nil)
    assert(debug == nil)
    local undeclared_allowed = pcall(vulcan.runtime.module, "socket")
    assert(undeclared_allowed == false)
    local bytes = random.bytes(8)
    return vulcan.json.encode({
        ok = true,
        result = {
            bytes = #bytes,
            windows = vulcan.runtime.is_windows(),
            emptyObject = vulcan.json.object(),
            emptyArray = vulcan.json.array(),
            nullValue = vulcan.json.null,
        },
    })
end

function M.shutdown(args)
    return vulcan.json.encode({ ok = true, result = { closed = true } })
end

return M
]]

-- loaded proves declared modules remain lazy and an unused unavailable name is never required.
-- loaded 证明声明模块保持延迟加载，未使用且不可用的名称绝不会触发 require。
local loaded = bridge.load({
    plugin_id = "bridge-module-test",
    runtime_id = "runtime-test",
    lease_scope = "enabled",
    workspace_root = args.workspace_root,
    bundle_source = bundle_source,
    lua_modules = { "openssl.rand", "vulcan.unavailable.lazy.module" },
    invoke_function = "dispatch",
    shutdown_function = "shutdown",
    hook_functions = {},
    additional_functions = {},
})
assert(loaded == true)
assert(bridge.probe() == true)

-- raw is the exact serialized plugin result returned through bridge invocation.
-- raw 是通过桥接调用返回的精确序列化插件结果。
local raw = bridge.invoke({
    function_name = "dispatch",
    plugin_args = {},
})

-- decoded is parsed outside the restricted plugin environment for test assertions.
-- decoded 在受限插件环境之外解析，用于测试断言。
local decoded = runtime_vulcan.json.decode(raw)
assert(decoded.ok == true)
assert(decoded.result.bytes == 8)
assert(type(decoded.result.windows) == "boolean")
assert(runtime_vulcan.json.encode(decoded.result.emptyObject) == "{}")
assert(runtime_vulcan.json.encode(decoded.result.emptyArray) == "[]")
assert(decoded.result.nullValue == runtime_vulcan.json.null)

return {
    loaded = true,
    undeclared_module_blocked = true,
    raw_require_hidden = true,
    verbatim_search_paths_normalized = not windows_runtime
        or (string.find(package.path, [[\\?\]], 1, true) == nil
            and string.find(package.cpath, [[\\?\]], 1, true) == nil),
    random_bytes = decoded.result.bytes,
    windows = decoded.result.windows,
}
