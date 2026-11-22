//! Keeps the app starting and running on CPU when `vulkan-1.dll` is missing (no GPU driver,
//! virtual machines, Remote Desktop) or when Vulkan is switched off with `ALOUDTYPE_NO_VULKAN=1`.
//!
//! The Vulkan build links the GPU backend statically and ggml initialises Vulkan at the first
//! model load, even for CPU decoding. `vulkan-1.dll` is delay-loaded (see `build.rs`), and
//! these delay-load hooks hand ggml a stand-in `vkGetInstanceProcAddr` whose functions all
//! report `VK_ERROR_INITIALIZATION_FAILED`. ggml catches that error and registers no Vulkan
//! backend, so models load on CPU. The GPU probe child does the same and the probe fails, so
//! the GPU is never approved.
//!
//! Only `vkGetInstanceProcAddr` is reached before a Vulkan instance exists; the other imports
//! (`vkCmdCopyBuffer`, `vkGetPhysicalDeviceFeatures2`) need an instance, which is never created.
use std::ffi::{c_char, c_void, CStr};

/// Environment switch that keeps Vulkan unloaded (troubleshooting a crashing GPU driver).
pub const NO_VULKAN_ENV: &str = "ALOUDTYPE_NO_VULKAN";

const VULKAN_DLL: &str = "vulkan-1.dll";

// dliNotify values from delayimp.h.
const DLI_NOTE_PRE_LOAD_LIBRARY: u32 = 1;
const DLI_FAIL_LOAD_LIB: u32 = 3;
const DLI_FAIL_GET_PROC: u32 = 4;

/// `DelayLoadInfo` from delayimp.h (only the fields read here need to be exact).
#[repr(C)]
pub struct DelayLoadInfo {
    cb: u32,
    pidd: *const c_void,
    ppfn: *mut *const c_void,
    sz_dll: *const c_char,
    // DelayLoadProc { BOOL fImportByName; union { LPCSTR szProcName; DWORD dwOrdinal; } }
    f_import_by_name: i32,
    sz_proc_name: *const c_char,
    hmod_cur: *mut c_void,
    pfn_cur: *const c_void,
    dw_last_error: u32,
}

type Hook = unsafe extern "system" fn(u32, *const DelayLoadInfo) -> *const c_void;

#[link(name = "kernel32")]
extern "system" {
    fn GetModuleHandleW(name: *const u16) -> *mut c_void;
}

/// Any Vulkan entry point: reports VK_ERROR_INITIALIZATION_FAILED (-3). Arguments are
/// ignored, which is sound in the x64 calling convention (the caller owns the stack).
extern "system" fn vk_unavailable() -> isize {
    -3
}

/// Stand-in `vkGetInstanceProcAddr`: every function resolves to [`vk_unavailable`].
extern "system" fn stub_get_instance_proc_addr(
    _instance: *mut c_void,
    _name: *const c_char,
) -> *const c_void {
    vk_unavailable as *const c_void
}

unsafe fn is_vulkan(info: *const DelayLoadInfo) -> bool {
    let Some(info) = info.as_ref() else {
        return false;
    };
    !info.sz_dll.is_null()
        && CStr::from_ptr(info.sz_dll)
            .to_str()
            .is_ok_and(|n| n.eq_ignore_ascii_case(VULKAN_DLL))
}

/// Our own module: given in place of vulkan-1.dll, its lookups then fail into [`failure_hook`].
fn this_module() -> *const c_void {
    unsafe { GetModuleHandleW(std::ptr::null()) as *const c_void }
}

unsafe extern "system" fn notify_hook(
    dli_notify: u32,
    info: *const DelayLoadInfo,
) -> *const c_void {
    if dli_notify == DLI_NOTE_PRE_LOAD_LIBRARY
        && is_vulkan(info)
        && std::env::var_os(NO_VULKAN_ENV).is_some_and(|v| !v.is_empty() && v != "0")
    {
        return this_module();
    }
    std::ptr::null()
}

unsafe extern "system" fn failure_hook(
    dli_notify: u32,
    info: *const DelayLoadInfo,
) -> *const c_void {
    if !is_vulkan(info) {
        return std::ptr::null();
    }
    match dli_notify {
        DLI_FAIL_LOAD_LIB => this_module(),
        DLI_FAIL_GET_PROC => {
            let info = &*info;
            let wants_proc_addr = info.f_import_by_name != 0
                && !info.sz_proc_name.is_null()
                && CStr::from_ptr(info.sz_proc_name).to_bytes() == b"vkGetInstanceProcAddr";
            if wants_proc_addr {
                stub_get_instance_proc_addr as *const c_void
            } else {
                vk_unavailable as *const c_void
            }
        }
        _ => std::ptr::null(),
    }
}

// Read by the MSVC delay-load helper (delayimp.lib).
#[no_mangle]
#[used]
pub static __pfnDliNotifyHook2: Hook = notify_hook;
#[no_mangle]
#[used]
pub static __pfnDliFailureHook2: Hook = failure_hook;
