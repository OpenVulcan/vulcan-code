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
    -- environment keeps bundled CLI output plain and suppresses standalone-agent installation hints.
    -- environment 保持随包 CLI 输出为纯文本，并抑制独立代理的安装提示。
    local environment = {
        NO_COLOR = "1",
        RTK_SUPPRESS_HOOK_WARNING = "1",
    }
    if config.telemetryDisabled then
        environment.RTK_TELEMETRY_DISABLED = "1"
    end
    return environment
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

-- Extract the first non-empty version line from the RTK self-check.
-- 从 RTK 自检中提取第一条非空版本行。
--
-- stdout is the exact decoded process stdout.
-- stdout 是进程精确解码后的标准输出。
-- Returns a trimmed version line or nil when the CLI emitted no version.
-- 返回去空白后的版本行；CLI 未输出版本时返回 nil。
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
    -- rows preserves the upstream ranking without dropping negative weighted savings.
    -- rows 保留上游排名，不丢弃加权节省率为负的记录。
    local rows = {}
    -- in_section begins only at RTK's by-command table rather than its summary or recall sections.
    -- in_section 仅在 RTK 按命令表开始，不匹配汇总或回读区块。
    local in_section = false
    for line in tostring(stdout or ""):gmatch("[^\r\n]+") do
        if line:match("^By Command%s*$") then
            in_section = true
        elseif in_section then
            -- These cells follow RTK 1.0's Command/Count/Saved/Total%/Time columns.
            -- 这些单元格遵循 RTK 1.0 的 Command/Count/Saved/Total%/Time 列。
            local command, count, saved, savings_pct, duration = line:match(
                "^%s*%d+%.%s%s(.-)%s%s+(%d+)%s+([%d%.]+[KMG]?)%s+([%-]?[%d%.]+)%%%s+([%d%.]+[a-z]+)"
            )
            if command ~= nil then
                table.insert(rows, {
                    command = command,
                    count = tonumber(count),
                    savedTokens = parse_compact_metric(saved),
                    savingsPct = tonumber(savings_pct),
                    avgTimeMs = parse_duration_ms(duration),
                })
            end
        end
    end
    if #rows == 0 then
        return vulcan.json.array()
    end
    return rows
end

-- Bound for each RTK statistics process; `rtk gain` rescans the whole history, so it slows as data grows.
-- The `statistics-get` action timeout in plugin.yaml must cover two of these run back to back.
-- 每个 RTK 统计进程的超时；`rtk gain` 会重新扫描全部历史，数据越多越慢。
-- plugin.yaml 中 `statistics-get` 的 action 超时必须容纳两次连续执行。
local STATISTICS_PROCESS_TIMEOUT_MS = 60000

-- Prepare only a durable UI revision; RTK continues to own its actual history and recall databases.
-- 仅准备持久界面修订号；实际历史库和回读库继续由 RTK 持有。
-- Takes no parameters and returns after the host validates the initial private schema.
-- 不接收参数，在宿主校验初始私有 Schema 后返回。
local function prepare_statistics_revision()
    -- The revision stays inside JavaScript's exact integer range because the WebUI rejects unsafe event revisions.
    -- 修订号保持在 JavaScript 的精确整数范围内，因为 WebUI 会拒绝不安全的事件修订号。
    vulcan.host.call("vulcan.plugin.storage.sqlite.migrate", {
        version = 1,
        plugin_version = "0.1.2",
        applied_at_ms = 0,
        statements = {
            [[CREATE TABLE rtk_ui_revision (
                singleton_id INTEGER PRIMARY KEY CHECK (singleton_id = 1),
                revision INTEGER NOT NULL CHECK (revision >= 0 AND revision <= 9007199254740991)
            )]],
            "INSERT INTO rtk_ui_revision(singleton_id, revision) VALUES (1, 0)",
        },
    })
end

