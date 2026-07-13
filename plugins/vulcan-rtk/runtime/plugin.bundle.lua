-- Vulcan RTK plugin runtime implemented entirely through the fixed Lua bridge.
-- 完全通过固定 Lua 桥接层实现的 Vulcan RTK 插件运行时。

-- M contains every function declared by the RTK plugin manifest.
-- M 包含 RTK 插件清单声明的全部函数。
local M = {}

-- Encode one successful plugin response envelope.
-- 编码一个成功的插件响应信封。
--
-- result is the exact JSON-safe value returned to the host.
-- result 是返回宿主的精确 JSON 安全值。
-- Returns the JSON string required by the managed plugin ABI.
-- 返回受管插件 ABI 所需的 JSON 字符串。
local function success(result)
    return vulcan.json.encode({ ok = true, result = result })
end

-- Read the complete schema-validated RTK plugin configuration.
-- 读取完整且通过 Schema 校验的 RTK 插件配置。
--
-- Returns the host-owned configuration object.
-- 返回宿主持有的配置对象。
local function read_config()
    return vulcan.host.call("vulcan.plugin.config.get", {})
end

-- Build the only environment map allowed for the bundled RTK executable.
-- 构建随包 RTK 可执行文件唯一允许使用的环境映射。
--
-- config is the current complete plugin configuration.
-- config 是当前完整插件配置。
-- Returns a string-only environment table.
-- 返回仅包含字符串值的环境表。
local function rtk_environment(config)
    if config.telemetryDisabled then
        return { RTK_TELEMETRY_DISABLED = "1" }
    end
    return {}
end

-- Execute the manifest-declared RTK binary with bounded options.
-- 使用有界选项执行清单声明的 RTK 二进制文件。
--
-- arguments is an ordered string array, cwd is an optional trusted workspace path, and timeout_ms is bounded.
-- arguments 是有序字符串数组，cwd 是可选可信工作区路径，timeout_ms 受硬边界约束。
-- Returns the structured LuaSkills process result.
-- 返回结构化 LuaSkills 进程结果。
local function run_rtk(arguments, cwd, timeout_ms, config)
    -- options contains only fields authorized by the fixed bridge process resolver.
    -- options 仅包含固定桥接进程解析器授权的字段。
    local options = {
        args = arguments,
        env = rtk_environment(config),
        timeout_ms = timeout_ms,
        stdout_encoding = "utf-8",
        stderr_encoding = "utf-8",
    }
    if cwd ~= nil then
        options.cwd = cwd
    end
    return vulcan.process.exec("rtk", options)
end

-- Decode one exact host hook invocation frame.
-- 解码一个精确的宿主 Hook 调用帧。
--
-- args is the bridge-owned call argument table.
-- args 是桥接层持有的调用参数表。
-- Returns the decoded invocation object.
-- 返回解码后的调用对象。
local function decode_invocation(args)
    if type(args.invocation_json) ~= "string" or args.invocation_json == "" then
        error("RTK hook invocation_json is required")
    end
    return vulcan.json.decode(args.invocation_json)
end

-- Extract the first non-empty output line from `rtk rewrite`.
-- 从 `rtk rewrite` 输出中提取第一条非空行。
--
-- stdout is the exact decoded process stdout.
-- stdout 是进程精确解码后的标准输出。
-- Returns a trimmed command or nil when RTK did not match the input.
-- 返回去空白后的命令；RTK 未命中输入时返回 nil。
local function first_non_empty_line(stdout)
    for line in string.gmatch(stdout or "", "[^\r\n]+") do
        -- trimmed removes only surrounding whitespace and preserves command quoting.
        -- trimmed 只移除两端空白并保留命令引号。
        local trimmed = string.match(line, "^%s*(.-)%s*$")
        if trimmed ~= "" then
            return trimmed
        end
    end
    return nil
end

