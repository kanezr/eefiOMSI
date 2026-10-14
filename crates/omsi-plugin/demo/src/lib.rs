//! A plugin with OMSI's plugin interface, for the tests of `omsi-plugin`. Its `.opl`
//! lists `[varlist] 1 throttle`, `[stringvarlist] 1 note`, `[systemvarlist] 1 Time` and
//! `[triggers] 1 horn`: it doubles `throttle`, writes "seen at <Time>" into `note`,
//! remembers `Time` as it last saw it, and holds `horn` down on odd whole seconds.
use std::sync::atomic::{AtomicU32, Ordering};

static TIME: AtomicU32 = AtomicU32::new(0);
static STARTED: AtomicU32 = AtomicU32::new(0);

#[no_mangle]
pub extern "system" fn PluginStart(_owner: *mut std::ffi::c_void) {
    STARTED.fetch_add(1, Ordering::SeqCst);
}

#[no_mangle]
pub extern "system" fn PluginFinalize() {}

/// # Safety
/// `value` and `write` point to a Single and a Boolean, as OMSI passes them.
#[no_mangle]
pub unsafe extern "system" fn AccessVariable(index: u16, value: *mut f32, write: *mut u8) {
    if index == 0 {
        *value *= 2.0;
        *write = 1;
    }
}

/// # Safety
/// As `AccessVariable`.
#[no_mangle]
pub unsafe extern "system" fn AccessSystemVariable(index: u16, value: *mut f32, write: *mut u8) {
    if index == 0 {
        TIME.store((*value).to_bits(), Ordering::SeqCst);
        *write = 0;
    }
}

/// # Safety
/// `text` is a buffer of the text's length + 1 wide characters.
#[no_mangle]
pub unsafe extern "system" fn AccessStringVariable(index: u16, text: *mut u16, write: *mut u8) {
    if index != 0 {
        return;
    }
    let mut len = 0;
    while *text.add(len) != 0 {
        len += 1;
    }
    let t = f32::from_bits(TIME.load(Ordering::SeqCst));
    let msg: Vec<u16> = format!("seen at {t}").encode_utf16().collect();
    // only as much as the buffer holds, as a real plugin must
    let n = msg.len().min(len);
    std::ptr::copy_nonoverlapping(msg.as_ptr(), text, n);
    *text.add(n) = 0;
    *write = 1;
}

/// # Safety
/// `active` points to a Boolean.
#[no_mangle]
pub unsafe extern "system" fn AccessTrigger(index: u16, active: *mut u8) {
    if index == 0 {
        let t = f32::from_bits(TIME.load(Ordering::SeqCst));
        *active = (t as u32 % 2 == 1) as u8;
    }
}

/// Homebrew's i686 MinGW links no unwinder the prebuilt standard library can use; built
/// with `panic=abort` nothing unwinds, and this stands in for the one symbol it names.
#[cfg(all(target_os = "windows", target_arch = "x86", target_env = "gnu"))]
#[no_mangle]
pub extern "C" fn _Unwind_Resume() -> ! {
    std::process::abort()
}
