//! Native notification commands that preserve operating-system failures and application identity.
//! 保留操作系统错误与应用身份的原生通知命令。

/// NotificationText contains only the title and body accepted by the desktop shell.
/// NotificationText 仅包含桌面壳接受的标题与正文。
#[derive(serde::Deserialize)]
pub(crate) struct NotificationText {
    /// Plain notification title, never executable markup.
    /// 通知纯文本标题，不作为可执行标记。
    title: String,
    /// Plain notification body, without conversation answer content.
    /// 通知纯文本正文，不包含会话回答内容。
    body: String,
}

/// Reads notification availability without installing an identity or prompting; null requires setup.
/// 无安装身份或授权提示地读取通知可用性；null 表示需要设置。
#[tauri::command]
pub(crate) async fn notification_permission(app: tauri::AppHandle) -> Result<Option<bool>, String> {
    #[cfg(windows)]
    return tokio::task::spawn_blocking(move || {
        windows_native::permission(&app.config().identifier)
    })
    .await
    .map_err(|error| error.to_string())?;
    #[cfg(not(windows))]
    {
        use tauri_plugin_notification::NotificationExt;
        app.notification()
            .permission_state()
            .map(|state| match state {
                tauri::plugin::PermissionState::Granted => Some(true),
                tauri::plugin::PermissionState::Denied => Some(false),
                _ => None,
            })
            .map_err(|error| error.to_string())
    }
}

/// Registers this desktop application's Windows identity only after an explicit enable action.
/// 仅在明确启用操作后注册本桌面应用的 Windows 身份。
#[tauri::command]
pub(crate) async fn request_notification_permission(
    app: tauri::AppHandle,
) -> Result<String, String> {
    #[cfg(windows)]
    return tokio::task::spawn_blocking(move || {
        windows_native::register(&app.config().identifier)?;
        windows_native::permission(&app.config().identifier).map(|allowed| {
            match allowed {
                Some(true) => "granted",
                Some(false) => "denied",
                None => "prompt",
            }
            .to_owned()
        })
    })
    .await
    .map_err(|error| error.to_string())?;
    #[cfg(not(windows))]
    {
        use tauri_plugin_notification::NotificationExt;
        app.notification()
            .request_permission()
            .map(|state| {
                match state {
                    tauri::plugin::PermissionState::Granted => "granted",
                    tauri::plugin::PermissionState::Denied => "denied",
                    _ => "prompt",
                }
                .to_owned()
            })
            .map_err(|error| error.to_string())
    }
}

/// Submits one notification and returns the actual OS submission result; banner presentation remains OS-owned.
/// 提交一条通知并返回真实系统提交结果；横幅呈现仍由操作系统决定。
#[tauri::command]
pub(crate) async fn show_notification(
    app: tauri::AppHandle,
    options: NotificationText,
) -> Result<(), String> {
    // Validate bounded plain text before passing it across the OS boundary.
    // 向操作系统提交前校验有界纯文本。
    if options.title.len() > 1024 || options.body.len() > 8192 || options.title.trim().is_empty() {
        return Err("Notification title or body is invalid".to_owned());
    }
    tokio::task::spawn_blocking(move || {
        #[cfg(windows)]
        return windows_native::show(&app.config().identifier, &options.title, &options.body);
        #[cfg(not(windows))]
        {
            #[cfg(target_os = "macos")]
            notify_rust::set_application(&app.config().identifier)
                .map_err(|error| error.to_string())?;
            #[cfg(not(target_os = "macos"))]
            let _ = app;
            notify_rust::Notification::new()
                .summary(&options.title)
                .body(&options.body)
                .show()
                .map(|_| ())
                .map_err(|error| error.to_string())
        }
    })
    .await
    .map_err(|error| error.to_string())?
}

