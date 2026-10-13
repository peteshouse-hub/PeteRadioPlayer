use std::io;
use std::process::{Command, Stdio};
use std::net::{TcpStream, UdpSocket, ToSocketAddrs};
use std::time::Duration;

const CONTROL: &str = "/Applications/hqp6-control.app/Contents/MacOS/hqp6-control";

pub fn call(host: &str, args: &[&str]) -> io::Result<()> {
    let status = Command::new(CONTROL).arg(host).args(args).status()?;
    if status.success() { Ok(()) } else { Err(io::Error::other(format!("hqp6-control at {host}: {status}"))) }
}
pub fn play(host: &str, uri: &str) -> io::Result<()> {
    call(host, &["--play-next-uri", uri])?;
    call(host, &["--play"])
}
pub fn stop(host: &str) -> io::Result<()> { call(host, &["--stop"]) }

pub fn info(host: &str) -> Option<String> {
    let output = Command::new(CONTROL).arg(host).arg("--get-info")
        .stdout(Stdio::piped()).stderr(Stdio::piped()).output().ok()?;
    if !output.status.success() { return None; }
    let a=String::from_utf8_lossy(&output.stdout).trim().to_string();
    let b=String::from_utf8_lossy(&output.stderr).trim().to_string();
    Some(if a.is_empty() {b} else {a})
}
pub fn reachable(host: &str) -> bool {
    let Ok(addresses)=format!("{host}:4321").to_socket_addrs() else {return false};
    addresses.into_iter().any(|addr|TcpStream::connect_timeout(&addr,Duration::from_millis(700)).is_ok())
}
pub fn stream_host(target: &str) -> io::Result<String> {
    if target=="127.0.0.1" || target=="localhost" {return Ok("127.0.0.1".into());}
    let sock=UdpSocket::bind("0.0.0.0:0")?;
    sock.connect(format!("{target}:4321"))?;
    Ok(sock.local_addr()?.ip().to_string())
}

// HQPlayer 6 Desktop exposes a UPnP MediaRenderer. We use SSDP to discover
// candidate hosts, but verify each candidate through the actual control API.
// This is bounded discovery, not a LAN-wide port scan.
pub fn discover() -> Vec<(String,String)> {
    let mut hosts=vec!["127.0.0.1".to_string()];
    if let Ok(sock)=UdpSocket::bind("0.0.0.0:0") {
        let _=sock.set_read_timeout(Some(Duration::from_millis(550)));
        let _=sock.set_multicast_ttl_v4(2);
        let query=b"M-SEARCH * HTTP/1.1\r\nHOST: 239.255.255.250:1900\r\nMAN: \"ssdp:discover\"\r\nMX: 1\r\nST: urn:schemas-upnp-org:device:MediaRenderer:1\r\n\r\n";
        for _ in 0..2 {let _=sock.send_to(query,"239.255.255.250:1900");}
        let mut buf=[0u8;8192];
        for _ in 0..24 {
            match sock.recv_from(&mut buf) {
                Ok((n,addr))=>{
                    let body=String::from_utf8_lossy(&buf[..n]).to_ascii_lowercase();
                    if body.contains("mediarenderer") || body.contains("upnp") {
                        let host=addr.ip().to_string();
                        if !hosts.contains(&host) {hosts.push(host);}
                    }
                }
                Err(_)=>break,
            }
        }
    }
    let mut found=Vec::new();
    for host in hosts {
        if !reachable(&host) {continue;}
        if let Some(info)=info(&host) {
            if info.contains("HQPlayer") || info.contains("Signalyst") {
                let label=if host=="127.0.0.1" {format!("This Mac · {}",info)} else {format!("{} · {}",host,info)};
                found.push((host,label));
            }
        }
    }
    found
}
