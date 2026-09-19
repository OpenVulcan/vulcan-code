# Invoke the same native COM callback that Windows uses for a clicked Vulcan Code toast.
# 调用 Windows 点击 Vulcan Code 通知时使用的同一原生 COM 回调。
param(
    [Parameter(Mandatory = $true)]
    [ValidatePattern('^(?:w10-notification-[0-9a-f-]{36}|notification:[0-9a-f-]{36})$')]
    [string]$NotificationId,
    [string]$DebugExecutable,
    [switch]$IdentityOnly,
    [ValidateSet('Release', 'Debug')]
    [string]$Profile = 'Release'
)

$ErrorActionPreference = 'Stop'

# Reject a toast identifier that belongs to the other package identity.
# 拒绝属于另一发行包身份的通知标识。
if (($Profile -eq 'Debug' -and !$NotificationId.StartsWith('notification:')) -or
    ($Profile -eq 'Release' -and !$NotificationId.StartsWith('w10-notification-'))) {
    throw 'NotificationId does not belong to the selected desktop profile.'
}

# Resolve-DebugNotificationIdentity derives the same path-owned AUMID and CLSID as Electron.
# Resolve-DebugNotificationIdentity 派生与 Electron 相同的路径专属 AUMID 与 CLSID。
# DebugExecutable is the exact branded binary; the returned object contains its native identity.
# DebugExecutable 是精确的品牌化二进制程序；返回对象包含其原生身份。
function Resolve-DebugNotificationIdentity {
    param(
        [Parameter(Mandatory = $true)]
        [string]$ExecutablePath
    )

    if (-not (Test-Path -LiteralPath $ExecutablePath -PathType Leaf)) {
        throw "Debug executable does not exist: $ExecutablePath"
    }
    # normalizedExecutable matches Node's case-insensitive absolute Windows path input.
    # normalizedExecutable 与 Node 不区分大小写的 Windows 绝对路径输入保持一致。
    $normalizedExecutable = [System.IO.Path]::GetFullPath($ExecutablePath)
    if ($normalizedExecutable.StartsWith('\\?\')) {
        $normalizedExecutable = $normalizedExecutable.Substring(4)
    }
    $normalizedExecutable = $normalizedExecutable.ToLowerInvariant()
    # sha256 owns the deterministic hash operation and is disposed before COM activation.
    # sha256 持有确定性摘要操作，并在 COM 激活前释放。
    $sha256 = [System.Security.Cryptography.SHA256]::Create()
    try {
        $hashBytes = $sha256.ComputeHash([System.Text.Encoding]::UTF8.GetBytes($normalizedExecutable))
    }
    finally {
        $sha256.Dispose()
    }
    # hash is the lowercase hexadecimal identity material shared with the Node implementation.
    # hash 是与 Node 实现共享的小写十六进制身份材料。
    $hash = [System.BitConverter]::ToString($hashBytes).Replace('-', '').ToLowerInvariant()
    # variantNibble installs RFC 4122 variant bits while retaining two hash bits.
    # variantNibble 应用 RFC 4122 变体位，同时保留两位摘要信息。
    $variantNibble = (([System.Convert]::ToInt32($hash.Substring(16, 1), 16) -band 3) -bor 8).ToString('x')
    # activator formats the first 128 hash bits as the same version-5-style GUID used by Electron.
    # activator 将摘要前 128 位格式化为与 Electron 相同的第 5 版样式 GUID。
    $activator = ('{' + $hash.Substring(0, 8) + '-' + $hash.Substring(8, 4) + '-5' +
        $hash.Substring(13, 3) + '-' + $variantNibble + $hash.Substring(17, 3) + '-' +
        $hash.Substring(20, 12) + '}').ToUpperInvariant()
    return [pscustomobject]@{
        AppUserModelId = 'ai.vulcan.code.desktop.debug.' + $hash.Substring(0, 12)
        ActivatorClsid = $activator
    }
}

# Bind the probe to the exact native identity for the selected desktop package.
# 将探针绑定到所选桌面发行包的精确原生身份。
if ($Profile -eq 'Debug') {
    if ([string]::IsNullOrWhiteSpace($DebugExecutable)) {
        throw 'DebugExecutable is required for a path-owned Debug notification identity.'
    }
    # debugIdentity is derived from the executable that must receive the activation.
    # debugIdentity 从必须接收激活的可执行程序派生。
    $debugIdentity = Resolve-DebugNotificationIdentity -ExecutablePath $DebugExecutable
    $appUserModelId = $debugIdentity.AppUserModelId
    $activatorClsid = $debugIdentity.ActivatorClsid
}
else {
    $appUserModelId = 'ai.vulcan.code.desktop'
    $activatorClsid = 'AB3EF79F-A068-4BA3-9B40-814138499202'
}

# IdentityOnly returns the resolved native identity without creating a COM activation instance.
# IdentityOnly 返回已解析的原生身份，而不创建 COM 激活实例。
if ($IdentityOnly) {
    [pscustomobject]@{
        AppUserModelId = $appUserModelId
        ActivatorClsid = $activatorClsid
    } | ConvertTo-Json -Compress
    return
}

$source = @'
using System;
using System.Runtime.InteropServices;

namespace VulcanCodeToastProbe {
    // Match the Windows SDK notification activation interface exactly.
    // 精确匹配 Windows SDK 的通知激活接口。
    [ComImport]
    [Guid("53E31837-6600-4A81-9395-75CFFE746F94")]
    [InterfaceType(ComInterfaceType.InterfaceIsIUnknown)]
    public interface INotificationActivationCallback {
        // Deliver the application id, clicked toast arguments and optional inputs; return HRESULT.
        // 传递应用 ID、被点击通知的参数和可选输入；返回 HRESULT。
        [PreserveSig]
        int Activate(
            [MarshalAs(UnmanagedType.LPWStr)] string appUserModelId,
            [MarshalAs(UnmanagedType.LPWStr)] string invokedArgs,
            IntPtr data,
            uint count);
    }

    // Resolve only the fixed Vulcan Code toast activator registered by Electron.
    // 只解析由 Electron 注册的固定 Vulcan Code 通知激活器。
    public static class ActivatorProbe {
        // Invoke one native click activation for the specified verified toast id; return HRESULT.
        // 对指定且已核验的通知 ID 执行一次原生点击激活；返回 HRESULT。
        public static int Activate(string notificationId, string appUserModelId, string activatorClsid) {
            Type serverType = Type.GetTypeFromCLSID(
                new Guid(activatorClsid), true);
            object server = System.Activator.CreateInstance(serverType);
            try {
                return ((INotificationActivationCallback)server).Activate(
                    appUserModelId,
                    "type=click&tag=" + notificationId,
                    IntPtr.Zero,
                    0);
            } finally {
                Marshal.ReleaseComObject(server);
            }
        }
    }
}
'@

Add-Type -TypeDefinition $source -ErrorAction Stop
$result = [VulcanCodeToastProbe.ActivatorProbe]::Activate($NotificationId, $appUserModelId, $activatorClsid)
if ($result -ne 0) {
    throw "Native toast activation returned HRESULT $result"
}