// COM interop is confined here; all callers use safe operations with owned paths and balanced apartments.
// COM 互操作仅限此模块；所有调用方使用具有自有路径和配对线程单元的安全操作。
#[cfg(windows)]
#[allow(unsafe_code)]
mod windows_native {
    use std::path::{Path, PathBuf};
    use windows::{
        Data::Xml::Dom::XmlDocument,
        Foundation::{DateTime, IReference, PropertyValue},
        UI::Notifications::{NotificationSetting, ToastNotification, ToastNotificationManager},
        Win32::{
            Foundation::{ERROR_FILE_NOT_FOUND, ERROR_NOT_FOUND, ERROR_PATH_NOT_FOUND},
            Storage::EnhancedStorage::PKEY_AppUserModel_ID,
            System::Com::{
                CLSCTX_INPROC_SERVER, COINIT_MULTITHREADED, CoCreateInstance, CoInitializeEx,
                CoTaskMemFree, CoUninitialize, IPersistFile, StructuredStorage::PROPVARIANT,
            },
            System::Registry::{
                HKEY_CURRENT_USER, REG_SZ, RRF_RT_REG_SZ, RegGetValueW, RegSetKeyValueW,
            },
            UI::Shell::{
                FOLDERID_Programs, IShellLinkW, KF_FLAG_DEFAULT, PropertiesSystem::IPropertyStore,
                SHGetKnownFolderPath, ShellLink,
            },
        },
        core::{BSTR, HSTRING, Interface, w},
    };

    /// Apartment balances each successful COM initialization on the current worker thread.
    /// Apartment 配对当前工作线程上每次成功的 COM 初始化。
    struct Apartment;
    impl Apartment {
        /// Initializes COM for a worker; returns an explicit error for incompatible apartment state.
        /// 为工作线程初始化 COM；线程单元不兼容时返回明确错误。
        fn new() -> Result<Self, String> {
            // No pointer is retained; this worker owns the balanced initialization count.
            // 不保留指针；当前线程拥有配对初始化计数。
            unsafe { CoInitializeEx(None, COINIT_MULTITHREADED).ok() }
                .map_err(|error| error.to_string())?;
            Ok(Self)
        }
    }
    impl Drop for Apartment {
        /// Releases exactly the initialization count acquired by Apartment::new.
        /// 仅释放 Apartment::new 取得的一次初始化计数。
        fn drop(&mut self) {
            unsafe { CoUninitialize() };
        }
    }

    /// Resolves the current user's Programs directory using the OS-owned known-folder contract.
    /// 使用操作系统的已知文件夹契约解析当前用户的开始菜单程序目录。
    fn shortcut_path() -> Result<PathBuf, String> {
        // Windows allocates this string with the COM allocator; free it after copying.
        // Windows 使用 COM 分配器分配此字符串，复制后立即释放。
        let allocated = unsafe { SHGetKnownFolderPath(&FOLDERID_Programs, KF_FLAG_DEFAULT, None) }
            .map_err(|error| error.to_string())?;
        // Conversion occurs before freeing the sole owned OS allocation.
        // 在释放唯一自有系统分配前完成转换。
        let copied = unsafe { allocated.to_string() };
        unsafe { CoTaskMemFree(Some(allocated.0.cast())) };
        copied
            .map(|path| {
                PathBuf::from(path)
                    .join("Vulcan Code")
                    .join("Vulcan Code.lnk")
            })
            .map_err(|error| error.to_string())
    }

    /// Writes an owned shortcut with the explicit application ID used for every toast.
    /// 写入自有快捷方式，携带每条通知使用的明确应用标识。
    fn write_shortcut(path: &Path, executable: &Path, app_id: &str) -> Result<(), String> {
        // A new in-process ShellLink exposes its documented property-store and persistence interfaces.
        // 新建进程内 ShellLink 提供契约规定的属性存储与持久化接口。
        let shortcut: IShellLinkW =
            unsafe { CoCreateInstance(&ShellLink, None, CLSCTX_INPROC_SERVER) }
                .map_err(|error| error.to_string())?;
        // HSTRING owns the UTF-16 path throughout each synchronous COM call.
        // HSTRING 在每次同步 COM 调用期间持有 UTF-16 路径。
        unsafe { shortcut.SetPath(&HSTRING::from(executable.as_os_str())) }
            .map_err(|error| error.to_string())?;
        unsafe { shortcut.SetDescription(&HSTRING::from("Vulcan Code")) }
            .map_err(|error| error.to_string())?;
        unsafe { shortcut.SetIconLocation(&HSTRING::from(executable.as_os_str()), 0) }
            .map_err(|error| error.to_string())?;
        // The property variant owns its string and outlives SetValue and Commit.
        // 属性变体持有字符串，其生命周期覆盖 SetValue 和 Commit。
        let store: IPropertyStore = shortcut.cast().map_err(|error| error.to_string())?;
        // identity owns the application identifier for the property-store write.
        // identity 持有写入属性存储的应用标识。
        let identity = PROPVARIANT::from(app_id);
        unsafe { store.SetValue(&PKEY_AppUserModel_ID, &identity) }
            .map_err(|error| error.to_string())?;
        unsafe { store.Commit() }.map_err(|error| error.to_string())?;
        // Save writes only the caller-provided application-owned shortcut path.
        // Save 仅写入调用方提供的应用自有快捷方式路径。
        let file: IPersistFile = shortcut.cast().map_err(|error| error.to_string())?;
        unsafe { file.Save(&HSTRING::from(path.as_os_str()), true) }
            .map_err(|error| error.to_string())
    }

