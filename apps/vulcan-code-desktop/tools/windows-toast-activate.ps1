# Invoke the same native COM callback that Windows uses for a clicked Vulcan Code toast.
# 调用 Windows 点击 Vulcan Code 通知时使用的同一原生 COM 回调。
param(
    [Parameter(Mandatory = $true)]
    [ValidatePattern('^(?:w10-notification-[0-9a-f-]{36}|notification:[0-9a-f-]{36})$')]
    [string]$NotificationId,
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

# Bind the probe to the exact native identity for the selected desktop package.
# 将探针绑定到所选桌面发行包的精确原生身份。
$appUserModelId = if ($Profile -eq 'Debug') { 'ai.vulcan.code.desktop.debug' } else { 'ai.vulcan.code.desktop' }
$activatorClsid = if ($Profile -eq 'Debug') { '3A5A6297-FA5F-4789-83BC-1520F0D103A7' } else { 'AB3EF79F-A068-4BA3-9B40-814138499202' }

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
