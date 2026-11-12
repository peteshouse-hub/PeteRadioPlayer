use std::io::{self, BufRead, BufReader};
use std::sync::{Arc, Mutex};

use serde_json::{Value, json};
use std::process::{Child, ChildStdout, Command, Stdio};

use crate::station::Station;

pub struct FfmpegStream {
    child: Child,
    stdout: ChildStdout,
    stats: Arc<Mutex<Value>>,
}

impl FfmpegStream {
    pub fn start(station: &Station) -> io::Result<Self> {
        eprintln!("[RadioKing] FFmpeg: {} -> FLAC", station.name);

        let child = Command::new(crate::platform::FFMPEG)
            .args([
                "-hide_banner",
                "-loglevel",
                "info",
                "-nostats",
                "-i",
                station.source_url.as_str(),
                "-map",
                "0:a:0",
                "-vn",
                "-c:a",
                "flac",
                "-sample_fmt",
                "s16",
                "-f",
                "flac",
                "pipe:1",
            ])
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()?;

        Self::from_child(child)
    }

    pub(crate) fn from_child(mut child: Child) -> io::Result<Self> {
        let stdout = child
            .stdout
            .take()
            .ok_or_else(|| io::Error::other("FFmpeg stdout unavailable"))?;

        let stats = Arc::new(Mutex::new(json!({})));
        if let Some(stderr) = child.stderr.take() {
            let stats = stats.clone();
            std::thread::spawn(move || {
                let mut output = false;
                for line in BufReader::new(stderr).lines().map_while(Result::ok) {
                    if line.starts_with("Output #0") {
                        output = true;
                    }
                    if let Some(audio) = line
                        .trim_start()
                        .strip_prefix("Stream #")
                        .and_then(|line| line.split_once(": Audio: ").map(|(_, audio)| audio))
                    {
                        let key = if output { "output" } else { "source" };
                        let mut stats = stats.lock().unwrap();
                        if stats.get(key).is_none() {
                            stats[key] = audio_stats(audio, output);
                        }
                    }
                    eprintln!("{line}");
                }
            });
        }
        Ok(Self {
            child,
            stdout,
            stats,
        })
    }

    pub fn stats(&self) -> Value {
        self.stats.lock().unwrap().clone()
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

fn audio_stats(description: &str, output: bool) -> Value {
    let mut fields = description.split(", ");
    let codec = fields.next().unwrap_or_default();
    let sample_rate = fields.find(|field| field.ends_with(" Hz"));
    let channels = fields.next();
    let remaining: Vec<_> = fields.collect();
    let bitrate = remaining.iter().find(|field| field.ends_with(" kb/s"));
    let bit_depth = if output {
        Some(16)
    } else if codec.starts_with("pcm_") || codec.starts_with("flac") || codec.starts_with("alac") {
        remaining.iter().find_map(|field| {
            let (format, _) = field.split_once(' ').unwrap_or((field, ""));
            if let Some((_, bits)) = field.split_once('(')
                && let Some(bits) = bits.strip_suffix(" bit)")
            {
                return bits.parse().ok();
            }
            // s32 can be a container for a smaller coded depth.
            (format == "s16").then_some(16)
        })
    } else {
        None
    };
    json!({"codec":codec,"sample_rate":sample_rate,"channels":channels,"bitrate":bitrate,"bit_depth":bit_depth})
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn distinguishes_codec_sample_format_from_source_bit_depth() {
        let aac = audio_stats("aac (HE-AAC), 48000 Hz, stereo, fltp, 96 kb/s", false);
        assert_eq!(aac["bitrate"], "96 kb/s");
        assert_eq!(aac["sample_rate"], "48000 Hz");
        assert!(aac["bit_depth"].is_null());
        let flac = audio_stats("flac, 48000 Hz, stereo, s32 (24 bit)", false);
        assert_eq!(flac["bit_depth"], 24);
        assert_eq!(
            audio_stats("flac, 48000 Hz, stereo, s16", true)["bit_depth"],
            16
        );
    }
}