-- Tokenize a rewritten command sufficiently to inspect RTK route boundaries.
-- 对改写命令做足以检查 RTK 路由边界的分词。
--
-- command is the exact single-line rewrite output.
-- command 是精确的单行 rewrite 输出。
-- Returns ordered tokens while preserving quoted substrings as one token.
-- 返回有序 Token，并把引号包裹内容保留为单个 Token。
local function tokenize_command(command)
    -- tokens accumulates completed shell tokens.
    -- tokens 累积已完成的 shell Token。
    local tokens = {}
    -- current accumulates the current token text.
    -- current 累积当前 Token 文本。
    local current = ""
    -- quote stores the active quote character or nil.
    -- quote 保存当前活动引号字符或 nil。
    local quote = nil
    -- index is the current byte position; RTK route syntax is ASCII.
    -- index 是当前字节位置；RTK 路由语法仅使用 ASCII。
    local index = 1
    while index <= #command do
        -- character is the current ASCII command character.
        -- character 是当前 ASCII 命令字符。
        local character = string.sub(command, index, index)
        if quote ~= nil then
            if character == quote then
                quote = nil
            else
                current = current .. character
            end
        elseif character == "\"" or character == "'" then
            quote = character
        elseif string.match(character, "%s") ~= nil then
            if current ~= "" then
                table.insert(tokens, current)
                current = ""
            end
        elseif character == ";" or character == "&" or character == "|" then
            if current ~= "" then
                table.insert(tokens, current)
                current = ""
            end
            -- next_character permits exact recognition of && and || operators.
            -- next_character 用于精确识别 && 与 || 操作符。
            local next_character = string.sub(command, index + 1, index + 1)
            if next_character == character and (character == "&" or character == "|") then
                table.insert(tokens, character .. next_character)
                index = index + 1
            else
                table.insert(tokens, character)
            end
        else
            current = current .. character
        end
        index = index + 1
    end
    if current ~= "" then
        table.insert(tokens, current)
    end
    return tokens
end

-- Return whether one token identifies the RTK executable.
-- 返回单个 Token 是否标识 RTK 可执行文件。
--
-- token is one unquoted shell token.
-- token 是一个去引号的 shell Token。
-- Returns true for `rtk`, `rtk.exe`, or a path ending in either name.
-- 对 `rtk`、`rtk.exe` 或以二者结尾的路径返回 true。
local function is_rtk_executable(token)
    -- normalized converts Windows separators before selecting the basename.
    -- normalized 在选择基础文件名前转换 Windows 分隔符。
    local normalized = string.gsub(token, "\\", "/")
    -- basename is the final path segment in lowercase.
    -- basename 是转为小写的最后一个路径片段。
    local basename = string.lower(string.match(normalized, "([^/]+)$") or normalized)
    return basename == "rtk" or basename == "rtk.exe"
end

-- Collect unique route names from an RTK-rewritten command.
-- 从 RTK 改写后的命令收集唯一的路由名称。
--
-- command is the exact rewrite output.
-- command 是精确的 rewrite 输出。
-- Returns both an ordered route array and a lookup set.
-- 同时返回有序路由数组与查询集合。
local function collect_routes(command)
    -- routes preserves deterministic first-seen order for display.
    -- routes 为展示保留确定性的首次出现顺序。
    local routes = {}
    -- route_set supports exact configuration checks.
    -- route_set 支持精确配置检查。
    local route_set = {}
    -- expect_route tracks whether the previous token was an RTK executable.
    -- expect_route 跟踪前一个 Token 是否为 RTK 可执行文件。
    local expect_route = false
    for _, token in ipairs(tokenize_command(command)) do
        if expect_route then
            if token ~= "&&" and token ~= "||" and token ~= "|" and token ~= ";" and token ~= "&" and string.sub(token, 1, 1) ~= "-" then
                -- route is normalized because configuration route names are lowercase.
                -- route 被归一化，因为配置中的路由名称使用小写。
                local route = string.lower(token)
                if route_set[route] == nil then
                    route_set[route] = true
                    table.insert(routes, route)
                end
            end
            expect_route = false
        end
        if is_rtk_executable(token) then
            expect_route = true
        end
    end
    return routes, route_set
end

