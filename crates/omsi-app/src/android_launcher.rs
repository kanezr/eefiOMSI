//! The launcher page on a phone: the desktop launcher's page (`launcher/ui`, in the APK's
//! assets) in a web view over the game's window (`LauncherBridge.java`), its commands
//! answered here by the same code the desktop launcher runs (`omsi-launcher-api`). A duty it
//! starts is handed to this process as before (`omsi_launcher_lib::launch` keeps the command
//! line, `android::Shell::switch` takes it): the web view steps aside while the session runs
//! and comes back when it ends. The bus preview and the city map are drawn by the launcher
//! service on a thread of this process (`launcher::InProcess`).

use jni::objects::{GlobalRef, JByteArray, JClass, JObject, JString, JValue};
use jni::sys::{jbyteArray, jstring};
use jni::JNIEnv;
use serde_json::Value;
use std::sync::OnceLock;
use std::time::Duration;

/// The launcher page is shown (the APK carries it): the game's own launcher is not made at
/// all then - no window of its own, no renderer, no shaders built under a page that hides
/// it. Off for good once the player picks the game's own launcher instead.
static WEB: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);

pub(crate) fn set_web(on: bool) {
    WEB.store(on, std::sync::atomic::Ordering::Relaxed);
}

pub(crate) fn web() -> bool {
    WEB.load(std::sync::atomic::Ordering::Relaxed)
}

/// `LauncherBridge.closed()`: the page went away for the game's own launcher.
#[no_mangle]
pub extern "system" fn Java_org_openomsi_game_LauncherBridge_closed<'a>(_env: JNIEnv<'a>, _class: JClass<'a>) {
    set_web(false);
}

/// `LauncherBridge`, held from its first call (a thread of this program cannot find an app
/// class by name: the system's class loader does not know them).
static BRIDGE: OnceLock<GlobalRef> = OnceLock::new();

fn service() -> &'static super::launcher::InProcess {
    static S: OnceLock<super::launcher::InProcess> = OnceLock::new();
    S.get_or_init(|| {
        super::launcher::install();
        super::launcher::InProcess::start()
    })
}

/// Call a static `void name(String...)` of `LauncherBridge` from any thread.
fn bridge(name: &str, args: &[&str]) {
    let Some(class) = BRIDGE.get() else { return };
    let ctx = ndk_context::android_context();
    // SAFETY: the VM ndk_context was given lives as long as the program
    let Ok(vm) = (unsafe { jni::JavaVM::from_raw(ctx.vm().cast()) }) else { return };
    let Ok(mut env) = vm.attach_current_thread() else { return };
    let strings: Vec<JObject> = args.iter().filter_map(|a| env.new_string(a).ok().map(JObject::from)).collect();
    let values: Vec<JValue> = strings.iter().map(JValue::Object).collect();
    let sig = format!("({})V", "Ljava/lang/String;".repeat(values.len()));
    // SAFETY: the global reference is the class `LauncherBridge`
    let class = unsafe { JClass::from_raw(class.as_obj().as_raw()) };
    if env.call_static_method(&class, name, sig, &values).is_err() {
        let _ = env.exception_clear();
    }
    std::mem::forget(class);
}

/// The session ended: the launcher page comes back (and hears that no game runs any more).
pub(crate) fn session_ended() {
    bridge("sessionEnded", &[]);
}

struct Phone;

impl omsi_launcher_api::Platform for Phone {
    fn emit(&self, event: &str, payload: Value) {
        bridge("emit", &[event, &payload.to_string()]);
    }
    fn launched(&self) {
        bridge("launched", &[]);
    }
    fn renderer(&self, req: Value, wait: Duration) -> Result<(Value, Vec<u8>), String> {
        service().ask(req, wait)
    }
}

fn take(env: &mut JNIEnv, s: &JString) -> String {
    env.get_string(s).map(Into::into).unwrap_or_default()
}

/// `LauncherBridge.call(cmd, args)`: a command of the page (on the bridge's worker thread);
/// "1" and the JSON answer, or "0" and the error.
#[no_mangle]
pub extern "system" fn Java_org_openomsi_game_LauncherBridge_call<'a>(mut env: JNIEnv<'a>, class: JClass<'a>, cmd: JString<'a>, args: JString<'a>) -> jstring {
    if BRIDGE.get().is_none() {
        if let Ok(g) = env.new_global_ref(&class) {
            let _ = BRIDGE.set(g);
        }
    }
    let cmd = take(&mut env, &cmd);
    let args: Value = serde_json::from_str(&take(&mut env, &args)).unwrap_or(Value::Null);
    let out = match omsi_launcher_api::call(&cmd, &args, &Phone) {
        Ok(v) => format!("1{v}"),
        Err(e) => format!("0{e}"),
    };
    env.new_string(out).map(|s| s.into_raw()).unwrap_or(std::ptr::null_mut())
}

/// `LauncherBridge.bytes(cmd, args)`: a command answering bytes (the preview's pictures);
/// the bytes after a 1, or a 0 and the error's text.
#[no_mangle]
pub extern "system" fn Java_org_openomsi_game_LauncherBridge_bytes<'a>(mut env: JNIEnv<'a>, _class: JClass<'a>, cmd: JString<'a>, args: JString<'a>) -> jbyteArray {
    let cmd = take(&mut env, &cmd);
    let args: Value = serde_json::from_str(&take(&mut env, &args)).unwrap_or(Value::Null);
    let out = match omsi_launcher_api::call_bytes(&cmd, &args, &Phone) {
        Ok(b) => [&[1u8][..], &b].concat(),
        Err(e) => [&[0u8][..], e.as_bytes()].concat(),
    };
    env.byte_array_from_slice(&out).map(JByteArray::into_raw).unwrap_or(std::ptr::null_mut())
}
