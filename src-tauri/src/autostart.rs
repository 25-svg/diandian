const RUN_KEY: &str = r"HKCU\Software\Microsoft\Windows\CurrentVersion\Run";
const VALUE_NAME: &str = "DianDianLiveClip";

#[cfg(target_os = "windows")]
fn run_reg(arguments: &[&str]) -> Result<std::process::Output, String> {
    use std::os::windows::process::CommandExt;
    const CREATE_NO_WINDOW: u32 = 0x0800_0000;
    std::process::Command::new("reg.exe")
        .args(arguments)
        .creation_flags(CREATE_NO_WINDOW)
        .output()
        .map_err(|error| format!("无法调用 Windows 启动项管理：{error}"))
}

#[cfg(target_os = "windows")]
pub fn set_enabled(enabled: bool) -> Result<(), String> {
    let output = if enabled {
        let executable =
            std::env::current_exe().map_err(|error| format!("无法获取程序路径：{error}"))?;
        let command = format!("\"{}\" --minimized", executable.display());
        run_reg(&[
            "ADD", RUN_KEY, "/v", VALUE_NAME, "/t", "REG_SZ", "/d", &command, "/f",
        ])?
    } else {
        run_reg(&["DELETE", RUN_KEY, "/v", VALUE_NAME, "/f"])?
    };
    if output.status.success() || (!enabled && !is_enabled()) {
        Ok(())
    } else {
        let error = String::from_utf8_lossy(&output.stderr).trim().to_string();
        Err(if error.is_empty() {
            "Windows 启动项更新失败".to_string()
        } else {
            error
        })
    }
}

#[cfg(target_os = "windows")]
pub fn is_enabled() -> bool {
    run_reg(&["QUERY", RUN_KEY, "/v", VALUE_NAME])
        .map(|output| output.status.success())
        .unwrap_or(false)
}

#[cfg(not(target_os = "windows"))]
pub fn set_enabled(_enabled: bool) -> Result<(), String> {
    Ok(())
}

#[cfg(not(target_os = "windows"))]
pub fn is_enabled() -> bool {
    false
}

#[cfg(test)]
mod tests {
    #[test]
    fn startup_value_name_is_stable() {
        assert_eq!(super::VALUE_NAME, "DianDianLiveClip");
    }
}
