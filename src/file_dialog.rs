use std::path::{Path, PathBuf};

mod paths;
pub use paths::{probe_directory, selected_path, writable_directory};

#[derive(Clone, Copy)]
pub enum Purpose {
    ExportExcel,
    ExportUigf,
    ImportJson,
}

pub struct Request {
    pub purpose: Purpose,
    pub title: String,
    pub directory: PathBuf,
    pub file_name: String,
    pub extension: &'static str,
}

impl Request {
    pub fn is_save(&self) -> bool {
        !matches!(self.purpose, Purpose::ImportJson)
    }
}

pub fn export_directory(data_directory: &Path) -> PathBuf {
    let fallback = data_directory.join("export");
    for directory in [
        dirs::download_dir(),
        dirs::document_dir(),
        dirs::home_dir(),
        Some(fallback.clone()),
    ]
    .into_iter()
    .flatten()
    {
        if directory == fallback {
            let _ = std::fs::create_dir_all(&directory);
        }
        if writable_directory(&directory) {
            return directory;
        }
    }
    data_directory.to_path_buf()
}

#[cfg(not(windows))]
pub fn show(request: Request, frame: &eframe::Frame) -> Result<Option<PathBuf>, String> {
    let dialog = rfd::FileDialog::new()
        .set_parent(frame)
        .set_title(&request.title)
        .set_directory(&request.directory)
        .add_filter(request.extension.to_uppercase(), &[request.extension]);
    let path = if request.is_save() {
        dialog.set_file_name(&request.file_name).save_file()
    } else {
        dialog.pick_file()
    };
    Ok(path)
}

#[cfg(windows)]
pub fn show(request: Request, frame: &eframe::Frame) -> Result<Option<PathBuf>, String> {
    windows_dialog(request, frame).map_err(|error| error.to_string())
}

#[cfg(windows)]
fn windows_dialog(
    request: Request,
    frame: &eframe::Frame,
) -> windows::core::Result<Option<PathBuf>> {
    use raw_window_handle::{HasWindowHandle, RawWindowHandle};
    use std::os::windows::ffi::{OsStrExt, OsStringExt};
    use windows::core::{Interface, PCWSTR};
    use windows::Win32::Foundation::{ERROR_CANCELLED, HWND};
    use windows::Win32::System::Com::{CoCreateInstance, CoTaskMemFree, CLSCTX_INPROC_SERVER};
    use windows::Win32::UI::Shell::Common::COMDLG_FILTERSPEC;
    use windows::Win32::UI::Shell::{
        FileOpenDialog, FileSaveDialog, IFileDialog, IFileOpenDialog, IFileSaveDialog, IShellItem,
        SHCreateItemFromParsingName, FOS_FILEMUSTEXIST, FOS_FORCEFILESYSTEM, FOS_NOTESTFILECREATE,
        FOS_OVERWRITEPROMPT, FOS_PATHMUSTEXIST, SIGDN_FILESYSPATH,
    };

    let _com = crate::platform::ComApartment::new()?;
    let wide = |text: &str| text.encode_utf16().chain(Some(0)).collect::<Vec<_>>();
    let owner = frame
        .window_handle()
        .ok()
        .and_then(|handle| match handle.as_raw() {
            RawWindowHandle::Win32(handle) => Some(HWND(handle.hwnd.get() as *mut _)),
            _ => None,
        });
    unsafe {
        let dialog: IFileDialog = if request.is_save() {
            let save: IFileSaveDialog =
                CoCreateInstance(&FileSaveDialog, None, CLSCTX_INPROC_SERVER)?;
            save.cast()?
        } else {
            let open: IFileOpenDialog =
                CoCreateInstance(&FileOpenDialog, None, CLSCTX_INPROC_SERVER)?;
            open.cast()?
        };
        let mut options = dialog.GetOptions()? | FOS_FORCEFILESYSTEM | FOS_PATHMUSTEXIST;
        options |= if request.is_save() {
            FOS_OVERWRITEPROMPT | FOS_NOTESTFILECREATE
        } else {
            FOS_FILEMUSTEXIST
        };
        dialog.SetOptions(options)?;
        let title = wide(&request.title);
        dialog.SetTitle(PCWSTR(title.as_ptr()))?;
        let label = wide(&request.extension.to_uppercase());
        let pattern = wide(&format!("*.{}", request.extension));
        dialog.SetFileTypes(&[COMDLG_FILTERSPEC {
            pszName: PCWSTR(label.as_ptr()),
            pszSpec: PCWSTR(pattern.as_ptr()),
        }])?;
        let extension = wide(request.extension);
        dialog.SetDefaultExtension(PCWSTR(extension.as_ptr()))?;
        if request.is_save() {
            let name = wide(&request.file_name);
            dialog.SetFileName(PCWSTR(name.as_ptr()))?;
        }
        let directory = request
            .directory
            .as_os_str()
            .encode_wide()
            .chain(Some(0))
            .collect::<Vec<_>>();
        if let Ok(folder) =
            SHCreateItemFromParsingName::<_, _, IShellItem>(PCWSTR(directory.as_ptr()), None)
        {
            dialog.SetFolder(&folder)?;
        }
        if let Err(error) = dialog.Show(owner) {
            if error.code() == windows::core::HRESULT::from_win32(ERROR_CANCELLED.0) {
                return Ok(None);
            }
            return Err(error);
        }
        let item = dialog.GetResult()?;
        let name = item.GetDisplayName(SIGDN_FILESYSPATH)?;
        let mut length = 0;
        while *name.0.add(length) != 0 {
            length += 1;
        }
        let path = std::ffi::OsString::from_wide(std::slice::from_raw_parts(name.0, length));
        CoTaskMemFree(Some(name.0.cast()));
        Ok(Some(PathBuf::from(path)))
    }
}