    /// Registers the executable in this user's Start menu without changing notification preferences.
    /// 在当前用户开始菜单注册可执行程序，不更改通知偏好。
    pub(super) fn register(app_id: &str) -> Result<(), String> {
        // _apartment keeps every registration COM call on a correctly initialized worker.
        // _apartment 确保每个注册 COM 调用均位于已正确初始化的工作线程。
        let _apartment = Apartment::new()?;
        // path is the single application-owned Start menu shortcut location.
        // path 是应用唯一自有的开始菜单快捷方式位置。
        let path = shortcut_path()?;
        // Preserve existing application or user shortcuts rather than replacing their arguments or metadata.
        // 保留已有应用或用户快捷方式，不覆盖其中的参数与元数据。
        if path.try_exists().map_err(|error| error.to_string())? {
            if !registered(&path, app_id)? {
                return Err(
                    "Notification shortcut path is occupied by another application identity"
                        .to_owned(),
                );
            }
        } else {
            // Only create the application-owned shortcut when no existing one occupies its exact path.
            // 仅当精确路径不存在已有快捷方式时创建应用自有快捷方式。
            let parent = path.parent().ok_or("Notification shortcut has no parent")?;
            std::fs::create_dir_all(parent).map_err(|error| error.to_string())?;
            // The current process owns this explicit enable request.
            // 当前进程持有本次明确启用请求。
            let executable = std::env::current_exe().map_err(|error| error.to_string())?;
            write_shortcut(&path, &executable, app_id)?;
        }
        register_notification_identity(app_id)?;
        initialize_notification_channel(app_id)
    }

    /// Initializes the unpackaged app's first notification channel only during explicit registration.
    /// 仅在明确注册时初始化非打包应用的首次通知通道。
    /// app_id identifies the owned notifier; returns OS errors without changing notification policy.
    /// app_id 标识自有通知器；返回系统错误，不修改通知策略。
    fn initialize_notification_channel(app_id: &str) -> Result<(), String> {
        // Windows Community Toolkit's PreRegisterIdentityLessApp documents this first-send requirement.
        // Windows Community Toolkit 的 PreRegisterIdentityLessApp 记录了此首次投递要求。
        let notifier = ToastNotificationManager::CreateToastNotifierWithId(&HSTRING::from(app_id))
            .map_err(|error| format!("Create notification channel: {error}"))?;
        match notifier.Setting() {
            Ok(_) => return Ok(()),
            Err(error) if error.code() == ERROR_NOT_FOUND.to_hresult() => {}
            Err(error) => return Err(format!("Read notification channel: {error}")),
        }
        // Only this suppressed setup toast is removed; existing user notifications remain untouched.
        // 仅移除此条静默初始化通知；不影响已有用户通知。
        let document = XmlDocument::new().map_err(|error| error.to_string())?;
        document
            .LoadXml(&HSTRING::from(toast_xml("Vulcan Code", "通知已初始化")))
            .map_err(|error| error.to_string())?;
        // The fixed tag/group isolate setup from all conversation notifications.
        // 固定标签与分组使初始化通知与所有会话通知隔离。
        let notification = ToastNotification::CreateToastNotification(&document)
            .map_err(|error| error.to_string())?;
        notification
            .SetSuppressPopup(true)
            .map_err(|error| error.to_string())?;
        notification
            .SetTag(&HSTRING::from("vcn-init"))
            .map_err(|error| error.to_string())?;
        notification
            .SetGroup(&HSTRING::from("vcn-setup"))
            .map_err(|error| error.to_string())?;
        // WinRT time uses 100 ns ticks since 1601; a 15 second expiry bounds a failed cleanup.
        // WinRT 时间为自 1601 年起的 100 纳秒计数；15 秒过期时间限制清理失败的影响。
        let unix_ticks = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map_err(|error| error.to_string())?
            .as_nanos()
            / 100;
        // Conversion checks the WinRT signed range before boxing the expiration value.
        // 装箱过期时间前校验 WinRT 有符号范围。
        let expiration: IReference<DateTime> = PropertyValue::CreateDateTime(DateTime {
            UniversalTime: i64::try_from(unix_ticks + 116_444_736_000_000_000 + 150_000_000)
                .map_err(|error| error.to_string())?,
        })
        .and_then(|value| value.cast())
        .map_err(|error| error.to_string())?;
        notification
            .SetExpirationTime(&expiration)
            .map_err(|error| error.to_string())?;
        notifier
            .Show(&notification)
            .map_err(|error| format!("Initialize notification channel: {error}"))?;
        ToastNotificationManager::History()
            .and_then(|history| {
                history.RemoveGroupedTagWithId(
                    &HSTRING::from("vcn-init"),
                    &HSTRING::from("vcn-setup"),
                    &HSTRING::from(app_id),
                )
            })
            .map_err(|error| format!("Remove notification setup entry: {error}"))
    }

