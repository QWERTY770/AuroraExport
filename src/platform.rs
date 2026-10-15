//! Small platform helpers: opening files/URLs and console handling.

mod targets;
use targets::normalized_target;

#[cfg(windows)]
pub struct ComApartment;

#[cfg(windows)]
impl ComApartment {
    pub fn new() -> windows::core::Result<Self> {
        use windows::Win32::System::Com::{
            CoInitializeEx, COINIT_APARTMENTTHREADED, COINIT_DISABLE_OLE1DDE,
        };
        unsafe {
            CoInitializeEx(None, COINIT_APARTMENTTHREADED | COINIT_DISABLE_OLE1DDE).ok()?;
        }
        Ok(Self)
    }
}

#[cfg(windows)]
impl Drop for ComApartment {
    fn drop(&mut self) {
        unsafe {
            windows::Win32::System::Com::CoUninitialize();
        }
    }
}

pub fn open_target(target: &str) -> Result<(), String> {
    let target = normalized_target(target)?;
    #[cfg(windows)]
    {
        std::thread::Builder::new()
            .name("aurora-open".into())
            .spawn(move || open_windows(&target))
            .map_err(|error| error.to_string())?
            .join()
            .map_err(|_| "system launcher failed".to_string())?
    }
    #[cfg(not(windows))]
    {
        #[cfg(target_os = "macos")]
        let command = "open";
        #[cfg(not(target_os = "macos"))]
        let command = "xdg-open";
        let output = std::process::Command::new(command)
            .arg(&target)
            .output()
            .map_err(|error| format!("{command}: {error}"))?;
        if output.status.success() {
            Ok(())
        } else {
            Err(format!(
                "{command}: {} ({})",
                String::from_utf8_lossy(&output.stderr).trim(),
                output.status
            ))
        }
    }
}

#[cfg(windows)]
fn open_windows(target: &str) -> Result<(), String> {
    let _com = ComApartment::new().map_err(|error| error.to_string())?;
    let permissions = crate::permissions::process_permissions()
        .map_err(|error| format!("process permissions: {error}"))?;
    if let Some(permissions) = permissions {
        if !permissions.restricted && !permissions.app_container {
            match open_via_desktop(target) {
                Ok(()) => return Ok(()),
                Err(error) if permissions.elevated => {
                    return Err(format!(
                        "desktop launcher: {error}; restart AuroraExport without administrator privileges"
                    ));
                }
                Err(desktop_error) => {
                    return shell_execute(target).map_err(|error| {
                        format!("desktop launcher: {desktop_error}; system launcher: {error}")
                    });
                }
            }
        }
    }
    shell_execute(target)
}

#[cfg(windows)]
fn open_via_desktop(target: &str) -> windows::core::Result<()> {
    use windows::core::{Interface, BSTR};
    use windows::Win32::System::Com::{
        CoCreateInstance, IDispatch, IServiceProvider, CLSCTX_LOCAL_SERVER,
    };
    use windows::Win32::System::Variant::VARIANT;
    use windows::Win32::UI::Shell::{
        IShellBrowser, IShellDispatch2, IShellFolderViewDual, IShellWindows, SID_STopLevelBrowser,
        ShellWindows, SVGIO_BACKGROUND, SWC_DESKTOP, SWFO_NEEDDISPATCH,
    };

    unsafe {
        let shell_windows: IShellWindows =
            CoCreateInstance(&ShellWindows, None, CLSCTX_LOCAL_SERVER)?;
        let empty = VARIANT::default();
        let mut window = 0;
        let desktop = shell_windows.FindWindowSW(
            &empty,
            &empty,
            SWC_DESKTOP,
            &mut window,
            SWFO_NEEDDISPATCH,
        )?;
        let provider: IServiceProvider = desktop.cast()?;
        let browser: IShellBrowser = provider.QueryService(&SID_STopLevelBrowser)?;
        let view = browser.QueryActiveShellView()?;
        let dispatch: IDispatch = view.GetItemObject(SVGIO_BACKGROUND)?;
        let folder: IShellFolderViewDual = dispatch.cast()?;
        let shell: IShellDispatch2 = folder.Application()?.cast()?;
        let empty_string = VARIANT::from("");
        shell.ShellExecute(
            &BSTR::from(target),
            &empty_string,
            &empty_string,
            &VARIANT::from("open"),
            &VARIANT::from(1i32),
        )
    }
}

#[cfg(windows)]
fn shell_execute(target: &str) -> Result<(), String> {
    use windows::core::PCWSTR;
    use windows::Win32::UI::Shell::{
        ShellExecuteExW, SEE_MASK_FLAG_NO_UI, SEE_MASK_NOASYNC, SHELLEXECUTEINFOW,
    };
    let operation = "open".encode_utf16().chain(Some(0)).collect::<Vec<_>>();
    let file = target.encode_utf16().chain(Some(0)).collect::<Vec<_>>();
    let mut info = SHELLEXECUTEINFOW {
        cbSize: std::mem::size_of::<SHELLEXECUTEINFOW>() as u32,
        fMask: SEE_MASK_NOASYNC | SEE_MASK_FLAG_NO_UI,
        lpVerb: PCWSTR(operation.as_ptr()),
        lpFile: PCWSTR(file.as_ptr()),
        nShow: 1,
        ..Default::default()
    };
    unsafe { ShellExecuteExW(&mut info) }.map_err(|error| error.to_string())
}

/// Number of processes attached to this process's console (0 when there is none).
#[cfg(windows)]
pub fn console_process_count() -> u32 {
    #[link(name = "kernel32")]
    extern "system" {
        fn GetConsoleProcessList(process_list: *mut u32, count: u32) -> u32;
    }
    let mut processes = [0u32; 16];
    unsafe { GetConsoleProcessList(processes.as_mut_ptr(), processes.len() as u32) }
}

#[cfg(not(windows))]
pub fn console_process_count() -> u32 {
    0
}

/// Hides the console window when it was created *for this process*.
///
/// A console subsystem binary gets a console window when it is double clicked.
/// The window is only hidden when our process is the sole owner of that
/// console: when the tool is started from an existing terminal the console is
/// shared with the shell, and staying visible keeps `--help`, `--selftest` and
/// `--fetch` working like a normal command line program.
#[cfg(windows)]
pub fn hide_console_if_standalone() -> bool {
    #[link(name = "kernel32")]
    extern "system" {
        fn GetConsoleWindow() -> *mut core::ffi::c_void;
    }
    #[link(name = "user32")]
    extern "system" {
        fn ShowWindow(hwnd: *mut core::ffi::c_void, cmd_show: i32) -> i32;
    }
    const SW_HIDE: i32 = 0;

    let count = console_process_count();
    // 0 means "no console at all" (started by a GUI parent or with redirected
    // handles); 1 means the console exists only for us.
    if count > 1 {
        return false;
    }
    let hwnd = unsafe { GetConsoleWindow() };
    if hwnd.is_null() {
        return false;
    }
    unsafe {
        ShowWindow(hwnd, SW_HIDE);
    }
    true
}

#[cfg(not(windows))]
pub fn hide_console_if_standalone() -> bool {
    false
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn empty_target_is_rejected() {
        assert!(open_target("   ").is_err());
    }

    #[test]
    fn missing_target_reports_an_error() {
        // Uses a path that cannot exist: this exercises the shell call without
        // opening anything.
        let path =
            std::env::temp_dir().join(format!("aurora-missing-target-{}", std::process::id()));
        let result = open_target(&path.to_string_lossy());
        assert!(result.is_err(), "expected an error for a missing target");
    }
}
