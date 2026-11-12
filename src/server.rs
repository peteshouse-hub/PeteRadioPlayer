use std::io::{self, Read, Write};
use std::net::{TcpListener, TcpStream};
use std::process::Command;
use std::sync::{Arc, atomic::{AtomicBool, Ordering}};
use std::thread;
use std::time::Duration;
use serde_json::{json, Value};
use crate::{Shared, hqplayer, ffmpeg::FfmpegStream, station::Station};

pub fn run(address: &str, state: Shared, running: Arc<AtomicBool>) -> io::Result<()> {
    let listener = TcpListener::bind("0.0.0.0:8765")?;
    listener.set_nonblocking(true)?;
    eprintln!("[RadioKing] RAW HTTP/1.0 audio + GUI ready at http://{address}/");
    while running.load(Ordering::SeqCst) {
        match listener.accept() {
            Ok((socket, _)) => {
                let s = state.clone();
                thread::spawn(move || {
                    if let Err(e) = handle(socket, s) { eprintln!("[RadioKing] request: {e}"); }
                });
            }
            Err(e) if e.kind() == io::ErrorKind::WouldBlock => thread::sleep(Duration::from_millis(20)),
            Err(e) => return Err(e),
        }
    }
    Ok(())
}

fn reply(mut socket: TcpStream, code: &str, mime: &str, body: &[u8]) -> io::Result<()> {
    write!(socket, "HTTP/1.0 {code}\r\nContent-Type: {mime}\r\nCache-Control: no-store\r\nContent-Length: {}\r\nConnection: close\r\n\r\n", body.len())?;
    socket.write_all(body)
}
fn json_reply(socket: TcpStream, data: Value) -> io::Result<()> {
    reply(socket, "200 OK", "application/json; charset=utf-8", data.to_string().as_bytes())
}
fn snapshot(state: &Shared) -> Value {
    let s = state.lock().unwrap();
    json!({"stations": s.stations.iter().enumerate().map(|(i,st)|json!({"id":i,"name":st.name,"group":st.group})).collect::<Vec<_>>(),
        "selected":s.selected,"generation":s.generation,"playing":s.playing,
        "connected":s.connected,"message":s.message,"bytes":s.last_bytes,"retries":s.retry_count,
        "phase":s.phase,"hq_host":s.hq_host,"hq_targets":s.hq_targets.iter().map(|(host,label)|json!({"host":host,"label":label})).collect::<Vec<_>>()})
}
fn decode(s: &str) -> String {
    let bytes=s.as_bytes(); let mut out=Vec::new(); let mut i=0;
    while i<bytes.len() {
        if bytes[i]==b'%' && i+2<bytes.len() {
            if let (Some(a),Some(b))=((bytes[i+1] as char).to_digit(16),(bytes[i+2] as char).to_digit(16)) {
                out.push((a*16+b) as u8);i+=3;continue;
            }
        }
        out.push(if bytes[i]==b'+' {b' '} else {bytes[i]});i+=1;
    }
    String::from_utf8_lossy(&out).into_owned()
}
fn param<'a>(path: &'a str, name: &str) -> Option<String> {
    let query=path.split_once('?')?.1;
    query.split('&').find_map(|pair| {
        let (key,value)=pair.split_once('=')?;
        (key==name).then(||decode(value))
    })
}
fn play(state: &Shared, index: usize) -> Result<(),String> {
    let generation;
    {
        let mut s=state.lock().unwrap();
        if index>=s.stations.len() {return Err("Unknown station".into());}
        s.selected=index;s.generation+=1;generation=s.generation;
        s.playing=true;s.connected=false;s.retry_count=0;s.last_bytes=0;
        s.message=format!("Validating {}…",s.stations[index].name);s.phase="validating".into();
    }
    // Preflight the selected upstream, avoiding HQPlayer decoding an empty stream.
    // FFmpeg's input timeout is bounded by the child watchdog below.
    let station={let s=state.lock().unwrap();s.stations[index].clone()};
    if let Err(e)=preflight(&station) {
        let mut s=state.lock().unwrap();
        if s.generation==generation {s.playing=false;s.phase="error".into();s.message=format!("Source unavailable: {e}");}
        return Err(format!("Source unavailable: {e}"));
    }
    let host={let mut s=state.lock().unwrap();
        if s.generation!=generation {return Err("Playback request superseded".into());}
        s.phase="starting".into();s.message="Starting HQPlayer…".into();s.hq_host.clone()};
    let source_host=match hqplayer::stream_host(&host) {
        Ok(h)=>h,
        Err(e)=>{
            let message=format!("Cannot route stream to {host}: {e}");
            let mut s=state.lock().unwrap();
            if s.generation==generation {s.playing=false;s.phase="error".into();s.message=message.clone();}
            return Err(message);
        }
    };
    let _=hqplayer::stop(&host);
    let uri=format!("http://{source_host}:8765/live.flac?g={generation}");
    if let Err(e)=hqplayer::play(&host,&uri) {
        let mut s=state.lock().unwrap();
        if s.generation==generation {s.playing=false;s.phase="error".into();s.message=format!("HQPlayer control failed: {e}");}
        return Err(format!("HQPlayer control failed: {e}"));
    }
    Ok(())
}
fn preflight(station: &Station) -> Result<(), String> {
    // FFmpeg writes one second of decoded FLAC into a temporary stdout buffer.
    // The process is killed after 12 seconds; an HTTP 403, dead URL or bad codec
    // is reported before disturbing HQPlayer playback.
    use std::process::{Stdio, Command};
    use std::time::Instant;
    let temp_path=format!("/tmp/pete-radio-preflight-{}-{}.flac",std::process::id(),station.name.replace(' ',"_"));
    let _=std::fs::remove_file(&temp_path);
    let mut child=Command::new("/opt/homebrew/bin/ffmpeg")
        .args(["-hide_banner","-loglevel","error","-t","1","-i",&station.source_url,
               "-map","0:a:0","-vn","-c:a","flac","-sample_fmt","s16","-f","flac","-y",&temp_path])
        .stdin(Stdio::null()).stdout(Stdio::null()).stderr(Stdio::null())
        .spawn().map_err(|e|format!("FFmpeg could not start: {e}"))?;
    let started=Instant::now();
    loop {
        match child.try_wait() {
            Ok(Some(status))=>{
                if !status.success(){return Err("BBC or station stream rejected the request (or stream is offline)".into());}
                let bytes=std::fs::read(&temp_path).map_err(|e|e.to_string())?;
                if bytes.starts_with(b"fLaC") && bytes.len()>1024 {return Ok(());}
                return Err("No valid FLAC audio produced".into());
            }
            Ok(None)=>{},
            Err(e)=>return Err(e.to_string()),
        }
        if started.elapsed()>Duration::from_secs(12) {
            let _=child.kill();let _=child.wait();
            let _=std::fs::remove_file(&temp_path);
            return Err("Upstream stream timed out after 12 seconds".into());
        }
        thread::sleep(Duration::from_millis(100));
    }
}