-- Convert one configured route array into an exact lookup set.
-- 把一个配置路由数组转换为精确查询集合。
--
-- values is the schema-validated route array.
-- values 是通过 Schema 校验的路由数组。
-- Returns a lowercase route lookup table.
-- 返回小写路由查询表。
local function route_lookup(values)
    -- lookup contains normalized route membership.
    -- lookup 包含归一化后的路由成员关系。
    local lookup = {}
    for _, value in ipairs(values) do
        lookup[string.lower(value)] = true
    end
    return lookup
end

-- Decide whether every route in a rewrite result is enabled by configuration.
-- 根据配置判断 rewrite 结果中的每条路由是否均已启用。
--
-- routes is an ordered non-empty route array and config is the complete plugin configuration.
-- routes 是非空有序路由数组，config 是完整插件配置。
-- Returns true only when no route is denied by the exact policy.
-- 仅当精确策略未拒绝任何路由时返回 true。
local function routes_allowed(routes, config)
    if #routes == 0 or config.routeMode == "disabled" then
        return false
    end
    -- enabled contains the selected-mode allowlist.
    -- enabled 包含精选模式白名单。
    local enabled = route_lookup(config.enabledRoutes)
    -- disabled contains the explicit denylist applied to all modes.
    -- disabled 包含应用于全部模式的显式拒绝列表。
    local disabled = route_lookup(config.disabledRoutes)
    for _, route in ipairs(routes) do
        if disabled[route] or (config.routeMode == "selected" and not enabled[route]) then
            return false
        end
    end
    return config.routeMode == "selected" or config.routeMode == "all"
end

-- Remove RTK runtime warning lines from compressed Git output.
-- 从压缩后的 Git 输出中移除 RTK 运行时警告行。
--
-- stdout is exact decoded RTK stdout.
-- stdout 是精确解码后的 RTK 标准输出。
-- Returns cleaned output without changing meaningful content lines.
-- 返回不改变有效内容行的清理后输出。
local function clean_rtk_output(stdout)
    -- lines accumulates non-warning output.
    -- lines 累积非警告输出。
    local lines = {}
    for line in string.gmatch((stdout or "") .. "\n", "(.-)\r?\n") do
        if string.match(line, "^%s*%[rtk%]") == nil then
            table.insert(lines, line)
        end
    end
    return string.match(table.concat(lines, "\n"), "^%s*(.-)%s*$") or ""
end

-- Execute one RTK Git compression command and return a structured section body.
-- 执行一条 RTK Git 压缩命令并返回结构化区块正文。
--
-- arguments is the exact RTK argument array and repository_root is trusted by the bridge.
-- arguments 是精确 RTK 参数数组，repository_root 由桥接层信任。
-- Returns cleaned stdout or nil when the command failed.
-- 返回清理后的标准输出；命令失败时返回 nil。
local function run_git_section(arguments, repository_root, config)
    -- result is the bounded process execution result.
    -- result 是受边界约束的进程执行结果。
    local result = run_rtk(arguments, repository_root, 5000, config)
    if not result.success then
        return nil
    end
    -- body normalizes empty output into an explicit stable marker.
    -- body 把空输出归一化为明确稳定的标记。
    local body = clean_rtk_output(result.stdout)
    if body == "" then
        body = "(none)"
    end
    return body
end

