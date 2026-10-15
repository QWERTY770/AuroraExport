use std::fmt;
use std::io;

pub struct ProcessPermissions {
    pub elevated: bool,
    pub restricted: bool,
    pub app_container: bool,
}

impl fmt::Display for ProcessPermissions {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            formatter,
            "elevated={} restricted={} app_container={}",
            self.elevated, self.restricted, self.app_container
        )
    }
}

#[cfg(not(windows))]
pub fn process_permissions() -> io::Result<Option<ProcessPermissions>> {
    Ok(None)
}

#[cfg(windows)]
pub fn process_permissions() -> io::Result<Option<ProcessPermissions>> {
    use std::ffi::c_void;

    #[link(name = "kernel32")]
    extern "system" {
        fn GetCurrentProcess() -> *mut c_void;
        fn CloseHandle(handle: *mut c_void) -> i32;
    }
    #[link(name = "advapi32")]
    extern "system" {
        fn OpenProcessToken(process: *mut c_void, access: u32, token: *mut *mut c_void) -> i32;
        fn GetTokenInformation(
            token: *mut c_void,
            class: u32,
            value: *mut c_void,
            length: u32,
            returned: *mut u32,
        ) -> i32;
        fn IsTokenRestricted(token: *mut c_void) -> i32;
    }

    struct Token(*mut c_void);
    impl Drop for Token {
        fn drop(&mut self) {
            unsafe {
                CloseHandle(self.0);
            }
        }
    }

    fn flag(token: &Token, class: u32) -> io::Result<bool> {
        let mut value = 0u32;
        let mut returned = 0u32;
        let success = unsafe {
            GetTokenInformation(
                token.0,
                class,
                (&mut value as *mut u32).cast(),
                std::mem::size_of::<u32>() as u32,
                &mut returned,
            )
        };
        if success == 0 {
            Err(io::Error::last_os_error())
        } else {
            Ok(value != 0)
        }
    }

    const TOKEN_QUERY: u32 = 0x0008;
    const TOKEN_ELEVATION: u32 = 20;
    const TOKEN_IS_APP_CONTAINER: u32 = 29;
    let mut handle = std::ptr::null_mut();
    if unsafe { OpenProcessToken(GetCurrentProcess(), TOKEN_QUERY, &mut handle) } == 0 {
        return Err(io::Error::last_os_error());
    }
    let token = Token(handle);
    Ok(Some(ProcessPermissions {
        elevated: flag(&token, TOKEN_ELEVATION)?,
        restricted: unsafe { IsTokenRestricted(token.0) } != 0,
        app_container: flag(&token, TOKEN_IS_APP_CONTAINER)?,
    }))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn permissions_summary_keeps_elevation_separate_from_restrictions() {
        let permissions = ProcessPermissions {
            elevated: false,
            restricted: true,
            app_container: false,
        };
        assert_eq!(
            permissions.to_string(),
            "elevated=false restricted=true app_container=false"
        );
    }

    #[cfg(windows)]
    #[test]
    fn current_process_token_can_be_queried_without_elevation() {
        assert!(process_permissions().unwrap().is_some());
    }
}
