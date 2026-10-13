mod ffmpeg;
mod hqplayer;
mod station;
mod server;

use std::sync::{Arc, Mutex, atomic::{AtomicBool, Ordering}};
use std::thread;
use std::time::Duration;
use station::{Station, bbc};

#[derive(Clone)]
pub struct State {
    pub stations: Vec<Station>,
    pub selected: usize,
    pub generation: u64,
    pub playing: bool,
    pub connected: bool,
    pub message: String,
    pub last_bytes: u64,
    pub retry_count: u32,
}

pub type Shared = Arc<Mutex<State>>;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    println!("PeteRadioPlayer 0.3.2 — International BBC + World Explorer");
    let state: Shared = Arc::new(Mutex::new(State {
        stations: bbc(), selected: 0, generation: 0, playing: false,
        connected: false, message: "Ready — choose a station".into(), last_bytes: 0,
        retry_count: 0,
    }));
    let running = Arc::new(AtomicBool::new(true));
    let signal = running.clone();
    ctrlc::set_handler(move || signal.store(false, Ordering::SeqCst))?;
    let shared = state.clone();
    let flag = running.clone();
    let worker = thread::spawn(move || server::run("127.0.0.1:8765", shared, flag));
    thread::sleep(Duration::from_millis(250));
    println!("Open http://127.0.0.1:8765/ in your browser");
    let _ = std::process::Command::new("open").arg("http://127.0.0.1:8765/").status();
    while running.load(Ordering::SeqCst) { thread::sleep(Duration::from_millis(200)); }
    let _ = hqplayer::stop();
    match worker.join() {
        Ok(Ok(())) => Ok(()),
        Ok(Err(e)) => Err(e.into()),
        Err(_) => Err("Server thread panicked".into()),
    }
}
