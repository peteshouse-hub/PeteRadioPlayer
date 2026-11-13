use std::io::{self, Read, Write};
use std::net::{TcpListener, TcpStream};
use std::process::Command;
use std::sync::{Arc, atomic::{AtomicBool, Ordering}};
use std::thread;
use std::time::Duration;
use serde_json::{json, Value};
#[cfg(target_os = "macos")]
use crate::hqplayer;
use crate::{MAC_OUTPUT, Shared, platform, rewind, ffmpeg::FfmpegStream, station::Station};

pub fn run(listener: TcpListener, state: Shared, running: Arc<AtomicBool>) -> io::Result<()> {
    listener.set_nonblocking(true)?;
    let monitor_state = state.clone();
    let monitor_running = running.clone();
    thread::spawn(move || watch_rewind_window(&monitor_state, &monitor_running));
    eprintln!("[RadioKing] RAW HTTP/1.0 audio + GUI ready at http://127.0.0.1:8765/");
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
    json!({"stations": s.stations.iter().enumerate().map(|(i,st)|json!({"id":i,"name":st.name,"group":st.group,"rewind":st.rewind_url.is_some()})).collect::<Vec<_>>(),
        "selected":s.selected,"generation":s.generation,"playing":s.playing,
        "stream_stats":s.connected.then_some(&s.stream_stats),"connected":s.connected,"message":s.message,"bytes":s.last_bytes,"retries":s.retry_count,
        "phase":s.phase,"rewind_delay":s.rewind_delay.filter(|_| s.playing),"hq_supported":platform::HQPLAYER,"system_output":platform::SYSTEM_OUTPUT,"hq_host":s.hq_host,"hq_targets":s.hq_targets.iter().map(|(host,label)|json!({"host":host,"label":label})).collect::<Vec<_>>()})
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
fn play(state: &Shared, index: usize, use_rewind: bool, requested_delay: Option<i64>) -> Result<(),String> {
    let custom_delay = if let Some(delay) = requested_delay {
        let (manifest, generation) = {
            let s = state.lock().unwrap();
            let url = s.stations.get(index).and_then(|station| station.rewind_url.clone())
                .ok_or("Rewind is unavailable for this station")?;
            (url, s.generation)
        };
        let delay = rewind::validate(&manifest, Some(delay))
            .map_err(|error| format!("Rewind unavailable: {error}"))?;
        Some((delay, generation))
    } else {
        None
    };
    let (generation, station);
    {
        let mut s=state.lock().unwrap();
        if custom_delay.is_some_and(|(_, previous)| s.generation != previous) {
            return Err("Playback request superseded".into());
        }
        if index>=s.stations.len() {return Err("Unknown station".into());}
        station=s.stations[index].clone();
        if use_rewind && station.rewind_url.is_none() {
            return Err("Rewind is unavailable for this station".into());
        }
        stop_mac_player(&mut s);
        s.rewind_delay=None;
        s.selected=index;s.generation+=1;generation=s.generation;
        s.playing=true;s.connected=false;s.retry_count=0;s.last_bytes=0;
        s.message=format!("Validating {}…",s.stations[index].name);s.phase="validating".into();
    }
    // Preflight the selected upstream, avoiding HQPlayer decoding an empty stream.
    // FFmpeg's input timeout is bounded by the child watchdog below.
    let preflight_result = if let Some((delay, _)) = custom_delay {
        Ok(Some(delay))
    } else if use_rewind {
        rewind::validate(station.rewind_url.as_deref().expect("verified rewind URL"), None)
            .map(Some).map_err(|error| format!("Rewind unavailable: {error}"))
    } else {
        preflight(&station).map(|()| None)
    };
    let delay = match preflight_result {
        Ok(delay) => delay,
        Err(e) => {
            let mut s=state.lock().unwrap();
            if s.generation==generation {s.playing=false;s.phase="error".into();s.message=format!("Source unavailable: {e}");}
            return Err(format!("Source unavailable: {e}"));
        }
    };
    let output = {
        let mut s = state.lock().unwrap();
        if s.generation != generation { return Err("Playback request superseded".into()); }
        s.rewind_delay = delay;
        s.phase = "starting".into();
        s.message = format!("Starting {}…", output_label(&s.hq_host));
        s.hq_host.clone()
    };
    if let Err(error) = start_output(state, generation, &output) {
        let mut s = state.lock().unwrap();
        if s.generation == generation {
            s.playing = false;
            s.phase = "error".into();
            s.message = format!("Output failed: {error}");
        }
        return Err(error);
    }
    Ok(())
}

fn output_label(output: &str) -> &str {
    if output == MAC_OUTPUT { platform::SYSTEM_OUTPUT } else { "HQPlayer" }
}

fn stop_mac_player(state: &mut crate::State) {
    if let Some(mut player) = state.mac_player.take() {
        let _ = player.kill();
        let _ = player.wait();
    }
}

fn start_output(state: &Shared, generation: u64, output: &str) -> Result<(), String> {
    if output != MAC_OUTPUT {
        #[cfg(target_os = "macos")]
        {
            let source = hqplayer::stream_host(output).map_err(|e| format!("Cannot route stream to {output}: {e}"))?;
            let _ = hqplayer::stop(output);
            let uri = format!("http://{source}:8765/live.flac?g={generation}");
            return hqplayer::play(output, &uri).map_err(|error| error.to_string());
        }
        #[cfg(not(target_os = "macos"))]
        return Err("HQPlayer Desktop is unavailable on this platform".into());
    }

    let mut s = state.lock().unwrap();
    if s.generation != generation || !s.playing { return Err("Playback request superseded".into()); }
    stop_mac_player(&mut s);
    let uri = format!("http://127.0.0.1:8765/live.flac?g={generation}");
    let mut command = Command::new(platform::FFPLAY);
    #[cfg(target_os = "macos")]
    command.env("SDL_AUDIODRIVER", "coreaudio");
    let player = command
        .args(["-hide_banner", "-loglevel", "error", "-nostats", "-nodisp", "-autoexit", "-i", &uri])
        .stdin(std::process::Stdio::null())
        .spawn().map_err(|error| format!("Could not start FFplay. Install FFmpeg with FFplay support: {error}"))?;
    s.mac_player = Some(player);
    drop(s);

    let shared = state.clone();
    thread::spawn(move || loop {
        thread::sleep(Duration::from_millis(200));
        let mut s = shared.lock().unwrap();
        if !s.playing || s.generation != generation { return; }
        let Some(player) = s.mac_player.as_mut() else { return; };
        match player.try_wait() {
            Ok(None) => continue,
            Ok(Some(_)) => {
                s.mac_player = None;
                drop(s);
                record_failure(&shared, generation, "System audio player exited".into());
            }
            Err(error) => {
                stop_mac_player(&mut s);
                drop(s);
                record_failure(&shared, generation, format!("System audio player: {error}"));
            }
        }
        return;
    });
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
    let mut child=Command::new(crate::platform::FFMPEG)
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

pub fn stop(state: &Shared) -> Result<(),String> {
    let (generation, _output, _was_playing) = {
        let mut s = state.lock().unwrap();
        let was_playing = s.playing;
        s.generation += 1;
        s.playing = false;
        s.connected = false;
        s.retry_count = 0;
        s.phase = "stopping".into();
        s.message = "Stopping…".into();
        stop_mac_player(&mut s);
        (s.generation, s.hq_host.clone(), was_playing)
    };
    #[cfg(target_os = "macos")]
    let result = if _output != MAC_OUTPUT && _was_playing {
        hqplayer::stop(&_output).map_err(|e| e.to_string())
    } else {
        Ok(())
    };
    #[cfg(not(target_os = "macos"))]
    let result: Result<(), String> = Ok(());
    let mut s = state.lock().unwrap();
    if s.generation == generation {
        s.phase = if result.is_ok() { "idle" } else { "error" }.into();
        s.message = match &result {
            Ok(()) => "Stopped".into(),
            Err(error) => format!("Stop failed: {error}"),
        };
    }
    result
}

fn select_hq(state:&Shared,host:&str)->Result<(),String>{
    #[cfg(not(target_os = "macos"))]
    if host != MAC_OUTPUT { return Err("HQPlayer Desktop is unavailable on this platform".into()); }
    #[cfg(target_os = "macos")]
    if host != MAC_OUTPUT {
        if host.is_empty() || host.len()>255 || !host.chars().all(|c|c.is_ascii_alphanumeric()||c=='.'||c=='-') {return Err("Invalid HQPlayer host".into());}
        if !hqplayer::reachable(host) {return Err(format!("HQPlayer control port unreachable at {host}:4321"));}
        if hqplayer::info(host).is_none() {return Err(format!("HQPlayer control handshake failed at {host}"));}
    }
    let (generation, _old, _was_playing) = {
        let mut s = state.lock().unwrap();
        if s.hq_host == host { return Ok(()); }
        let was_playing = s.playing;
        s.generation += 1;
        s.playing = false;
        s.connected = false;
        s.phase = "switching".into();
        s.message = "Switching output…".into();
        stop_mac_player(&mut s);
        (s.generation, s.hq_host.clone(), was_playing)
    };
    #[cfg(target_os = "macos")]
    if _old != MAC_OUTPUT && _was_playing { let _ = hqplayer::stop(&_old); }
    let mut s = state.lock().unwrap();
    if s.generation != generation { return Err("Output selection superseded".into()); }
    s.hq_host = host.into();
    s.phase = "idle".into();
    s.message = if host == MAC_OUTPUT { format!("{} selected", platform::SYSTEM_OUTPUT) } else { format!("HQPlayer selected: {host}") };
    if host != MAC_OUTPUT && !s.hq_targets.iter().any(|(h,_)|h==host){s.hq_targets.push((host.into(),format!("{} · manually added",host)));}
    Ok(())
}
fn directory(path: &str) -> Result<Vec<Value>, String> {
    for host in ["de1.api.radio-browser.info", "nl1.api.radio-browser.info", "at1.api.radio-browser.info"] {
        let url = format!("https://{host}/json/{path}");
        let output=Command::new(crate::platform::CURL).args(["-fLsS","--connect-timeout","4","--max-time","9","-A",concat!("RadioKing/", env!("CARGO_PKG_VERSION")),&url]).output();
        if let Ok(o)=output {
            if o.status.success() {
                if let Ok(Value::Array(items))=serde_json::from_slice::<Value>(&o.stdout) {
                    return Ok(items);
                }
            }
        }
    }
    Err("Global directory unavailable; try again later".into())
}
fn directory_stations(filters: &[(&str, &str)]) -> Result<Vec<Value>, String> {
    let query: String = url::form_urlencoded::Serializer::new(String::new())
        .extend_pairs(filters.iter().copied()).finish();
    let items = directory(&format!("stations/search?{query}&tagExact=true&languageExact=true&limit=40&hidebroken=true&order=votes&reverse=true"))?;
    Ok(items.into_iter().filter_map(|v| {
        let url=v.get("url_resolved").and_then(Value::as_str).or_else(||v.get("url").and_then(Value::as_str))?;
        if !(url.starts_with("http://")||url.starts_with("https://")){return None;}
        Some(json!({"name":v.get("name").and_then(Value::as_str).unwrap_or("Unknown"),
            "country":v.get("countrycode").and_then(Value::as_str).unwrap_or(""),
            "codec":v.get("codec").and_then(Value::as_str).unwrap_or(""),
            "bitrate":v.get("bitrate").and_then(Value::as_i64).unwrap_or(0),"url":url,"rewind":crate::station::directory_rewind_url(url).is_some()}))
    }).collect())
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
    #[cfg(target_os = "macos")]
    if method=="POST" && path=="/api/hq/refresh" {
        let shared=state.clone();
        thread::spawn(move || {let targets=hqplayer::discover();let mut s=shared.lock().unwrap();s.hq_targets=targets;});
        return json_reply(socket,json!({"ok":true}));
    }
    if method=="GET" && path.split('?').next()==Some("/api/rewind/limits") {
        let index=param(path,"id").and_then(|value| value.parse::<usize>().ok());
        let manifest = {
            let s = state.lock().unwrap();
            index.and_then(|id| s.stations.get(id)).and_then(|station| station.rewind_url.clone())
        };
        let result = match manifest {
            Some(url) => match rewind::available_limits(&url) {
                Ok(limits) => json!({"ok":true,"min":limits.min,"max":limits.max,"aligned":limits.aligned}),
                Err(error) => json!({"ok":false,"error":error.to_string()}),
            },
            None => json!({"ok":false,"error":"Rewind is unavailable for this station"}),
        };
        return json_reply(socket,result);
    }
    if method=="POST" && path.starts_with("/api/play") {
        let id=param(path,"id").and_then(|s|s.parse::<usize>().ok()).unwrap_or(usize::MAX);
        let use_rewind=param(path,"mode").as_deref()==Some("rewind");
        let requested_delay=match param(path,"delay") {
            Some(value) => match value.parse::<i64>() {
                Ok(delay) if use_rewind && (1..=86_400).contains(&delay) => Some(delay),
                _ => return json_reply(socket,json!({"ok":false,"error":"Enter a rewind delay from 1 second to 24 hours"})),
            },
            None => None,
        };
        return json_reply(socket,match play(&state,id,use_rewind,requested_delay){Ok(())=>json!({"ok":true}),Err(e)=>json!({"ok":false,"error":e})});
    }
    if method=="POST" && path.starts_with("/api/stop") {
        return json_reply(socket,match stop(&state){Ok(())=>json!({"ok":true}),Err(e)=>json!({"ok":false,"error":e})});
    }
    if method == "GET" && path.split('?').next() == Some("/api/browse") {
        let kind = param(path, "kind").unwrap_or_default();
        let (list, key, field) = match kind.as_str() {
            "genre" => ("tags", "name", "tag"),
            "region" => ("countries", "iso_3166_1", "countrycode"),
            "language" => ("languages", "name", "language"),
            _ => return json_reply(socket, json!({"ok":false,"error":"Unknown browse category"})),
        };
        let response = match param(path, "value").filter(|value| !value.is_empty()) {
            Some(value) => {
                let filter = param(path, "name").unwrap_or_default();
                let mut filters = vec![(field, value.as_str())];
                if !filter.trim().is_empty() {
                    filters.push(("name", filter.trim()));
                }
                directory_stations(&filters).map(|stations| json!({"ok":true,"stations":stations}))
            }
            None => directory(&format!("{list}?hidebroken=true&order=stationcount&reverse=true&limit=250"))
                .map(|items| {
                    let options: Vec<_> = items.iter().filter_map(|item| {
                        let name = item["name"].as_str()?.trim();
                        let value = item[key].as_str()?.trim();
                        if name.is_empty() || value.is_empty() { return None; }
                        Some(json!({"name":name,"value":value}))
                    }).collect();
                    json!({"ok":true,"options":options})
                }),
        };
        return json_reply(socket, response.unwrap_or_else(|error| json!({"ok":false,"error":error})));
    }
    if method=="GET" && path.starts_with("/api/search") {
        let term=param(path,"q").unwrap_or_default();
        let result = if term.trim().len() < 2 {
            Err("Enter at least two characters".into())
        } else {
            directory_stations(&[("name", term.trim())])
        };
        return json_reply(socket,match result{Ok(items)=>json!({"ok":true,"stations":items}),Err(e)=>json!({"ok":false,"error":e})});
    }
    if method=="POST" && path.starts_with("/api/add") {
        let name=param(path,"name").unwrap_or_default();
        let url=param(path,"url").unwrap_or_default();
        if name.trim().is_empty() || !(url.starts_with("https://")||url.starts_with("http://")) || url.len()>2048 {
            return json_reply(socket,json!({"ok":false,"error":"Invalid station name or stream URL"}));
        }
        let rewind_url = crate::station::directory_rewind_url(&url);
        let id = {
            let mut s = state.lock().unwrap();
            if let Some(id) = s.stations.iter().position(|station| station.source_url == url && station.name == name) {
                id
            } else {
                let id = s.stations.len();
                s.stations.push(Station { name, source_url: url, group: "World".into(), rewind_url });
                id
            }
        };
        return json_reply(socket,json!({"ok":true,"id":id}));
    }
    reply(socket,"404 Not Found","text/plain",b"Not found")
}

fn stream_audio(mut socket: TcpStream, state: Shared, path: &str) -> io::Result<()> {
    let generation=param(path,"g").and_then(|s|s.parse::<u64>().ok()).unwrap_or(0);
    let (station, delay)={let s=state.lock().unwrap();if !s.playing||s.generation!=generation{return reply(socket,"410 Gone","text/plain",b"Old stream");}(s.stations[s.selected].clone(),s.rewind_delay)};
    socket.set_read_timeout(None)?;
    // Known-good 0.2.1 wire protocol: HTTP/1.0, raw FLAC, no chunked encoding.
    socket.write_all(b"HTTP/1.0 200 OK\r\nContent-Type: audio/flac\r\nCache-Control: no-cache\r\nConnection: close\r\n\r\n")?;
    eprintln!("[RadioKing] live GET: {} (generation {generation})",station.name);
    let playback = match (delay, station.rewind_url.as_deref()) {
        (Some(delay), Some(url)) => rewind::start(url, delay),
        _ => FfmpegStream::start(&station).map_err(Into::into),
    };
    let mut ffmpeg=match playback {
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
                    let mut s=state.lock().unwrap();if s.generation==generation{s.connected=true;s.phase="playing".into();s.last_bytes=bytes;s.stream_stats=ffmpeg.stats();s.message=format!("{} — {}",if delay.is_some() {"Rewind"} else {"Live"},station.name);}
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
fn rewind_window_adjustment(state: &Shared, generation: u64, phase: &str) -> Option<i64> {
    let (url, delay) = {
        let s = state.lock().unwrap();
        if !s.playing || s.generation != generation || s.phase != phase {
            return None;
        }
        (s.stations[s.selected].rewind_url.clone()?, s.rewind_delay?)
    };
    rewind::available_limits(&url).ok()?.recovery_delay(delay)
}

fn watch_rewind_window(state: &Shared, running: &AtomicBool) {
    while running.load(Ordering::SeqCst) {
        thread::sleep(Duration::from_secs(15));
        if !running.load(Ordering::SeqCst) { break; }
        let generation = state.lock().unwrap().generation;
        if let Some(delay) = rewind_window_adjustment(state, generation, "playing")
            && running.load(Ordering::SeqCst)
        {
            restart_output(state, generation, "playing", Some(delay));
        }
    }
}

fn restart_output(state: &Shared, generation: u64, phase: &str, adjusted_delay: Option<i64>) {
    let (next, output) = {
        let mut s = state.lock().unwrap();
        if !s.playing || s.generation != generation || s.phase != phase { return; }
        stop_mac_player(&mut s);
        if let Some(delay) = adjusted_delay {
            eprintln!("[RadioKing] Rewind window moved; resuming {delay} seconds behind live");
            s.rewind_delay = Some(delay);
            s.message = "Rewind window moved. Retuning…".into();
        }
        s.generation += 1;
        s.connected = false;
        s.last_bytes = 0;
        s.phase = "starting".into();
        (s.generation, s.hq_host.clone())
    };
    if let Err(error) = start_output(state, next, &output) {
        record_failure(state, next, format!("Output reconnect: {error}"));
    }
}

fn record_failure(state: &Shared, generation: u64, reason: String) {
    let retry={
        let mut s=state.lock().unwrap();
        if !s.playing || s.generation != generation || s.phase == "reconnecting" { return; }
        stop_mac_player(&mut s);
        s.connected=false;s.phase="reconnecting".into();s.retry_count+=1;
        let retry=s.retry_count;
        s.message=if retry<=3 {format!("Stream interrupted ({reason}); retry {retry}/3…")}else{format!("Playback failed after 3 retries: {reason}. Press Play to retry.")};
        if retry>3 {s.playing=false;s.phase="error".into();return;}
        retry
    };
    let shared=state.clone();
    thread::spawn(move||{
        thread::sleep(Duration::from_secs(2u64.pow(retry.min(3))));
        let delay = rewind_window_adjustment(&shared, generation, "reconnecting");
        restart_output(&shared, generation, "reconnecting", delay);
    });
}
