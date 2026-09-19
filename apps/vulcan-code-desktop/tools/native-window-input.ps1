# Native pointer input verifies actual Windows hit testing on an isolated Electron window.
# 原生指针输入用于验证隔离 Electron 窗口的真实 Windows 命中行为。
param(
    # WindowHandle is the exact HWND returned by the isolated Electron process.
    # WindowHandle 是隔离 Electron 进程返回的精确窗口句柄。
    [Parameter(Mandatory = $true)][long]$WindowHandle,
    # Action selects one bounded native mouse gesture.
    # Action 选择一种有界的原生鼠标手势。
    [Parameter(Mandatory = $true)][ValidateSet('Drag', 'DoubleClick', 'Click', 'Probe')][string]$Action,
    # FromX and FromY identify the physical screen point whose root window must match.
    # FromX 与 FromY 标识必须命中目标根窗口的物理屏幕坐标。
    [Parameter(Mandatory = $true)][int]$FromX,
    [Parameter(Mandatory = $true)][int]$FromY,
    # ToX and ToY identify the physical destination for a drag.
    # ToX 与 ToY 标识拖动使用的物理目标坐标。
    [int]$ToX = 0,
    [int]$ToY = 0
)

Add-Type -TypeDefinition @'
using System;
using System.Runtime.InteropServices;

// NativePoint carries one physical desktop coordinate into Win32 hit testing.
// NativePoint 将物理桌面坐标传给 Win32 命中测试。
public struct NativePoint {
    // X is the horizontal physical coordinate.
    // X 是水平物理坐标。
    public int X;
    // Y is the vertical physical coordinate.
    // Y 是垂直物理坐标。
    public int Y;
}

// NativeRect carries the operating system's outer window rectangle.
// NativeRect 保存操作系统报告的窗口外边界。
public struct NativeRect {
    // Left and Top identify the outer origin.
    // Left 和 Top 标识外边界原点。
    public int Left;
    public int Top;
    // Right and Bottom identify the exclusive outer edge.
    // Right 和 Bottom 标识不包含在内的外边界终点。
    public int Right;
    public int Bottom;
}

// NativeWindowInput exposes only the Win32 calls required by the isolated acceptance gesture.
// NativeWindowInput 仅暴露隔离验收手势所需的 Win32 调用。
public static class NativeWindowInput {
    // SetProcessDPIAware aligns coordinates with physical screen pixels.
    // SetProcessDPIAware 让坐标与物理屏幕像素对齐。
    [DllImport("user32.dll")]
    public static extern bool SetProcessDPIAware();

    // SetForegroundWindow asks Windows to focus the exact test window.
    // SetForegroundWindow 请求 Windows 聚焦精确的测试窗口。
    [DllImport("user32.dll")]
    public static extern bool SetForegroundWindow(IntPtr handle);

    // SetCursorPos moves the real system pointer to one physical screen point.
    // SetCursorPos 将真实系统指针移动到一个物理屏幕坐标。
    [DllImport("user32.dll")]
    public static extern bool SetCursorPos(int x, int y);

    // WindowFromPoint resolves the actual window receiving pointer input.
    // WindowFromPoint 解析实际接收指针输入的窗口。
    [DllImport("user32.dll")]
    public static extern IntPtr WindowFromPoint(NativePoint point);

    // GetAncestor resolves the top-level window of a renderer child window.
    // GetAncestor 解析渲染子窗口所属的顶层窗口。
    [DllImport("user32.dll")]
    public static extern IntPtr GetAncestor(IntPtr handle, uint flags);

    // SendMessage probes the native hit-test result at the verified screen point.
    // SendMessage 探查已验证屏幕坐标的原生命中测试结果。
    [DllImport("user32.dll")]
    public static extern IntPtr SendMessage(IntPtr handle, uint message, IntPtr wParam, IntPtr lParam);

    // GetWindowRect reports the actual native outer bounds for DPI comparison.
    // GetWindowRect 报告原生外边界，供 DPI 坐标对照。
    [DllImport("user32.dll")]
    public static extern bool GetWindowRect(IntPtr handle, out NativeRect rectangle);

    // GetWindowLongPtr reports native style flags, including WS_THICKFRAME.
    // GetWindowLongPtr 报告包括 WS_THICKFRAME 在内的原生样式标志。
    [DllImport("user32.dll", EntryPoint = "GetWindowLongPtrW")]
    public static extern IntPtr GetWindowLongPtr(IntPtr handle, int index);

