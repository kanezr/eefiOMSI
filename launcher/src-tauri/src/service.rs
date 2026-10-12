//! The game program as the launcher's helper (`openomsi --launcher-service`, no window): the
//! bus preview drawn by the game's own renderer, as the game's launcher window draws it, and
//! the city map with a trip's route read the way the navigator reads it. Started once, on the
//! first request, and again if it ended; requests carry a number and the answers find their
//! request by it, so a map being read does not hold up the pictures.

use serde_json::{json, Value};
use std::collections::HashMap;
use std::io::{BufRead, BufReader, Read, Write};
use std::process::{Child, ChildStdin, Command, Stdio};
use std::sync::mpsc::{channel, Sender};
use std::sync::{Arc, Mutex};
use std::time::Duration;

type Pending = Arc<Mutex<HashMap<u64, Sender<(Value, Vec<u8>)>>>>;

struct Running {
    child: Child,
    stdin: ChildStdin,
    pending: Pending,
}

static SERVICE: Mutex<Option<Running>> = Mutex::new(None);
static NEXT: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(1);

fn start() -> Result<Running, String> {
    let game = omsi_launcher_lib::game_binary().ok_or("the game program was not found")?;
    let log = std::fs::File::create(omsi_launcher_lib::data_dir().join("launcher-service.log")).map_err(|e| e.to_string())?;
    let mut child = Command::new(&game)
        .arg("--launcher-service")
        .env("RUST_LOG", "info")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::from(log))
        .spawn()
        .map_err(|e| format!("starting {}: {e}", game.display()))?;
    let stdin = child.stdin.take().ok_or("no stdin")?;
    let stdout = child.stdout.take().ok_or("no stdout")?;
    let pending: Pending = Arc::default();
    let p = pending.clone();
    std::thread::Builder::new()
        .name("launcher service".into())
        .spawn(move || {
            let mut r = BufReader::new(stdout);
            let mut line = String::new();
            loop {
                line.clear();
                if r.read_line(&mut line).unwrap_or(0) == 0 {
                    break;
                }
                let Ok(head) = serde_json::from_str::<Value>(line.trim()) else { continue };
                let mut body = vec![0u8; head.get("len").and_then(Value::as_u64).unwrap_or(0) as usize];
                if r.read_exact(&mut body).is_err() {
                    break;
                }
                let id = head.get("id").and_then(Value::as_u64).unwrap_or(0);
                if let Some(tx) = p.lock().unwrap_or_else(|e| e.into_inner()).remove(&id) {
                    let _ = tx.send((head, body));
                }
            }
            // (the service ended: whoever still waits hears it at once)
            p.lock().unwrap_or_else(|e| e.into_inner()).clear();
        })
        .map_err(|e| e.to_string())?;
    omsi_launcher_lib::log_to_file(&format!("launcher service started: {}", game.display()));
    Ok(Running { child, stdin, pending })
}

/// Ask the service; its answer and the bytes after it.
pub fn ask(mut req: Value, wait: Duration) -> Result<(Value, Vec<u8>), String> {
    let id = NEXT.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
    req["id"] = json!(id);
    let (tx, rx) = channel();
    {
        let mut s = SERVICE.lock().unwrap_or_else(|e| e.into_inner());
        if s.as_mut().is_some_and(|r| r.child.try_wait().ok().flatten().is_some()) {
            *s = None;
        }
        if s.is_none() {
            *s = Some(start()?);
        }
        let r = s.as_mut().unwrap();
        r.pending.lock().unwrap_or_else(|e| e.into_inner()).insert(id, tx);
        if writeln!(r.stdin, "{req}").and_then(|_| r.stdin.flush()).is_err() {
            *s = None;
            return Err("the preview helper ended".into());
        }
    }
    let (head, body) = rx.recv_timeout(wait).map_err(|_| "the preview helper did not answer".to_string())?;
    if head.get("ok").and_then(Value::as_bool) == Some(false) {
        return Err(head.get("error").and_then(Value::as_str).unwrap_or("the preview helper failed").to_string());
    }
    Ok((head, body))
}

/// End the service with the launcher.
pub fn stop() {
    if let Some(mut r) = SERVICE.lock().unwrap_or_else(|e| e.into_inner()).take() {
        let _ = writeln!(r.stdin, "{}", json!({ "cmd": "quit" }));
        let _ = r.stdin.flush();
        std::thread::sleep(Duration::from_millis(100));
        let _ = r.child.kill();
    }
}
