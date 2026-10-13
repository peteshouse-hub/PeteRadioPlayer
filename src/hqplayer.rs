use std::io;
use std::process::Command;

const CONTROL: &str = "/Applications/hqp6-control.app/Contents/MacOS/hqp6-control";
const HOST: &str = "127.0.0.1";

fn call(args: &[&str]) -> io::Result<()> {
    let status = Command::new(CONTROL)
        .arg(HOST)
        .args(args)
        .status()?;

    if status.success() {
        Ok(())
    } else {
        Err(io::Error::other(format!("hqp6-control exited with {status}")))
    }
}

pub fn play(uri: &str) -> io::Result<()> {
    call(&["--play-next-uri", uri])?;
    call(&["--play"])
}

pub fn stop() -> io::Result<()> {
    call(&["--stop"])
}
