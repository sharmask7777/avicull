use std::fs::File;
use std::io::Read;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::thread::sleep;
use std::time::Duration;

use super::local::LocalVolumeBackend;
use super::{FileLocality, StorageBackend, StorageKind};

/// macOS CloudStorage provider (OneDrive, Dropbox, Google Drive, iCloud via FileProvider).
#[derive(Debug, Default, Clone, Copy)]
pub struct MacOSCloudStorageBackend;

impl MacOSCloudStorageBackend {
    pub fn new() -> Self {
        Self
    }
}

impl StorageBackend for MacOSCloudStorageBackend {
    fn kind(&self) -> StorageKind {
        StorageKind::MacOSCloudStorage
    }

    fn inspect_locality(&self, path: &Path) -> std::io::Result<FileLocality> {
        LocalVolumeBackend.inspect_locality(path)
    }

    fn materialize(&self, path: &Path) -> std::io::Result<PathBuf> {
        let mut last_err = None;
        for attempt in 0..2 {
            if attempt > 0 {
                sleep(Duration::from_millis(500));
            }
            match File::open(path).and_then(|mut f| {
                let mut buf = [0u8; 4096];
                let _ = f.read(&mut buf)?;
                Ok(())
            }) {
                Ok(_) => return Ok(path.to_path_buf()),
                Err(err) => {
                    last_err = Some(err);
                }
            }
        }

        Err(last_err.unwrap_or_else(|| {
            std::io::Error::other(format!("Failed to materialize {}", path.display()))
        }))
    }

    fn evict(&self, path: &Path) -> std::io::Result<()> {
        #[cfg(target_os = "macos")]
        {
            if let Ok(()) = unsafe { macos_evict_ubiquitous_item(path) } {
                return Ok(());
            }
        }

        let path_str = path.to_string_lossy();
        let status = if path_str.contains("/Library/Mobile Documents/com~apple~CloudDocs") {
            Command::new("brctl").arg("evict").arg(path).status()
        } else {
            Command::new("fileproviderctl")
                .arg("evict")
                .arg(path)
                .status()
        };

        match status {
            Ok(s) if s.success() => Ok(()),
            Ok(s) => Err(std::io::Error::other(format!(
                "Eviction failed with exit status: {}",
                s
            ))),
            Err(e) => Err(e),
        }
    }

    fn rename_asset(&self, src: &Path, dst: &Path) -> std::io::Result<()> {
        std::fs::rename(src, dst)
    }
}

#[cfg(target_os = "macos")]
unsafe fn macos_evict_ubiquitous_item(path: &Path) -> std::io::Result<()> {
    use std::ffi::CString;

    #[link(name = "objc")]
    extern "C" {
        fn objc_getClass(name: *const libc::c_char) -> *mut libc::c_void;
        fn sel_registerName(name: *const libc::c_char) -> *mut libc::c_void;
    }

    type MsgSendFn = unsafe extern "C" fn(*mut libc::c_void, *mut libc::c_void) -> *mut libc::c_void;
    type MsgSend1Fn =
        unsafe extern "C" fn(*mut libc::c_void, *mut libc::c_void, *mut libc::c_void) -> *mut libc::c_void;
    type MsgSend2BoolFn = unsafe extern "C" fn(
        *mut libc::c_void,
        *mut libc::c_void,
        *mut libc::c_void,
        *mut *mut libc::c_void,
    ) -> bool;

    let objc_msgsend_sym = libc::dlsym(libc::RTLD_DEFAULT, b"objc_msgSend\0".as_ptr() as *const _);
    if objc_msgsend_sym.is_null() {
        return Err(std::io::Error::other("objc_msgSend not found"));
    }

    let objc_msg_send: MsgSendFn = std::mem::transmute(objc_msgsend_sym);
    let objc_msg_send1: MsgSend1Fn = std::mem::transmute(objc_msgsend_sym);
    let objc_msg_send2_bool: MsgSend2BoolFn = std::mem::transmute(objc_msgsend_sym);

    let ns_file_manager_class = objc_getClass(b"NSFileManager\0".as_ptr() as *const _);
    if ns_file_manager_class.is_null() {
        return Err(std::io::Error::other("NSFileManager class not found"));
    }
    let default_manager_sel = sel_registerName(b"defaultManager\0".as_ptr() as *const _);
    let file_manager = objc_msg_send(ns_file_manager_class, default_manager_sel);
    if file_manager.is_null() {
        return Err(std::io::Error::other("defaultManager returned null"));
    }

    let ns_string_class = objc_getClass(b"NSString\0".as_ptr() as *const _);
    let string_sel = sel_registerName(b"stringWithUTF8String:\0".as_ptr() as *const _);
    let c_path = CString::new(path.to_string_lossy().as_bytes())
        .map_err(|e| std::io::Error::new(std::io::ErrorKind::InvalidInput, e))?;
    let ns_path = objc_msg_send1(ns_string_class, string_sel, c_path.as_ptr() as *mut _);

    let ns_url_class = objc_getClass(b"NSURL\0".as_ptr() as *const _);
    let url_sel = sel_registerName(b"fileURLWithPath:\0".as_ptr() as *const _);
    let ns_url = objc_msg_send1(ns_url_class, url_sel, ns_path);

    let evict_sel = sel_registerName(b"evictUbiquitousItemAtURL:error:\0".as_ptr() as *const _);
    let mut error: *mut libc::c_void = std::ptr::null_mut();
    let success = objc_msg_send2_bool(file_manager, evict_sel, ns_url, &mut error);

    if success {
        Ok(())
    } else {
        Err(std::io::Error::other(format!(
            "NSFileManager evict returned false for {}",
            path.display()
        )))
    }
}
