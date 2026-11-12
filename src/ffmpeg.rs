use std::io;
use std::process::{Child, ChildStdout, Command, Stdio};

use crate::station::Station;

pub struct FfmpegStream {
    child: Child,
    stdout: ChildStdout,
}

impl FfmpegStream {
    pub fn start(station: &Station) -> io::Result<Self> {
        eprintln!("[RadioKing] FFmpeg: {} -> FLAC 48 kHz stereo", station.name);

        let child = Command::new("/opt/homebrew/bin/ffmpeg")
            .args([
                "-hide_banner",
                "-loglevel", "warning",
                "-i", station.source_url.as_str(),
                "-map", "0:a:0",
                "-vn",
                "-c:a", "flac",
                "-sample_fmt", "s16",
                "-f", "flac",
                "pipe:1",
            ])
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::inherit())
            .spawn()?;

        Self::from_child(child)
    }

    pub(crate) fn from_child(mut child: Child) -> io::Result<Self> {
        let stdout = child.stdout.take()
            .ok_or_else(|| io::Error::other("FFmpeg stdout unavailable"))?;

        Ok(Self { child, stdout })
    }

    pub fn stdout(&mut self) -> &mut ChildStdout {
        &mut self.stdout
    }
}

impl Drop for FfmpegStream {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}
