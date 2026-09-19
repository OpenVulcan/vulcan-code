# Exercise the exact isolated Windows tray owner, its Core browser child and its native Quit menu.
# 验收精确隔离的 Windows 托盘所有者、Core 浏览器子进程及原生退出菜单。
param(
    [Parameter(Mandatory = $true)][string]$ServiceExecutable,
    [Parameter(Mandatory = $true)][string]$RuntimeLayout,
    [Parameter(Mandatory = $true)][string]$AssetRoot,
    [switch]$InjectBridgeFailure,
    [switch]$InjectPersistenceFailure,
    # Wait for a person to open the exact PID-bound tray icon menu.
    # 等待人工展开与精确 PID 绑定的托盘图标菜单。
    [switch]$WaitForManualTrayMenu,
    # Let a person activate Quit Service after the exact test icon is confirmed.
    # 确认精确测试图标后，由人工点击“Quit Service”。
    [switch]$WaitForManualTrayQuit,
    # Bound each manual interaction so abandoned tests cannot leave a tray owner running.
    # 对每次人工交互设置上限，避免放弃测试后遗留托盘进程。
    [ValidateRange(30, 600)][int]$ManualWaitSeconds = 300
)
$ErrorActionPreference = 'Stop'
if ($WaitForManualTrayMenu -and $WaitForManualTrayQuit) {
    throw 'Select only one manual tray interaction mode'
}
# manualTrayInteraction controls observation-only handling of Explorer's tray overflow.
# manualTrayInteraction 控制对 Explorer 托盘溢出面板只观察、不投递点击的路径。
$manualTrayInteraction = $WaitForManualTrayMenu -or $WaitForManualTrayQuit
$workspace = (Resolve-Path -LiteralPath (Join-Path $PSScriptRoot '../../..')).Path
$targetRoot = (Resolve-Path -LiteralPath (Join-Path $workspace 'target')).Path
$servicePath = (Resolve-Path -LiteralPath $ServiceExecutable).Path
$layoutPath = (Resolve-Path -LiteralPath $RuntimeLayout).Path
$assetPath = (Resolve-Path -LiteralPath $AssetRoot).Path
$testRoot = Join-Path $targetRoot ('w11-tray-exit-' + [Guid]::NewGuid().ToString('N'))
New-Item -ItemType Directory -Path $testRoot | Out-Null
$env:VULCAN_RUNTIME_LAYOUT = $layoutPath
$env:VULCAN_CODE_OUTPUT_ROOT = $assetPath
if ($InjectBridgeFailure) { $env:VULCAN_TEST_HTTP_BRIDGE_FAIL_AFTER_ACK = '1' }
else { Remove-Item -LiteralPath 'Env:VULCAN_TEST_HTTP_BRIDGE_FAIL_AFTER_ACK' -ErrorAction SilentlyContinue }
$service = $null
$browserHostPid = 0
$chromiumPid = 0
$passed = $false
$persistMetadataPath = $null
$persistBackupPath = $null
$persistConversationId = $null
$persistDraft = 'W11_TRAY_PERSISTENCE_RETRY_DRAFT'
try {
    $service = Start-Process -FilePath $servicePath -ArgumentList @(
        '--service', "--test-root=$testRoot"
    ) -WindowStyle Hidden -PassThru -RedirectStandardOutput (Join-Path $testRoot 'stdout.log') `
        -RedirectStandardError (Join-Path $testRoot 'stderr.log')
    # PowerShell releases a redirected Start-Process handle before ExitCode is read unless
    # the handle is materialized while the child is alive; retain it for the exit assertion.
    # PowerShell 对重定向的 Start-Process 可能提前释放句柄，导致退出码为空；
    # 在子进程仍运行时先取句柄，以便准确断言退出码。
    $null = $service.Handle
    $discoveryPath = Join-Path $testRoot 'user-vulcan\code\webui-service.json'
    $deadline = [DateTime]::UtcNow.AddSeconds(60)
    $discovery = $null
    $expectedDiscoveryStatus = if ($InjectBridgeFailure) { 'failed' } else { 'ready' }
    while ([DateTime]::UtcNow -lt $deadline) {
        $service.Refresh()
        if ($service.HasExited) { throw "Isolated tray Core exited early: $($service.ExitCode)" }
        if (Test-Path -LiteralPath $discoveryPath -PathType Leaf) {
            try {
                $discovery = Get-Content -LiteralPath $discoveryPath -Raw | ConvertFrom-Json
                if ($discovery.status -eq $expectedDiscoveryStatus -and
                    $discovery.pid -eq $service.Id) { break }
            } catch { }
        }
        Start-Sleep -Milliseconds 100
    }
    if ($null -eq $discovery -or $discovery.status -ne $expectedDiscoveryStatus) {
        throw "Isolated tray Core never published $expectedDiscoveryStatus"
    }
    if (-not $InjectBridgeFailure) {
        $headers = @{ 'x-vulcan-service-token' = $discovery.serviceToken }
        if ($InjectPersistenceFailure) {
            # Materialize one session, then force its next durable draft write to fail at the
            # exact metadata destination. Only this newly generated test root is modified.
            # 先物化一份会话，再让其下次可靠草稿写入在精确元数据目标处失败；
            # 只修改本次新建的测试根。
            $baselineBody = @{ method = 'workbench.snapshot'; params = @{} } | ConvertTo-Json -Depth 6
            $baseline = Invoke-RestMethod -Uri ($discovery.baseUrl + '/rpc') -Method Post -Headers $headers `
                -ContentType 'application/json' -Body $baselineBody -TimeoutSec 15
            if (-not $baseline.ok -or -not $baseline.result.projects[0]) {
                throw 'Isolated Core has no project for persistence retry'
            }
            $projectId = [string]$baseline.result.projects[0].id
            $submissionBody = @{ method = 'service_submit_conversation_prompt'; params = @{
                clientSubmissionId = 'w11-tray-dirty-' + [Guid]::NewGuid().ToString('N')
                projectId = $projectId
                conversationId = $null
                draftSlotKey = "new:$projectId"
                prompt = 'W11 tray persistence retry fixture'
                modelSelection = @{ providerId = 'w11-unavailable'; modelId = 'w11-unavailable';
                    connectionId = 'w11-unavailable' }
            } } | ConvertTo-Json -Depth 8
            $null = Invoke-RestMethod -Uri ($discovery.baseUrl + '/rpc') -Method Post -Headers $headers `
                -ContentType 'application/json' -Body $submissionBody -TimeoutSec 15
            $deadline = [DateTime]::UtcNow.AddSeconds(20)
            while ([DateTime]::UtcNow -lt $deadline) {
                $snapshot = Invoke-RestMethod -Uri ($discovery.baseUrl + '/rpc') -Method Post -Headers $headers `
                    -ContentType 'application/json' -Body $baselineBody -TimeoutSec 15
                $created = @($snapshot.result.conversations | Where-Object {
                    $_.projectId -eq $projectId -and
                    $_.id -notin @($baseline.result.conversations | ForEach-Object { $_.id })
                })
                if ($created.Count -eq 1) { $persistConversationId = [string]$created[0].id; break }
                Start-Sleep -Milliseconds 100
            }
            if (-not $persistConversationId) { throw 'Persistence retry conversation was not materialized' }
            $metadataFiles = @(Get-ChildItem -LiteralPath $testRoot -Filter metadata.json -File -Recurse |
                Where-Object { $_.Directory.Name.EndsWith("-$persistConversationId") })
            if ($metadataFiles.Count -ne 1) { throw 'Exact persistence retry metadata file was not found' }
            $persistMetadataPath = $metadataFiles[0].FullName
            $persistBackupPath = $persistMetadataPath + '.w11-backup'
            Move-Item -LiteralPath $persistMetadataPath -Destination $persistBackupPath
            New-Item -ItemType Directory -Path $persistMetadataPath | Out-Null
            $draftBody = @{ method = 'service_set_session_draft_v1'; params = @{
                sessionId = $persistConversationId; text = $persistDraft; attachments = @()
            } } | ConvertTo-Json -Depth 7
            $draftWrite = Invoke-RestMethod -Uri ($discovery.baseUrl + '/rpc') -Method Post -Headers $headers `
                -ContentType 'application/json' -Body $draftBody -TimeoutSec 15
            if ($draftWrite.ok) { throw 'Blocked metadata destination did not reject durable draft write' }
        }
        $body = @{ method = 'browser.session.create'; params = @{
            ownerSessionId = 'w11-tray-exit'; profileMode = 'ephemeral'
        } } | ConvertTo-Json -Depth 6
        $created = Invoke-RestMethod -Uri ($discovery.baseUrl + '/rpc') -Method Post -Headers $headers `
            -ContentType 'application/json' -Body $body -TimeoutSec 15
        if (-not $created.ok -or $created.result.hostState -ne 'running') {
            throw ('Core browser failed to start: ' + ($created.error | ConvertTo-Json -Compress))
        }
        $deadline = [DateTime]::UtcNow.AddSeconds(15)
        while ([DateTime]::UtcNow -lt $deadline) {
            $hostProcess = Get-CimInstance Win32_Process | Where-Object {
                $_.ParentProcessId -eq $service.Id -and $_.CommandLine -match 'chromium-host[\\/]main\.js'
            } | Select-Object -First 1
            if ($hostProcess) { $browserHostPid = $hostProcess.ProcessId; break }
            Start-Sleep -Milliseconds 100
        }
        if ($browserHostPid -eq 0) { throw 'Core browser host child was not observed' }
        $chromium = Get-CimInstance Win32_Process | Where-Object {
            $_.ParentProcessId -eq $browserHostPid -and $_.Name -eq 'chrome.exe'
        } | Select-Object -First 1
        if (-not $chromium) { throw 'Core Chromium child was not observed' }
        $chromiumPid = $chromium.ProcessId
    }

    Add-Type -AssemblyName UIAutomationClient
    $rootElement = [System.Windows.Automation.AutomationElement]::RootElement
    $treeChildren = [System.Windows.Automation.TreeScope]::Children
    $treeDescendants = [System.Windows.Automation.TreeScope]::Descendants
    $nameProperty = [System.Windows.Automation.AutomationElement]::NameProperty
    $classProperty = [System.Windows.Automation.AutomationElement]::ClassNameProperty
    $popupCondition = [System.Windows.Automation.PropertyCondition]::new(
        $classProperty, 'TopLevelWindowForOverflowXamlIsland')
    $attemptCount = if ($InjectPersistenceFailure) { 2 } else { 1 }
    for ($attempt = 1; $attempt -le $attemptCount; $attempt++) {
    Write-Host "Tray Quit attempt $attempt/$attemptCount for isolated Core $($service.Id)"
    if ($manualTrayInteraction) {
        Write-Host "MANUAL_TRAY_READY pid=$($service.Id) attempt=$attempt/$attemptCount tooltip='Vulcan Code Service Test $($service.Id)'"
    }
    $popup = $rootElement.FindFirst($treeChildren, $popupCondition)
    # Explorer can retain a UIA element after closing the overflow. Reopen it only when the
    # current element is missing or actually offscreen, especially on a second Quit attempt.
    # Explorer 关闭溢出面板后仍可能保留无障碍元素；仅在元素缺失或实际离屏时重新打开，
    # 尤其避免第二次退出操作误点已关闭的面板。
    if ($popup) {
        try { if ($popup.Current.IsOffscreen) { $popup = $null } }
        catch { $popup = $null }
    }
    # In the manual mode, Explorer's overflow is opened by the tester; UIA only observes it.
    # 人工模式下由测试者展开 Explorer 托盘；UIA 只观察，不向其投递点击。
    if (-not $popup -and -not $manualTrayInteraction) {
        $taskbar = $rootElement.FindFirst($treeChildren,
            [System.Windows.Automation.PropertyCondition]::new($classProperty, 'Shell_TrayWnd'))
        if (-not $taskbar) { throw 'Windows taskbar was not found' }
        $overflow = $taskbar.FindFirst($treeDescendants,
            [System.Windows.Automation.PropertyCondition]::new($nameProperty, '显示隐藏的图标'))
        if (-not $overflow) { throw 'System tray overflow button was not found' }
        $overflow.GetCurrentPattern([System.Windows.Automation.InvokePattern]::Pattern).Invoke()
        Start-Sleep -Milliseconds 350
        $popup = $rootElement.FindFirst($treeChildren, $popupCondition)
    }
    $exactIcon = 'Vulcan Code Service Test ' + $service.Id
    $icon = $null
    $lastTrayScanError = $null
    $deadline = [DateTime]::UtcNow.AddSeconds($(if ($manualTrayInteraction) { $ManualWaitSeconds } else { 15 }))
    # Explorer can replace the overflow accessibility tree while an icon appears; rescan a
    # fresh popup handle for a bounded time and still require the exact test PID tooltip.
    # 图标出现时 Explorer 可能替换通知区域无障碍树；在有限时间内重新获取弹窗句柄，
    # 同时仍要求提示文字包含测试进程的精确 PID。
    while ([DateTime]::UtcNow -lt $deadline) {
        try {
            $popup = $rootElement.FindFirst($treeChildren, $popupCondition)
            if ($popup -and -not $popup.Current.IsOffscreen) {
                $icons = $popup.FindAll($treeDescendants,
                    [System.Windows.Automation.PropertyCondition]::new($nameProperty, $exactIcon))
                $icon = $icons | Where-Object {
                    $_.Current.ControlType.ProgrammaticName -eq 'ControlType.Button' -and
                    -not $_.Current.IsOffscreen
                } | Select-Object -First 1
            }
            if ($icon) { break }
        } catch {
            $lastTrayScanError = $_.Exception.Message
        }
        Start-Sleep -Milliseconds 200
    }
    if (-not $icon) { throw "Exact isolated tray icon was not found: $exactIcon; last scan error: $lastTrayScanError" }
    # The Explorer Invoke provider can return without opening the menu. Use a native click only
    # after the visible button's exact PID tooltip and nonempty screen bounds are rechecked.
    # Explorer 的 Invoke 提供器有时返回却不弹出菜单；仅在重新确认可见按钮的精确 PID
    # 提示及非空屏幕边界后执行原生点击。
    if ($icon.Current.Name -ne $exactIcon) { throw 'Tray icon identity changed before click' }
    $bounds = $icon.Current.BoundingRectangle
    if ($bounds.Width -le 0 -or $bounds.Height -le 0) { throw 'Exact tray icon has no visible bounds' }
    Write-Host "Exact tray icon visible at $bounds for attempt $attempt"
    if ($manualTrayInteraction) {
        Write-Host "MANUAL_TRAY_ICON_FOUND pid=$($service.Id) attempt=$attempt/$attemptCount"
    }
    # A toast or another shell surface may cover a visible accessibility element. Resolve the
    # actual hit target before moving the pointer so a missed second click is diagnosable.
    # 通知或其他系统表面可能遮盖无障碍树中可见的图标；移动指针前解析实际命中目标，
    # 以便诊断第二次点击为何未打开菜单。
    $hitPoint = [System.Windows.Point]::new(
        [double]($bounds.Left + $bounds.Width / 2), [double]($bounds.Top + $bounds.Height / 2))
    $hitElement = [System.Windows.Automation.AutomationElement]::FromPoint($hitPoint)
    Write-Host "Tray hit target attempt $attempt`: name='$($hitElement.Current.Name)' class='$($hitElement.Current.ClassName)' pid=$($hitElement.Current.ProcessId)"
    # Windows 11 may return the XAML overflow parent from FromPoint even when the exact
    # PID-named child icon owns these visible bounds. A parent hit alone is not blank space.
    # Windows 11 即使精确 PID 子图标占据可见矩形，FromPoint 也可能返回 XAML 父容器；
    # 仅命中父容器并不能证明该坐标是空白。
    if ($WaitForManualTrayQuit) {
        Write-Host "MANUAL_TRAY_QUIT_READY pid=$($service.Id) attempt=$attempt/$attemptCount"
    } else {
    Add-Type -AssemblyName System.Windows.Forms
    # The generated Pointer class sends down/up to the exact visible test icon.
    # 生成的 Pointer 类向精确可见的测试图标发送按下与释放事件。
    if (-not ('W11Tray.Pointer' -as [type])) { Add-Type -Namespace W11Tray -Name Pointer -MemberDefinition @'
        // Sends one native pointer transition; flags choose left-down or left-up.
        // 发送一次原生指针转换；标志选择左键按下或释放。
        [System.Runtime.InteropServices.DllImport("user32.dll")]
        public static extern void mouse_event(uint flags, uint dx, uint dy, uint data, uint extraInfo);
'@
    }
    if (-not $WaitForManualTrayMenu) {
        $previousCursor = [System.Windows.Forms.Cursor]::Position
        try {
            [System.Windows.Forms.Cursor]::Position = [System.Drawing.Point]::new(
                [int]($bounds.Left + $bounds.Width / 2), [int]($bounds.Top + $bounds.Height / 2))
            Start-Sleep -Milliseconds 80
            [W11Tray.Pointer]::mouse_event(0x0002, 0, 0, 0, 0)
            [W11Tray.Pointer]::mouse_event(0x0004, 0, 0, 0, 0)
            Start-Sleep -Milliseconds 300
        } finally {
            [System.Windows.Forms.Cursor]::Position = $previousCursor
        }
    }
    # Native tray menus may appear after the shell animation rather than immediately after Invoke.
    # 原生托盘菜单可能在系统动画结束后才出现，不能假设 Invoke 返回时已可见。
    $quitItem = $null
    $statusItem = $null
    $expectedMenuStatus = if ($InjectBridgeFailure) {
        'Service: bridge failed (injected post-acknowledgement HTTP bridge failure)'
    } elseif ($InjectPersistenceFailure -and $attempt -eq 2) {
        'Service: recovery failed; restart required'
    } else { 'Service: ready' }
    $deadline = [DateTime]::UtcNow.AddSeconds($(if ($WaitForManualTrayMenu) { $ManualWaitSeconds } else { 15 }))
    $rightClickFallbackAt = [DateTime]::UtcNow.AddSeconds(4)
    $rightClickFallbackSent = $false
    while ([DateTime]::UtcNow -lt $deadline) {
        try {
            $quitItem = $rootElement.FindFirst($treeDescendants,
                [System.Windows.Automation.PropertyCondition]::new($nameProperty, 'Quit Service'))
            $statusItem = $rootElement.FindFirst($treeDescendants,
                [System.Windows.Automation.PropertyCondition]::new($nameProperty, $expectedMenuStatus))
            if ($quitItem -and $quitItem.Current.ProcessId -ne $service.Id) { $quitItem = $null }
            if ($statusItem -and $statusItem.Current.ProcessId -ne $service.Id) { $statusItem = $null }
            if ($quitItem -and $statusItem) { break }
        } catch {
            $quitItem = $null
            $statusItem = $null
        }
        if (-not $WaitForManualTrayMenu -and -not $rightClickFallbackSent -and
            [DateTime]::UtcNow -ge $rightClickFallbackAt) {
            # Some Explorer sessions ignore the first left click; use the exact same visible
            # PID-bound icon's native context click before concluding the tray is unavailable.
            # 某些 Explorer 会话忽略首次左键；判断托盘不可用前，对同一可见且 PID 精确匹配的
            # 图标执行原生右键菜单点击。
            $rightClickFallbackSent = $true
            $previousCursor = [System.Windows.Forms.Cursor]::Position
            try {
                [System.Windows.Forms.Cursor]::Position = [System.Drawing.Point]::new(
                    [int]($bounds.Left + $bounds.Width / 2), [int]($bounds.Top + $bounds.Height / 2))
                Start-Sleep -Milliseconds 80
                [W11Tray.Pointer]::mouse_event(0x0008, 0, 0, 0, 0)
                [W11Tray.Pointer]::mouse_event(0x0010, 0, 0, 0, 0)
                Start-Sleep -Milliseconds 300
            } finally {
                [System.Windows.Forms.Cursor]::Position = $previousCursor
            }
        }
        Start-Sleep -Milliseconds 200
    }
    if (-not $quitItem -or -not $statusItem) {
        $menus = $rootElement.FindAll($treeDescendants,
            [System.Windows.Automation.Condition]::TrueCondition) | Where-Object {
                $_.Current.ProcessId -eq $service.Id -and $_.Current.Name -match 'Service|Quit'
            } | ForEach-Object { "$($_.Current.Name) pid=$($_.Current.ProcessId)" }
        throw ('Exact tray status menu was not visible: ' + ($menus -join ', '))
    }
    if ($WaitForManualTrayMenu) {
        Write-Host "MANUAL_TRAY_MENU_CONFIRMED pid=$($service.Id) attempt=$attempt status='$expectedMenuStatus'"
    }
    # UIAutomation Invoke can acknowledge a native menu item without dispatching its command.
    # After exact owner and label checks, click its visible rectangle with the same native pointer.
    # UIAutomation Invoke 可能确认了原生菜单项却未投递命令；核对精确所属进程和名称后，
    # 使用同一原生指针点击可见菜单项矩形。
    $quitBounds = $quitItem.Current.BoundingRectangle
    if ($quitBounds.Width -le 0 -or $quitBounds.Height -le 0) {
        throw 'Exact Quit Service menu item has no visible bounds'
    }
    # quitPoint is the current menu item's screen center for hit testing.
    # quitPoint 是当前菜单项的屏幕中心点，用于命中测试。
    $quitPoint = [System.Windows.Point]::new(
        [double]($quitBounds.Left + $quitBounds.Width / 2),
        [double]($quitBounds.Top + $quitBounds.Height / 2))
    # quitHit identifies the actual surface under the pointer before dispatch.
    # quitHit 在投递点击前标识指针下实际可点击的界面。
    $quitHit = [System.Windows.Automation.AutomationElement]::FromPoint($quitPoint)
    if ($quitHit.Current.Name -eq 'Quit Service' -and $quitHit.Current.ProcessId -eq $service.Id) {
        $previousCursor = [System.Windows.Forms.Cursor]::Position
        try {
            [System.Windows.Forms.Cursor]::Position = [System.Drawing.Point]::new(
                [int]($quitBounds.Left + $quitBounds.Width / 2),
                [int]($quitBounds.Top + $quitBounds.Height / 2))
            Start-Sleep -Milliseconds 80
            [W11Tray.Pointer]::mouse_event(0x0002, 0, 0, 0, 0)
            [W11Tray.Pointer]::mouse_event(0x0004, 0, 0, 0, 0)
            Start-Sleep -Milliseconds 300
        } finally {
            [System.Windows.Forms.Cursor]::Position = $previousCursor
        }
    } else {
        $quitItem.GetCurrentPattern([System.Windows.Automation.InvokePattern]::Pattern).Invoke()
        Start-Sleep -Milliseconds 300
    }
    }
    if ($InjectPersistenceFailure -and $attempt -eq 1) {
        # A failed physical flush must preserve this PID and report recovery failure; only
        # then restore the exact metadata file and retry the same native tray command.
        # 物理刷盘失败后必须保留同一 PID 并报告恢复失败；随后才恢复精确元数据文件，
        # 并重试同一个原生托盘命令。
        $deadline = [DateTime]::UtcNow.AddSeconds($(if ($WaitForManualTrayQuit) { $ManualWaitSeconds } else { 20 }))
        $recoveryFailed = $false
        while ([DateTime]::UtcNow -lt $deadline) {
            $service.Refresh()
            if ($service.HasExited) { throw 'Core exited after blocked dirty-session flush' }
            $status = Invoke-RestMethod -Uri ($discovery.baseUrl + '/v1/service/status') `
                -Method Get -Headers $headers -TimeoutSec 5
            if ($status.lifecycle -eq 'recoveryFailed') { $recoveryFailed = $true; break }
            Start-Sleep -Milliseconds 100
        }
        if (-not $recoveryFailed) { throw 'Core did not report failed shutdown recovery' }
        Remove-Item -LiteralPath $persistMetadataPath
        Move-Item -LiteralPath $persistBackupPath -Destination $persistMetadataPath
        Start-Sleep -Milliseconds 350
        continue
    }
    }
    # exitWaitMs permits the second physical menu action while preserving the automatic limit.
    # exitWaitMs 为第二次物理菜单操作留出时间，同时保留自动模式的短时上限。
    $exitWaitMs = if ($WaitForManualTrayQuit) { $ManualWaitSeconds * 1000 } else { 30000 }
    if (-not $service.WaitForExit($exitWaitMs)) { throw 'Tray Quit did not exit Core within the allowed wait' }
    if ($service.ExitCode -ne 0) { throw "Tray Quit returned exit $($service.ExitCode)" }
    # Browser descendants can finish a short asynchronous close after the owner exits.
    # 浏览器后代可能在所有者退出后再用短暂时间完成异步关闭。
    $deadline = [DateTime]::UtcNow.AddSeconds(10)
    while ([DateTime]::UtcNow -lt $deadline) {
        if (-not (Get-Process -Id $browserHostPid -ErrorAction SilentlyContinue) -and
            -not (Get-Process -Id $chromiumPid -ErrorAction SilentlyContinue)) { break }
        Start-Sleep -Milliseconds 100
    }
    if ($browserHostPid -ne 0 -and (Get-Process -Id $browserHostPid -ErrorAction SilentlyContinue)) {
        throw 'Core browser host survived tray Quit'
    }
    if ($chromiumPid -ne 0 -and (Get-Process -Id $chromiumPid -ErrorAction SilentlyContinue)) {
        throw 'Core Chromium survived tray Quit'
    }
    if (Test-Path -LiteralPath $discoveryPath) { throw 'Tray Quit left a ready discovery record' }
    if ($InjectPersistenceFailure) {
        $publishedMetadata = Get-Content -LiteralPath $persistMetadataPath -Raw
        if (-not $publishedMetadata.Contains($persistDraft)) {
            throw 'Retried tray shutdown did not persist the dirty draft'
        }
    }
    $tcp = [System.Net.Sockets.TcpClient]::new()
    try {
        $connected = $tcp.ConnectAsync('127.0.0.1', [int]$discovery.port).Wait(1000) -and $tcp.Connected
        if ($connected) { throw 'Core HTTP port survived tray Quit' }
    } finally {
        $tcp.Dispose()
    }
    $passed = $true
    @{ result = 'passed'; corePid = $service.Id; browserHostPid = $browserHostPid;
        chromiumPid = $chromiumPid; coreExitCode = $service.ExitCode;
        discoveryRemoved = $true; portClosed = $true;
        bridgeFailureShown = $InjectBridgeFailure.IsPresent;
        dirtyPersistenceRetried = $InjectPersistenceFailure.IsPresent;
        dirtyConversationId = $persistConversationId } | ConvertTo-Json -Compress
} finally {
    if ($service) {
        $service.Refresh()
        if (-not $service.HasExited) { $service.Kill(); $service.WaitForExit(10000) | Out-Null }
    }
    # Only the generated direct child of this workspace target may be deleted.
    # 仅允许删除此工作区 target 下由本脚本生成的直接子目录。
    $resolved = (Resolve-Path -LiteralPath $testRoot).Path
    if ([IO.Path]::GetDirectoryName($resolved) -ne $targetRoot -or
        -not [IO.Path]::GetFileName($resolved).StartsWith('w11-tray-exit-')) {
        throw 'Refusing cleanup outside the exact tray test root'
    }
    if ($passed) { Remove-Item -LiteralPath $resolved -Recurse -Force }
    else { Write-Warning "Tray acceptance failed; retained isolated evidence at $resolved" }
}
