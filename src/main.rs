mod ffmpeg;
mod rewind;
#[cfg(target_os = "macos")]
mod hqplayer;
mod platform;
mod station;
mod server;

use std::sync::{Arc, Mutex, atomic::{AtomicBool, Ordering}};
use std::thread;
use std::time::Duration;
use station::{Station, bbc};

pub const MAC_OUTPUT: &str = "@mac";

pub struct State {
    pub stations: Vec<Station>,
    pub selected: usize,
    pub generation: u64,
    pub playing: bool,
    pub connected: bool,
    pub message: String,
    pub last_bytes: u64,
    pub stream_stats: serde_json::Value,
    pub retry_count: u32,
    pub hq_host: String,
    pub mac_player: Option<std::process::Child>,
    pub hq_targets: Vec<(String,String)>,
    pub phase: String,
    pub rewind_delay: Option<i64>,
}

pub type Shared = Arc<Mutex<State>>;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    println!("RadioKing {} — BBC + World Explorer", env!("CARGO_PKG_VERSION"));
    let state: Shared = Arc::new(Mutex::new(State {
        stations: bbc(), selected: 0, generation: 0, playing: false,
        connected: false, message: "Ready — choose a station".into(), last_bytes: 0, stream_stats: serde_json::Value::Null,
        retry_count: 0, hq_host: if platform::HQPLAYER { "127.0.0.1" } else { MAC_OUTPUT }.into(),
        mac_player: None, hq_targets: Vec::new(), phase: "idle".into(), rewind_delay: None,
    }));
    let running = Arc::new(AtomicBool::new(true));
    let signal = running.clone();
    ctrlc::set_handler(move || signal.store(false, Ordering::SeqCst))?;
    let listener = std::net::TcpListener::bind(platform::BIND_ADDRESS)?;
    let shared = state.clone();
    let flag = running.clone();
    #[cfg(target_os = "macos")]
    let discovery_state=state.clone();
    #[cfg(target_os = "macos")]
    let discovery_running=running.clone();
    #[cfg(target_os = "macos")]
    let discovery=thread::spawn(move || {
        while discovery_running.load(Ordering::SeqCst) {
            let found=hqplayer::discover();
            {
                let mut s=discovery_state.lock().unwrap();
                let selected=s.hq_host.clone();
                s.hq_targets=found;
                if selected != MAC_OUTPUT && s.hq_targets.is_empty() && s.phase == "idle" {s.message="No HQPlayer Desktop found — refresh or enter a host".into();}
                if selected != MAC_OUTPUT && !s.hq_targets.iter().any(|(h,_)| h==&selected) && s.phase == "idle" {
                    if let Some((h,_))=s.hq_targets.first() {s.hq_host=h.clone();}
                }
            }
            for _ in 0..300 {if !discovery_running.load(Ordering::SeqCst){return;}thread::sleep(Duration::from_millis(100));}
        }
    });
    let worker = thread::spawn(move || server::run(listener, shared, flag));
    thread::sleep(Duration::from_millis(250));
    println!("Open http://127.0.0.1:8765/ in your browser");
    if std::env::var_os("RADIOKING_NATIVE_LAUNCHER").is_none() {
        thread::spawn(|| {
            let _ = std::process::Command::new(platform::BROWSER)
                .arg("http://127.0.0.1:8765/")
                .status();
        });
    }
    while running.load(Ordering::SeqCst) { thread::sleep(Duration::from_millis(200)); }
    let _ = server::stop(&state);
    #[cfg(target_os = "macos")]
    let _=discovery.join();
    match worker.join() {
        Ok(Ok(())) => Ok(()),
        Ok(Err(e)) => Err(e.into()),
        Err(_) => Err("Server thread panicked".into()),
    }
}