fn stop(state: &Shared) -> Result<(),String> {
    let host={let mut s=state.lock().unwrap();s.generation+=1;s.playing=false;s.connected=false;s.retry_count=0;s.phase="stopping".into();s.message="Stopping…".into();s.hq_host.clone()};
    let result=hqplayer::stop(&host).map_err(|e|e.to_string());
    let mut s=state.lock().unwrap();s.phase=if result.is_ok(){"idle"}else{"error"}.into();
    s.message=if result.is_ok(){"Stopped".into()}else{format!("Stop failed: {}",result.as_ref().unwrap_err())};
    result
}
fn select_hq(state:&Shared,host:&str)->Result<(),String>{
    if host.is_empty() || host.len()>255 || !host.chars().all(|c|c.is_ascii_alphanumeric()||c=='.'||c=='-') {return Err("Invalid HQPlayer host".into());}
    if !hqplayer::reachable(host) {return Err(format!("HQPlayer control port unreachable at {host}:4321"));}
    if hqplayer::info(host).is_none() {return Err(format!("HQPlayer control handshake failed at {host}"));}
    let old={let s=state.lock().unwrap();s.hq_host.clone()};
    if old==host {return Ok(());}
    {let mut s=state.lock().unwrap();s.generation+=1;s.playing=false;s.connected=false;s.phase="switching".into();s.message="Switching HQPlayer…".into();}
    let _=hqplayer::stop(&old);
    let mut s=state.lock().unwrap();s.hq_host=host.into();s.phase="idle".into();s.message=format!("HQPlayer selected: {host}");
    if !s.hq_targets.iter().any(|(h,_)|h==host){s.hq_targets.push((host.into(),format!("{} · manually added",host)));}
    Ok(())
}
fn search(term: &str) -> Result<Vec<Value>,String> {
    if term.trim().len()<2 {return Err("Enter at least two characters".into());}
    let encoded: String=term.bytes().map(|b| if b.is_ascii_alphanumeric(){(b as char).to_string()}else{format!("%{b:02X}")}).collect();
    // Radio Browser is federated; try several mirrors instead of relying on one.
    for host in ["de1.api.radio-browser.info","nl1.api.radio-browser.info","at1.api.radio-browser.info"] {
        let url=format!("https://{host}/json/stations/search?name={encoded}&limit=40&hidebroken=true&order=votes&reverse=true");
        let output=Command::new("/usr/bin/curl").args(["-fLsS","--connect-timeout","4","--max-time","9","-A",concat!("RadioKing/", env!("CARGO_PKG_VERSION")),&url]).output();
        if let Ok(o)=output {
            if o.status.success() {
                if let Ok(Value::Array(items))=serde_json::from_slice::<Value>(&o.stdout) {
                    return Ok(items.into_iter().filter_map(|v| {
                        let url=v.get("url_resolved").and_then(Value::as_str).or_else(||v.get("url").and_then(Value::as_str))?;
                        if !(url.starts_with("http://")||url.starts_with("https://")){return None;}
                        Some(json!({"name":v.get("name").and_then(Value::as_str).unwrap_or("Unknown"),
                            "country":v.get("countrycode").and_then(Value::as_str).unwrap_or(""),
                            "codec":v.get("codec").and_then(Value::as_str).unwrap_or(""),
                            "bitrate":v.get("bitrate").and_then(Value::as_i64).unwrap_or(0),"url":url}))
                    }).collect());
                }
            }
        }
    }
    Err("Global directory unavailable; try again later".into())
}
fn handle(mut socket: TcpStream, state: Shared) -> io::Result<()> {
    // Preserve the crucial 0.2.1 fix: accepted sockets MUST be blocking.
    socket.set_nonblocking(false)?;
    socket.set_nodelay(true)?;
    socket.set_read_timeout(Some(Duration::from_secs(8)))?;
    let mut request=Vec::new();let mut buf=[0u8;1024];
    loop {
        let n=socket.read(&mut buf)?;if n==0{return Ok(());}
        request.extend_from_slice(&buf[..n]);
        if request.windows(4).any(|w|w==b"\r\n\r\n"){break;}
        if request.len()>16384{return Err(io::Error::other("Oversized request"));}
    }
    let line=String::from_utf8_lossy(&request).lines().next().unwrap_or("").to_owned();
    let mut parts=line.split_whitespace();
    let method=parts.next().unwrap_or("");let path=parts.next().unwrap_or("");
    // The HTTP GUI and control endpoints remain localhost-only. Remote HQPlayer
    // instances may access only the FLAC endpoint over the LAN.
    let is_audio=path.starts_with("/live.flac");
    if !is_audio && !socket.peer_addr()?.ip().is_loopback() {
        return reply(socket,"403 Forbidden","text/plain",b"Local control only");
    }
    if method=="HEAD" && path.starts_with("/live.flac") {
        return socket.write_all(b"HTTP/1.0 200 OK\r\nContent-Type: audio/flac\r\nConnection: close\r\n\r\n");
    }
    if method=="GET" && path.starts_with("/live.flac") {return stream_audio(socket,state,path);}
    if method=="GET" && path=="/" {return reply(socket,"200 OK","text/html; charset=utf-8",include_bytes!("index.html"));}
    if method=="GET" && path=="/api/status" {return json_reply(socket,snapshot(&state));}
    if method=="POST" && path.starts_with("/api/hq/select") {
        let host=param(path,"host").unwrap_or_default();
        return json_reply(socket,match select_hq(&state,&host){Ok(())=>json!({"ok":true}),Err(e)=>json!({"ok":false,"error":e})});
    }
    if method=="POST" && path=="/api/hq/refresh" {
        let shared=state.clone();
        thread::spawn(move || {let targets=hqplayer::discover();let mut s=shared.lock().unwrap();s.hq_targets=targets;});
        return json_reply(socket,json!({"ok":true}));
    }
    if method=="POST" && path.starts_with("/api/play") {
        let id=param(path,"id").and_then(|s|s.parse::<usize>().ok()).unwrap_or(usize::MAX);
        return json_reply(socket,match play(&state,id){Ok(())=>json!({"ok":true}),Err(e)=>json!({"ok":false,"error":e})});
    }
    if method=="POST" && path.starts_with("/api/stop") {
        return json_reply(socket,match stop(&state){Ok(())=>json!({"ok":true}),Err(e)=>json!({"ok":false,"error":e})});
    }
    if method=="GET" && path.starts_with("/api/search") {
        let term=param(path,"q").unwrap_or_default();
        return json_reply(socket,match search(&term){Ok(items)=>json!({"ok":true,"stations":items}),Err(e)=>json!({"ok":false,"error":e})});
    }
    if method=="POST" && path.starts_with("/api/add") {
        let name=param(path,"name").unwrap_or_default();
        let url=param(path,"url").unwrap_or_default();
        if name.trim().is_empty() || !(url.starts_with("https://")||url.starts_with("http://")) || url.len()>2048 {
            return json_reply(socket,json!({"ok":false,"error":"Invalid station name or stream URL"}));
        }
        let id={let mut s=state.lock().unwrap();let id=s.stations.len();s.stations.push(Station{name,source_url:url,group:"World".into()});id};
        return json_reply(socket,json!({"ok":true,"id":id}));
    }
    reply(socket,"404 Not Found","text/plain",b"Not found")
}