    /// Reads the unpackaged identity's display-name registration, required by ToastNotifier::Setting.
    /// 读取非打包应用身份的显示名称注册，ToastNotifier::Setting 需要此注册。
    /// app_id is the configured application identifier; returns false for an absent key, preserving other OS failures.
    /// app_id 是配置中的应用标识；键缺失时返回 false，保留其他系统错误。
    fn notification_identity_registered(app_id: &str) -> Result<bool, String> {
        // This path is the documented per-user identity namespace, not the notification policy namespace.
        // 此路径是文档规定的用户级身份命名空间，不是通知策略命名空间。
        let key = HSTRING::from(format!(r"Software\Classes\AppUserModelId\{app_id}"));
        // The query validates REG_SZ and counts its bytes without allocating or exposing other registration values.
        // 查询校验 REG_SZ 并统计字节，不分配或暴露其他注册值。
        let mut bytes = 0_u32;
        let result = unsafe {
            RegGetValueW(
                HKEY_CURRENT_USER,
                &key,
                w!("DisplayName"),
                RRF_RT_REG_SZ,
                None,
                None,
                Some(&mut bytes),
            )
        };
        if result == ERROR_FILE_NOT_FOUND || result == ERROR_PATH_NOT_FOUND {
            return Ok(false);
        }
        result
            .ok()
            .map_err(|error| format!("Read notification identity: {error}"))?;
        Ok(bytes > 2)
    }

    /// Registers only the display identity after explicit authorization, preserving all notification policy and activation values.
    /// 仅在明确授权后注册显示身份，保留全部通知策略与激活值。
    /// app_id is the application's configured identity; returns the exact registry write result.
    /// app_id 是应用配置身份；返回精确注册表写入结果。
    fn register_notification_identity(app_id: &str) -> Result<(), String> {
        if notification_identity_registered(app_id)? {
            return Ok(());
        }
        // A shortcut alone can exist while Setting fails with ERROR_NOT_FOUND on an unpackaged installation.
        // 非打包安装中可出现快捷方式已存在，而 Setting 仍因 ERROR_NOT_FOUND 失败的情况。
        let key = HSTRING::from(format!(r"Software\Classes\AppUserModelId\{app_id}"));
        // REG_SZ requires a terminating UTF-16 zero included in the byte count.
        // REG_SZ 要求 UTF-16 终止零，字节数必须包含它。
        let name: Vec<u16> = "Vulcan Code"
            .encode_utf16()
            .chain(std::iter::once(0))
            .collect();
        // The owned buffer remains alive until the synchronous registry write returns.
        // 自有缓冲区保持存活直到同步注册表写入返回。
        unsafe {
            RegSetKeyValueW(
                HKEY_CURRENT_USER,
                &key,
                w!("DisplayName"),
                REG_SZ.0,
                Some(name.as_ptr().cast()),
                (name.len() * 2) as u32,
            )
        }
        .ok()
        .map_err(|error| format!("Register notification identity: {error}"))
    }