-- Handle the generic shell-command pre-execution transform hook.
-- 处理通用 shell 命令执行前转换 Hook。
--
-- args contains the exact serialized HookInvocation frame.
-- args 包含精确序列化的 HookInvocation 帧。
-- Returns one successful plugin envelope containing a HookResult.
-- 返回一个包含 HookResult 的成功插件信封。
function M.transform_shell_command(args)
    -- invocation is the immutable host hook frame.
    -- invocation 是不可变的宿主 Hook 帧。
    local invocation = decode_invocation(args)
    -- payload is the exact host-owned shell transform protocol value.
    -- payload 是精确的宿主持有 shell 转换协议值。
    local payload = invocation.payload.value
    -- config is the current complete plugin configuration.
    -- config 是当前完整插件配置。
    local config = read_config()
    if not config.enabled or config.routeMode == "disabled" then
        return success({ payload = { value = payload } })
    end
    -- rewrite invokes RTK without shell interpolation and inside the trusted workspace cwd.
    -- rewrite 不经过 shell 插值，并在可信工作区 cwd 内调用 RTK。
    local rewrite = run_rtk({ "rewrite", payload.command }, payload.cwd, 2000, config)
    -- rewritten is nil when RTK intentionally does not match the command.
    -- RTK 有意不命中命令时 rewritten 为 nil。
    local rewritten = first_non_empty_line(rewrite.stdout)
    if rewritten == nil then
        return success({
            payload = { value = payload },
            diagnostics = {{
                severity = "info",
                message = "RTK rewrite was not applied: " .. tostring(rewrite.error or rewrite.stderr or "no rewrite output"),
            }},
        })
    end
    -- routes contains every RTK subcommand referenced by the rewrite.
    -- routes 包含改写结果引用的全部 RTK 子命令。
    local routes = collect_routes(rewritten)
    if not routes_allowed(routes, config) then
        return success({
            payload = { value = payload },
            diagnostics = {{
                severity = "info",
                message = "RTK rewrite routes were denied by plugin configuration: " .. table.concat(routes, ", "),
            }},
        })
    end
    payload.command = rewritten
    if config.telemetryDisabled then
        payload.environment.RTK_TELEMETRY_DISABLED = "1"
    end
    table.insert(payload.binary_path_requests, {
        registration_id = invocation.registration.id,
        binary_id = "rtk",
    })
    table.insert(payload.contributions, {
        badge = "RTK",
        label = "Command router",
        value = "vulcan-rtk (" .. table.concat(routes, ", ") .. ")",
    })
    return success({ payload = { value = payload } })
end

-- Handle the generic Git commit-summary context transform hook.
-- 处理通用 Git 提交总结上下文转换 Hook。
--
-- args contains the exact serialized HookInvocation frame.
-- args 包含精确序列化的 HookInvocation 帧。
-- Returns one successful plugin envelope containing a HookResult.
-- 返回一个包含 HookResult 的成功插件信封。
function M.transform_git_commit_context(args)
    -- invocation is the immutable host hook frame.
    -- invocation 是不可变的宿主 Hook 帧。
    local invocation = decode_invocation(args)
    -- payload is the exact host-owned Git context protocol value.
    -- payload 是精确的宿主持有 Git 上下文协议值。
    local payload = invocation.payload.value
    -- config is the current complete plugin configuration.
    -- config 是当前完整插件配置。
    local config = read_config()
    if not config.enabled or not config.gitCommitContextEnabled then
        return success({ payload = { value = payload } })
    end
    -- status is always present so the model sees branch and worktree state.
    -- status 始终存在，使模型能看到分支与工作树状态。
    local status = run_git_section({ "git", "status", "--short", "--branch" }, payload.repository_root, config)
    -- staged is the compressed staged diff.
    -- staged 是压缩后的暂存区差异。
    local staged = run_git_section({ "git", "diff", "--cached", "--no-color" }, payload.repository_root, config)
    if status == nil or staged == nil then
        return success({ payload = { value = payload } })
    end
    -- sections replaces raw Git sections only after all required RTK calls succeed.
    -- sections 仅在全部必需 RTK 调用成功后替换原始 Git 区块。
    local sections = {
        { id = "status", title = "Status (RTK)", body = status, code_fence = true },
        { id = "staged", title = "Staged changes (RTK compressed)", body = staged, code_fence = true },
    }
    if payload.include_unstaged then
        -- unstaged is optional only when the host explicitly excludes unstaged changes.
        -- 仅当宿主明确排除未暂存变更时，unstaged 才是可选项。
        local unstaged = run_git_section({ "git", "diff", "--no-color" }, payload.repository_root, config)
        if unstaged == nil then
            return success({ payload = { value = payload } })
        end
        table.insert(sections, {
            id = "unstaged",
            title = "Unstaged tracked changes (RTK compressed)",
            body = unstaged,
            code_fence = true,
        })
        -- Preserve host-produced untracked-file context because RTK only compresses tracked Git output.
        -- 保留宿主生成的未跟踪文件上下文，因为 RTK 只压缩已跟踪的 Git 输出。
        for _, original_section in ipairs(payload.sections) do
            if original_section.id == "untracked" then
                table.insert(sections, original_section)
            end
        end
    end
    payload.sections = sections
    table.insert(payload.contributions, {
        badge = "RTK",
        label = "Git context",
        value = "compressed by vulcan-rtk",
    })
    return success({ payload = { value = payload } })