fn stream_audio(mut socket: TcpStream, state: Shared, path: &str) -> io::Result<()> {
    let generation=param(path,"g").and_then(|s|s.parse::<u64>().ok()).unwrap_or(0);
    let station={let s=state.lock().unwrap();if !s.playing||s.generation!=generation{return reply(socket,"410 Gone","text/plain",b"Old stream");}s.stations[s.selected].clone()};
    socket.set_read_timeout(None)?;
    // Known-good 0.2.1 wire protocol: HTTP/1.0, raw FLAC, no chunked encoding.
    socket.write_all(b"HTTP/1.0 200 OK\r\nContent-Type: audio/flac\r\nCache-Control: no-cache\r\nConnection: close\r\n\r\n")?;
    eprintln!("[RadioKing] live GET: {} (generation {generation})",station.name);
    let mut ffmpeg=match FfmpegStream::start(&station) {
        Ok(p)=>p,
        Err(e)=>{record_failure(&state,generation,format!("FFmpeg start failed: {e}"));return Ok(());}
    };
    let mut buf=[0u8;8192];let mut bytes=0u64;
    let reason=loop {
        {let s=state.lock().unwrap();if !s.playing||s.generation!=generation {break "station changed".to_string();}}
        match ffmpeg.stdout().read(&mut buf) {
            Ok(0)=>break "source ended".to_string(),
            Ok(n)=>{
                if let Err(e)=socket.write_all(&buf[..n]) {break format!("client disconnected: {e}");}
                bytes+=n as u64;
                if bytes<16384||bytes%262144<(n as u64) {
                    let mut s=state.lock().unwrap();if s.generation==generation{s.connected=true;s.phase="playing".into();s.last_bytes=bytes;s.message=format!("Live — {}",station.name);}
                }
            }
            Err(e)=>break format!("FFmpeg pipe error: {e}"),
        }
    };
    eprintln!("[RadioKing] FLAC stream ended ({bytes} bytes): {reason}");
    // Unexpected EOF or lost socket is not always a server fault. Retry is bounded.
    let active={let s=state.lock().unwrap();s.playing&&s.generation==generation};
    if active {record_failure(&state,generation,reason);}
    Ok(())
}
fn record_failure(state: &Shared, generation: u64, reason: String) {
    let (retry, index)={
        let mut s=state.lock().unwrap();
        if !s.playing||s.generation!=generation{return;}
        s.connected=false;s.phase="reconnecting".into();s.retry_count+=1;
        let retry=s.retry_count;
        s.message=if retry<=3 {format!("Stream interrupted ({reason}); retry {retry}/3…")}else{format!("Playback failed after 3 retries: {reason}. Press Play to retry.")};
        if retry>3 {s.playing=false;s.phase="error".into();return;}
        (retry,s.selected)
    };
    let shared=state.clone();
    thread::spawn(move||{
        thread::sleep(Duration::from_secs(2u64.pow(retry.min(3))));
        let still_active={let s=shared.lock().unwrap();s.playing&&s.generation==generation};
        if !still_active{return;}
        // Retain retry budget when initiating an automatic recovery.
        let next={let mut s=shared.lock().unwrap();s.generation+=1;s.generation};
        let host=shared.lock().unwrap().hq_host.clone();
        let source_host=match hqplayer::stream_host(&host) {Ok(h)=>h,Err(e)=>{record_failure(&shared,next,format!("Route failure: {e}"));return;}};
        let _=hqplayer::stop(&host);
        let uri=format!("http://{source_host}:8765/live.flac?g={next}");
        if let Err(e)=hqplayer::play(&host,&uri) {record_failure(&shared,next,format!("HQPlayer reconnect: {e}"));}
        else {let mut s=shared.lock().unwrap();if s.generation==next{s.selected=index;}}
    });
}