    /// Reads the persisted shortcut identity; a missing registration requires the user's enable action.
    /// 读取持久化快捷方式身份；缺少注册时需要用户启用操作。
    fn registered(path: &Path, app_id: &str) -> Result<bool, String> {
        if !path.try_exists().map_err(|error| error.to_string())? {
            return Ok(false);
        }
        // shortcut is a fresh COM object used only to read the caller's exact persisted link.
        // shortcut 是仅用于读取调用方精确持久化链接的新 COM 对象。
        let shortcut: IShellLinkW =
            unsafe { CoCreateInstance(&ShellLink, None, CLSCTX_INPROC_SERVER) }
                .map_err(|error| error.to_string())?;
        // file exposes read-only shortcut loading through the documented persistence interface.
        // file 通过规定的持久化接口提供只读快捷方式加载。
        let file: IPersistFile = shortcut.cast().map_err(|error| error.to_string())?;
        unsafe {
            file.Load(
                &HSTRING::from(path.as_os_str()),
                windows::Win32::System::Com::STGM_READ,
            )
        }
        .map_err(|error| error.to_string())?;
        // store owns the loaded shortcut properties for the remainder of this read.
        // store 在本次读取结束前持有已加载的快捷方式属性。
        let store: IPropertyStore = shortcut.cast().map_err(|error| error.to_string())?;
        // value is the application-identity property and must be convertible to a string.
        // value 是应用身份属性，必须可以转换为字符串。
        let value =
            unsafe { store.GetValue(&PKEY_AppUserModel_ID) }.map_err(|error| error.to_string())?;
        // identity is compared byte-for-byte after UTF-16 conversion, without alias fallback.
        // identity 在 UTF-16 转换后进行精确比较，不使用别名回退。
        let identity = BSTR::try_from(&value).map_err(|error| error.to_string())?;
        Ok(identity.to_string() == app_id)
    }

    /// Reads both durable app registration and Windows notification policy; never writes user state.
    /// 同时读取耐久应用注册与 Windows 通知策略，绝不写入用户状态。
    pub(super) fn permission(app_id: &str) -> Result<Option<bool>, String> {
        // _apartment balances this read-only COM and WinRT access on its worker thread.
        // _apartment 配对当前工作线程上的只读 COM 与 WinRT 访问。
        let _apartment = Apartment::new()?;
        if !registered(&shortcut_path()?, app_id)? || !notification_identity_registered(app_id)? {
            return Ok(None);
        }
        // notifier reads policy for the exact durable application identity.
        // notifier 读取精确持久化应用身份对应的策略。
        let notifier = ToastNotificationManager::CreateToastNotifierWithId(&HSTRING::from(app_id))
            .map_err(|error| error.to_string())?;
        match notifier.Setting() {
            Ok(setting) => Ok(Some(setting == NotificationSetting::Enabled)),
            // A first-send channel is not initialized yet; reads must never perform setup themselves.
            // 首次投递通道尚未初始化；只读查询绝不能自行执行初始化。
            Err(error) if error.code() == ERROR_NOT_FOUND.to_hresult() => Ok(None),
            Err(error) => Err(format!("Read Windows notification policy: {error}")),
        }
    }

    /// Submits escaped text through the exact registered app notifier and propagates OS errors.
    /// 通过精确注册的应用通知器提交转义文本，并传播系统错误。
    pub(super) fn show(app_id: &str, title: &str, body: &str) -> Result<(), String> {
        // _apartment owns the COM initialization until the synchronous submission returns.
        // _apartment 持有 COM 初始化，直至同步提交返回。
        let _apartment = Apartment::new()?;
        if !registered(&shortcut_path()?, app_id)? || !notification_identity_registered(app_id)? {
            return Err("Notification app identity is not registered".to_owned());
        }
        // notifier is tied to the same identity verified from the installed shortcut.
        // notifier 绑定到刚从已安装快捷方式验证的同一身份。
        let notifier = ToastNotificationManager::CreateToastNotifierWithId(&HSTRING::from(app_id))
            .map_err(|error| error.to_string())?;
        if notifier.Setting().map_err(|error| error.to_string())? != NotificationSetting::Enabled {
            return Err("Windows notification settings block this application".to_owned());
        }
        // document contains a fixed notification template and escaped text only.
        // document 仅包含固定通知模板与转义文本。
        let document = XmlDocument::new().map_err(|error| error.to_string())?;
        document
            .LoadXml(&HSTRING::from(toast_xml(title, body)))
            .map_err(|error| error.to_string())?;
        // notification remains owned until Windows accepts or rejects the submission.
        // notification 的所有权保持到 Windows 接受或拒绝本次提交。
        let notification = ToastNotification::CreateToastNotification(&document)
            .map_err(|error| error.to_string())?;
        notifier
            .Show(&notification)
            .map_err(|error| error.to_string())
    }