-- Advance the durable data revision and invalidate mounted statistics without disclosing command history.
-- 递增持久数据修订号并使已挂载统计失效，不泄露命令历史。
-- Takes no parameters and returns after the existing host event boundary accepts the revision.
-- 不接收参数，在既有宿主事件边界接受修订号后返回。
local function publish_statistics_revision()
    vulcan.host.call("vulcan.plugin.storage.sqlite.execute", {
        sql = "UPDATE rtk_ui_revision SET revision = revision + 1 WHERE singleton_id = 1",
        params = vulcan.json.array(),
    })
    -- rows must contain the one schema-owned singleton; no clock or process-local counter can replace it.
    -- rows 必须包含 Schema 持有的唯一单例；时钟或进程局部计数器不能替代它。
    local rows = vulcan.host.call("vulcan.plugin.storage.sqlite.query", {
        sql = "SELECT revision FROM rtk_ui_revision WHERE singleton_id = 1",
        params = vulcan.json.array(),
    }).rows
    if #rows ~= 1 or type(rows[1].revision) ~= "number" then
        error("RTK statistics revision is missing or invalid")
    end
    vulcan.host.call("vulcan.plugin.ui.invalidate", {
        topic = "rtk.statistics",
        revision = rows[1].revision,
    })
end

-- Clear the current RTK statistics through its official non-interactive reset command.
-- 通过 RTK 官方非交互重置命令清空当前统计。
-- args is the generic UI action call table; this mutation accepts no user-supplied arguments.
-- args 是通用界面 Action 调用表；此变更不接受用户提供的参数。
-- Returns a cleared result only after reset and UI invalidation succeed; failures remain explicit.
-- 仅在重置与界面失效通知成功后返回已清空结果；失败保持显式。
function M.statistics_clear(args)
    -- config preserves the user's routing and telemetry settings throughout the reset.
    -- config 在重置期间保留用户的路由和遥测设置。
    local config = read_config()
    -- Validate private storage before changing RTK history, avoiding a reset when revision storage is unavailable.
    -- 修改 RTK 历史前先校验私有存储，避免修订号存储不可用时仍执行重置。
    prepare_statistics_revision()
    -- result comes from the same declared binary and default statistics scope as statistics_get.
    -- result 来自与 statistics_get 相同的已声明二进制和默认统计作用域。
    local result = run_rtk({ "gain", "--reset", "--yes" }, nil, STATISTICS_PROCESS_TIMEOUT_MS, config)
    if not result.success then
        error("RTK statistics reset failed (exit " .. tostring(result.code) .. "): "
            .. tostring(result.error or result.stderr), 0)
    end
    -- published and reason distinguish a completed reset from a later notification failure.
    -- published 与 reason 区分已完成的重置和后续通知失败。
    local published, reason = pcall(publish_statistics_revision)
    if not published then
        error("RTK statistics were cleared, but refreshing the statistics view failed: " .. tostring(reason), 0)
    end
    return success({ cleared = true })
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
    -- result is the bounded JSON export, including the upstream oversized_excluded summary field.
    -- result 是有界 JSON 导出，包含上游汇总字段 oversized_excluded。
    local result = run_rtk({ "gain", "--all", "--format", "json" }, nil, STATISTICS_PROCESS_TIMEOUT_MS, config)
    if not result.success then
        return success({
            available = false,
            summary = vulcan.json.object(),
            daily = vulcan.json.array(),
            weekly = vulcan.json.array(),
            monthly = vulcan.json.array(),
            byCommand = vulcan.json.array(),
            error = result.error or result.stderr or "RTK gain statistics failed",
        })
    end
    -- statistics retains RTK's own summary and period accounting without inventing replacement totals.
    -- statistics 保留 RTK 自身的汇总和周期统计口径，不自行补造替代总量。
    local statistics = vulcan.json.decode(result.stdout)
    -- RTK 1.0 omits command rows from JSON; --text explicitly selects its supported static table.
    -- RTK 1.0 的 JSON 不含命令明细；--text 显式选择其受支持的静态表格。
    local by_command_result = run_rtk({ "gain", "--text" }, nil, STATISTICS_PROCESS_TIMEOUT_MS, config)
    statistics.available = true
    statistics.byCommand = by_command_result.success
        and parse_gain_by_command(by_command_result.stdout)
        or vulcan.json.array()
    statistics.error = vulcan.json.null
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
            error = vulcan.json.null,
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