    // mouse_event sends a real down or up event through the operating system input queue.
    // mouse_event 通过操作系统输入队列发送真实的按下或释放事件。
    [DllImport("user32.dll")]
    public static extern void mouse_event(uint flags, uint x, uint y, uint data, UIntPtr extraInfo);
}
'@

# targetWindow is the exact native top-level window the gesture may touch.
# targetWindow 是手势唯一允许触及的原生顶层窗口。
$targetWindow = [IntPtr]::new($WindowHandle)
[void][NativeWindowInput]::SetProcessDPIAware()
[void][NativeWindowInput]::SetForegroundWindow($targetWindow)
if (-not [NativeWindowInput]::SetCursorPos($FromX, $FromY)) {
    throw 'Could not position the native mouse pointer.'
}
Start-Sleep -Milliseconds 100
# hitWindow verifies the current physical pointer position belongs to the requested test HWND.
# hitWindow 验证当前物理指针位置确实属于指定的测试窗口句柄。
$hitWindow = [NativeWindowInput]::WindowFromPoint([NativePoint]@{ X = $FromX; Y = $FromY })
$rootWindow = [NativeWindowInput]::GetAncestor($hitWindow, 2)
if ($rootWindow -ne $targetWindow) {
    throw "Native pointer target mismatch: expected $WindowHandle, received $($rootWindow.ToInt64())."
}
if ($Action -eq 'Probe') {
    # hitParameter packs physical screen coordinates for WM_NCHITTEST.
    # hitParameter 将物理屏幕坐标打包给 WM_NCHITTEST。
    $hitParameter = [long]((($FromY -band 0xffff) -shl 16) -bor ($FromX -band 0xffff))
    $hitResult = [NativeWindowInput]::SendMessage($targetWindow, 0x0084,
        [IntPtr]::Zero, [IntPtr]::new($hitParameter))
    $rectangle = [NativeRect]::new()
    if (-not [NativeWindowInput]::GetWindowRect($targetWindow, [ref]$rectangle)) {
        throw 'Could not read the native window rectangle.'
    }
    $style = [NativeWindowInput]::GetWindowLongPtr($targetWindow, -16).ToInt64()
    Write-Output (@{ hit = $hitResult.ToInt64(); rect = @{
        left = $rectangle.Left; top = $rectangle.Top;
        right = $rectangle.Right; bottom = $rectangle.Bottom
    }; thickFrame = (($style -band 0x40000) -ne 0) } | ConvertTo-Json -Compress)
    return
}

# leftDown and leftUp are the Win32 flags for actual mouse button transitions.
# leftDown 与 leftUp 是真实鼠标按键状态转移的 Win32 标志。
$leftDown = [uint32]0x0002
$leftUp = [uint32]0x0004
if ($Action -eq 'Drag') {
    [NativeWindowInput]::mouse_event($leftDown, 0, 0, 0, [UIntPtr]::Zero)
    # steps keeps the pointer held long enough for the native non-client drag loop.
    # steps 保持足够长的按下移动过程，使原生非客户区拖动循环能够接收。
    $steps = [int][math]::Min(80, [math]::Max(16,
        [math]::Ceiling([math]::Max([math]::Abs($ToX - $FromX), [math]::Abs($ToY - $FromY)) / 80)))
    for ($step = 1; $step -le $steps; $step++) {
        $x = [int][math]::Round($FromX + ($ToX - $FromX) * $step / $steps)
        $y = [int][math]::Round($FromY + ($ToY - $FromY) * $step / $steps)
        [void][NativeWindowInput]::SetCursorPos($x, $y)
        Start-Sleep -Milliseconds 20
    }
    [NativeWindowInput]::mouse_event($leftUp, 0, 0, 0, [UIntPtr]::Zero)
} elseif ($Action -eq 'DoubleClick') {
    for ($click = 0; $click -lt 2; $click++) {
        [NativeWindowInput]::mouse_event($leftDown, 0, 0, 0, [UIntPtr]::Zero)
        [NativeWindowInput]::mouse_event($leftUp, 0, 0, 0, [UIntPtr]::Zero)
        Start-Sleep -Milliseconds 70
    }
} else {
    [NativeWindowInput]::mouse_event($leftDown, 0, 0, 0, [UIntPtr]::Zero)
    [NativeWindowInput]::mouse_event($leftUp, 0, 0, 0, [UIntPtr]::Zero)
}
Write-Output "Native $Action delivered to HWND $WindowHandle."