end

-- Return the current complete plugin configuration to the mounted UI.
-- 向挂载界面返回当前完整插件配置。
--
-- args is the generic action call table and is unused for this read.
-- args 是通用 action 调用表，此读取操作不使用它。
-- Returns one successful configuration envelope.
-- 返回一个成功的配置响应信封。
function M.settings_get(args)
    return success(read_config())
end

-- Persist one complete schema-validated plugin configuration from the mounted UI.
-- 持久化挂载界面提交的一份完整且通过 Schema 校验的插件配置。
--
-- args.arguments is the exact action input validated by the host.
-- args.arguments 是由宿主校验的精确 action 输入。
-- Returns the persisted configuration object.
-- 返回持久化后的配置对象。
function M.settings_update(args)
    -- updated is returned by the host only after atomic persistence succeeds.
    -- updated 仅在宿主原子持久化成功后返回。
    local updated = vulcan.host.call("vulcan.plugin.config.update", { value = args.arguments })
    return success(updated)
end

-- Parse one compact RTK metric such as 3.2K, 7.5M, or 1.1G into an integer-like number.
-- 把 3.2K、7.5M 或 1.1G 这类 RTK 紧凑指标解析为近似整数。
--
-- value is the exact compact metric printed by RTK gain.
-- value 是 RTK gain 输出的精确紧凑指标。
-- Returns the expanded numeric value, or zero for an invalid metric.
-- 返回展开后的数值；指标无效时返回零。
local function parse_compact_metric(value)
    local amount, suffix = tostring(value or ""):match("^([%d%.]+)([KMG]?)$")
    amount = tonumber(amount)
    if amount == nil then
        return 0
    end
    local multipliers = { K = 1000, M = 1000000, G = 1000000000 }
    return amount * (multipliers[suffix] or 1)
end

-- Parse one RTK average-duration cell into milliseconds.
-- 把一个 RTK 平均耗时单元格解析为毫秒。
--
-- value is the exact duration text printed in the by-command table.
-- value 是按命令统计表输出的精确耗时文本。
-- Returns a millisecond value, or zero when the unit is not part of the RTK table contract.
-- 返回毫秒数；单位不属于 RTK 表格契约时返回零。
local function parse_duration_ms(value)
    local amount, unit = tostring(value or ""):match("^([%d%.]+)([a-z]+)$")
    amount = tonumber(amount)
    if amount == nil then
        return 0
    end
    if unit == "ms" then
        return amount
    end
    if unit == "s" then
        return amount * 1000
    end
    if unit == "m" then
        return amount * 60000
    end
    return 0
end

-- Parse the documented RTK gain by-command table into structured plugin statistics rows.
-- 把文档化的 RTK gain 按命令表解析为结构化插件统计行。
--
-- stdout is the complete decoded text from `rtk gain`.
-- stdout 是 `rtk gain` 的完整解码文本。
-- Returns an ordered JSON-array-compatible table.
-- 返回一个有序且兼容 JSON 数组的表。
local function parse_gain_by_command(stdout)
    local rows = {}
    local in_section = false
    for line in tostring(stdout or ""):gmatch("[^\r\n]+") do
        if line:match("^By Command%s*$") then
            in_section = true
        elseif in_section then
            local command, count, saved, savings_pct, duration = line:match(
                "^%s*%d+%.%s%s(.-)%s%s+(%d+)%s+([%d%.]+[KMG]?)%s+([%d%.]+)%%%s+([%d%.]+[a-z]+)"
            )
            if command ~= nil then
                table.insert(rows, {
                    command = command,
                    count = tonumber(count) or 0,
                    savedTokens = parse_compact_metric(saved),
                    avgSavingsPct = tonumber(savings_pct) or 0,
                    avgTimeMs = parse_duration_ms(duration),
                })
            end
        end
    end
    if #rows == 0 then
        return vulcan.json.decode("[]")
    end
    return rows