    /// Escapes user-visible text for the fixed ToastGeneric template, including XML control characters.
    /// 为固定 ToastGeneric 模板转义用户可见文本，并过滤 XML 控制字符。
    fn toast_xml(title: &str, body: &str) -> String {
        // Text is inserted only into XML text nodes; attributes remain fixed literals.
        // 文本仅插入 XML 文本节点；属性保持固定字面量。
        fn escape(text: &str) -> String {
            text.chars()
                .filter(|ch| {
                    *ch == '\t'
                        || *ch == '\n'
                        || *ch == '\r'
                        || (*ch >= ' ' && *ch != '\u{fffe}' && *ch != '\u{ffff}')
                })
                .map(|ch| match ch {
                    '&' => "&amp;".to_owned(),
                    '<' => "&lt;".to_owned(),
                    '>' => "&gt;".to_owned(),
                    _ => ch.to_string(),
                })
                .collect()
        }
        format!(
            "<toast><visual><binding template=\"ToastGeneric\"><text>{}</text><text>{}</text></binding></visual></toast>",
            escape(title),
            escape(body)
        )
    }

    #[cfg(test)]
    mod tests {
        use super::*;
        /// Runs the real desktop notification path only when explicitly selected after user authorization.
        /// 仅在用户授权后明确选中时运行真实桌面通知链路。
        /// Requires an existing app-owned shortcut; preserves its target and notification policy.
        /// 要求已有应用自有快捷方式；保留其目标和通知策略。
        #[test]
        #[ignore = "requires explicit authorization to register and send an actual desktop test notification"]
        fn installed_notification_delivery_acceptance() {
            let _apartment = Apartment::new().expect("COM");
            assert!(
                registered(&shortcut_path().expect("shortcut path"), "ai.vulcan.code")
                    .expect("installed app identity")
            );
            register("ai.vulcan.code").expect("register display identity");
            assert_eq!(
                permission("ai.vulcan.code").expect("real notification policy"),
                Some(true)
            );
            show(
                "ai.vulcan.code",
                "Vulcan Code",
                "真实通知验收：Windows 已接受本次测试通知。",
            )
            .expect("submit actual test notification");
        }
        /// Checks real COM persistence in a disposable directory without installing or displaying a notification.
        /// 在可丢弃目录检查真实 COM 持久化，不安装或显示通知。
        #[test]
        fn shortcut_preserves_exact_application_identity() {
            let _apartment = Apartment::new().expect("COM");
            let directory = tempfile::tempdir().expect("temporary directory");
            let path = directory.path().join("notification.lnk");
            assert!(!registered(&path, "ai.vulcan.code.test").expect("absent identity"));
            write_shortcut(
                &path,
                &std::env::current_exe().expect("executable"),
                "ai.vulcan.code.test",
            )
            .expect("write shortcut");
            assert!(registered(&path, "ai.vulcan.code.test").expect("exact identity"));
            assert!(!registered(&path, "ai.vulcan.code.other").expect("different identity"));
        }
        /// Checks text cannot inject XML elements or invalid control characters.
        /// 检查文本无法注入 XML 元素或非法控制字符。
        #[test]
        fn notification_text_is_escaped() {
            assert_eq!(
                toast_xml("<>&\0", "中文"),
                "<toast><visual><binding template=\"ToastGeneric\"><text>&lt;&gt;&amp;</text><text>中文</text></binding></visual></toast>"
            );
        }
    }
}
