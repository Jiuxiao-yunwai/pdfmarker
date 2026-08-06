const APP_ID: &str = "com.shuqianjiang.bookmarkcraftsman";
const APP_NAME: &str = "书签匠";

#[cfg(target_os = "windows")]
pub fn register_identity() -> Result<(), String> {
    use winreg::{enums::HKEY_CURRENT_USER, RegKey};

    let current_user = RegKey::predef(HKEY_CURRENT_USER);
    let path = format!(r"Software\Classes\AppUserModelId\{APP_ID}");
    let (key, _) = current_user
        .create_subkey(path)
        .map_err(|error| format!("无法注册通知身份：{error}"))?;
    key.set_value("DisplayName", &APP_NAME)
        .map_err(|error| format!("无法设置通知名称：{error}"))?;
    key.set_value("IconBackgroundColor", &"0")
        .map_err(|error| format!("无法设置通知图标背景：{error}"))?;
    Ok(())
}

#[cfg(not(target_os = "windows"))]
pub fn register_identity() -> Result<(), String> {
    Ok(())
}

#[tauri::command]
pub fn show_app_notification(title: String, body: String) -> Result<(), String> {
    #[cfg(target_os = "windows")]
    {
        use tauri_winrt_notification::Toast;

        register_identity()?;
        Toast::new(APP_ID)
            .title(&title)
            .text1(&body)
            .show()
            .map_err(|error| format!("Windows 通知发送失败：{error}"))?;
    }
    #[cfg(not(target_os = "windows"))]
    {
        let _ = (title, body);
    }
    Ok(())
}