end

-- Load RTK token-saving statistics for the mounted plugin UI.
-- 为挂载的插件界面加载 RTK Token 节省统计。
--
-- args is the generic action call table and is unused for this read.
-- args 是通用 action 调用表，此读取操作不使用它。
-- Returns structured global statistics or an explicit unavailable result.
-- 返回结构化全局统计或明确的不可用结果。
function M.statistics_get(args)
    -- config controls telemetry for the statistics process as well.
    -- config 同样控制统计进程的遥测行为。
    local config = read_config()
    -- result is the bounded JSON export command result.
    -- result 是受边界约束的 JSON 导出命令结果。
    local result = run_rtk({ "gain", "--all", "--format", "json" }, nil, 5000, config)
    if not result.success then
        return success({
            available = false,
            summary = vulcan.json.decode("{}"),
            daily = vulcan.json.decode("[]"),
            weekly = vulcan.json.decode("[]"),
            monthly = vulcan.json.decode("[]"),
            byCommand = vulcan.json.decode("[]"),
            error = result.error or result.stderr or "RTK gain statistics failed",
        })
    end
    -- statistics is the exact RTK JSON export augmented by the separate documented by-command table.
    -- statistics 是精确 RTK JSON 导出，并补充独立的文档化按命令统计表。
    local statistics = vulcan.json.decode(result.stdout)
    -- by_command_result reads the human table because RTK's JSON export does not include this breakdown.
    -- by_command_result 读取人类可读表格，因为 RTK JSON 导出不包含此明细。
    local by_command_result = run_rtk({ "gain" }, nil, 5000, config)
    statistics.available = true
    statistics.byCommand = by_command_result.success
        and parse_gain_by_command(by_command_result.stdout)
        or vulcan.json.decode("[]")
    statistics.error = vulcan.json.decode("null")
    return success(statistics)
end

-- Verify that the bundled RTK binary can execute through the plugin process boundary.
-- 验证随包 RTK 二进制可以通过插件进程边界执行。
--
-- args is the generic action call table and is unused for this check.
-- args 是通用 action 调用表，此检查不使用它。
-- Returns availability, version text, and an optional error.
-- 返回可用性、版本文本与可选错误。
function M.self_check(args)
    -- result is the bounded RTK version command result.
    -- result 是受边界约束的 RTK 版本命令结果。
    local result = run_rtk({ "--version" }, nil, 3000, read_config())
    if result.success then
        return success({
            available = true,
            version = first_non_empty_line(result.stdout) or "rtk",
            error = vulcan.json.decode("null"),
        })
    end
    return success({
        available = false,
        version = "",
        error = result.error or result.stderr or "RTK self-check failed",
    })
end

-- Reject accidental generic tool dispatch because RTK exposes actions and hooks only.
-- 拒绝意外的通用工具分发，因为 RTK 仅暴露 action 与 Hook。
--
-- args contains an unsupported generic dispatch payload.
-- args 包含不受支持的通用分发载荷。
-- Returns no value because this path always raises an explicit error.
-- 不返回值，因为此路径始终抛出明确错误。
function M.dispatch(args)
    error("Vulcan RTK does not expose provider-visible plugin tools")
end

-- Close the stateless RTK plugin lease.
-- 关闭无状态 RTK 插件租约。
--
-- args contains the host shutdown payload and requires no plugin cleanup.
-- args 包含宿主关闭载荷，插件无需清理。
-- Returns a successful closed marker.
-- 返回成功关闭标记。
function M.shutdown(args)
    return success({ closed = true })
end

return M
