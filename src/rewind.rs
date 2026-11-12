use crate::ffmpeg::FfmpegStream;
use chrono::{DateTime, LocalResult, TimeDelta, TimeZone, Utc};
use chrono_tz::{America::Toronto, Europe::London};
use std::{
    error::Error,
    io::{self, Write},
    process::{Command, Stdio},
    sync::mpsc::{self, SyncSender},
    thread,
    time::{Duration, Instant},
};
use url::Url;

type Result<T> = std::result::Result<T, Box<dyn Error>>;

struct Segment<'a> {
    sequence: u64,
    start: DateTime<Utc>,
    end: DateTime<Utc>,
    uri: &'a str,
}

fn playback_target(now: DateTime<Utc>) -> Result<DateTime<Utc>> {
    let wall_time = now.with_timezone(&Toronto).naive_local();
    let target = match London.from_local_datetime(&wall_time) {
        LocalResult::Single(time) => time,
        // Both occurrences precede Toronto's matching wall time. Use the later one.
        LocalResult::Ambiguous(first, second) => first.max(second),
        LocalResult::None => {
            return Err(format!("{wall_time} did not occur in Europe/London").into());
        }
    };
    Ok(target.to_utc())
}

fn parse_playlist(playlist: &str) -> Result<Vec<Segment<'_>>> {
    let mut lines = playlist.lines().map(str::trim);
    if lines.next() != Some("#EXTM3U") {
        return Err("expected an HLS media playlist".into());
    }

    let mut segments = Vec::new();
    let mut start = None;
    let mut duration = None;
    let mut sequence: u64 = 0;
    for line in lines {
        if let Some(value) = line.strip_prefix("#EXT-X-MEDIA-SEQUENCE:") {
            if !segments.is_empty() {
                return Err("media sequence appears after a segment".into());
            }
            sequence = value.parse()?;
        } else if let Some(value) = line.strip_prefix("#EXT-X-PROGRAM-DATE-TIME:") {
            start = Some(DateTime::parse_from_rfc3339(value)?.to_utc());
        } else if let Some(value) = line.strip_prefix("#EXTINF:") {
            let seconds = value.split(',').next().unwrap_or_default().parse()?;
            let delta = TimeDelta::from_std(Duration::try_from_secs_f64(seconds)?)?;
            if delta.is_zero() || duration.replace(delta).is_some() {
                return Err("invalid or repeated EXTINF".into());
            }
        } else if line.starts_with("#EXT-X-DISCONTINUITY")
            && !line.starts_with("#EXT-X-DISCONTINUITY-SEQUENCE:")
        {
            start = None;
        } else if [
            "#EXT-X-KEY:",
            "#EXT-X-BYTERANGE:",
            "#EXT-X-MAP:",
            "#EXT-X-GAP",
        ]
        .iter()
        .any(|tag| line.starts_with(tag))
        {
            return Err(format!("unsupported media tag: {line}").into());
        } else if !line.is_empty() && !line.starts_with('#') {
            let start_time = start.ok_or("segment has no program-date-time anchor")?;
            let delta = duration.take().ok_or("segment has no EXTINF")?;
            let end = start_time
                .checked_add_signed(delta)
                .ok_or("timestamp overflow")?;
            segments.push(Segment {
                sequence,
                start: start_time,
                end,
                uri: line,
            });
            sequence = sequence.checked_add(1).ok_or("media sequence overflow")?;
            start = Some(end);
        }
    }
    if segments.is_empty() || duration.is_some() {
        return Err("empty or incomplete media playlist".into());
    }
    Ok(segments)
}

fn select_segment<'a>(
    segments: &'a [Segment<'_>],
    target: DateTime<Utc>,
) -> Option<&'a Segment<'a>> {
    segments
        .iter()
        .find(|segment| segment.start <= target && target < segment.end)
}

fn download_with_timeout(url: &Url, timeout: &str) -> Result<Vec<u8>> {
    // Use the system HTTPS client. The proof of concept expects direct CDN URLs.
    let response = Command::new(crate::platform::CURL)
        .args([
            "-q",
            "--fail",
            "--location",
            "--max-redirs",
            "0",
            "--silent",
            "--show-error",
            "--max-time",
            timeout,
            "--proto",
            "=https",
            "--",
        ])
        .arg(url.as_str())
        .output()?;
    if !response.status.success() {
        return Err(format!(
            "fetch failed for {url}: {}",
            String::from_utf8_lossy(&response.stderr).trim()
        )
        .into());
    }
    if response.stdout.is_empty() {
        return Err(format!("empty response from {url}").into());
    }
    Ok(response.stdout)
}

const LOOKAHEAD: usize = 3;
const REFRESH: Duration = Duration::from_secs(30);

struct Source {
    url: Url,
    manifest: String,
    refreshed: Instant,
}

impl Source {
    fn open(manifest_url: &str) -> Result<Self> {
        let url = Url::parse(manifest_url)?;
        let manifest = retry(|| Ok(String::from_utf8(download_with_timeout(&url, "3")?)?))?;
        Ok(Self {
            url,
            manifest,
            refreshed: Instant::now(),
        })
    }

    fn refresh(&mut self) -> Result<()> {
        self.manifest = retry(|| Ok(String::from_utf8(download_with_timeout(&self.url, "3")?)?))?;
        self.refreshed = Instant::now();
        Ok(())
    }

    fn segment(
        &mut self,
        sequence: u64,
        expected_start: DateTime<Utc>,
    ) -> Result<(Vec<u8>, DateTime<Utc>)> {
        let deadline = Instant::now() + Duration::from_secs(20);
        let segment = loop {
            if self.refreshed.elapsed() >= REFRESH {
                self.refresh()?;
            }
            let playlist = parse_playlist(&self.manifest)?;
            let index = sequence
                .checked_sub(playlist[0].sequence)
                .ok_or("next segment has expired")?;
            if let Some(segment) = playlist.into_iter().nth(usize::try_from(index)?) {
                break segment;
            }
            if Instant::now() >= deadline {
                return Err("next segment is beyond the playlist".into());
            }
            thread::sleep(Duration::from_secs(1));
            self.refresh()?;
        };
        if segment.start != expected_start {
            return Err(format!(
                "sequence {sequence} starts at {}, expected {expected_start}",
                segment.start
            )
            .into());
        }
        let url = self.url.join(segment.uri)?;
        let bytes = retry(|| download_with_timeout(&url, "3"))?;
        Ok((bytes, segment.end))
    }

    fn pump(
        mut self,
        mut sequence: u64,
        mut start: DateTime<Utc>,
        tx: SyncSender<Vec<u8>>,
    ) -> Result<()> {
        loop {
            let (bytes, end) = self.segment(sequence, start)?;
            if tx.send(bytes).is_err() {
                return Ok(());
            }
            sequence = sequence.checked_add(1).ok_or("media sequence overflow")?;
            start = end;
        }
    }
}

fn retry<T>(mut fetch: impl FnMut() -> Result<T>) -> Result<T> {
    for delay in [250, 500] {
        match fetch() {
            Ok(value) => return Ok(value),
            Err(error) => {
                eprintln!("{error}; retrying in {delay} ms");
                thread::sleep(Duration::from_millis(delay));
            }
        }
    }
    fetch()
}

fn cursor(playlist: &str, target: DateTime<Utc>) -> Result<(u64, DateTime<Utc>)> {
    let segments = parse_playlist(playlist)?;
    let segment = select_segment(&segments, target)
        .ok_or("Requested programme time is outside the available rewind window")?;
    Ok((segment.sequence, segment.start))
}

pub struct Limits {
    pub min: i64,
    pub max: i64,
    pub aligned: Option<i64>,
}

fn limits(playlist: &str, now: DateTime<Utc>) -> Result<Limits> {
    let segments = parse_playlist(playlist)?;
    let buffered_start = segments
        .get(
            segments
                .len()
                .checked_sub(LOOKAHEAD)
                .ok_or("Too few segments to buffer rewind")?,
        )
        .ok_or("Too few segments to buffer rewind")?;
    // Leave room for startup buffering and for the playlist to advance between requests.
    let min = ((now - buffered_start.end).num_seconds() + 7).max(1);
    let max = (now - segments[0].start).num_seconds() - 15;
    if max < min {
        return Err("The available rewind window is too short".into());
    }
    let aligned = playback_target(now)
        .ok()
        .map(|target| (now - target).num_seconds());
    Ok(Limits { min, max, aligned })
}

pub fn available_limits(manifest: &str) -> Result<Limits> {
    let source = Source::open(manifest)?;
    limits(&source.manifest, Utc::now())
}

pub fn validate(manifest: &str, requested: Option<i64>) -> Result<i64> {
    let source = Source::open(manifest)?;
    let now = Utc::now();
    let delay = match requested {
        Some(delay) => {
            let limits = limits(&source.manifest, now)?;
            if !(limits.min..=limits.max).contains(&delay) {
                return Err("Requested delay is outside the available rewind window".into());
            }
            delay
        }
        None => (now - playback_target(now)?).num_seconds(),
    };
    cursor(&source.manifest, now - TimeDelta::seconds(delay))?;
    Ok(delay)
}

pub fn start(manifest: &str, delay: i64) -> Result<FfmpegStream> {
    let source = Source::open(manifest)?;
    let target = Utc::now() - TimeDelta::seconds(delay);
    let (sequence, start) = cursor(&source.manifest, target)?;
    let offset = (target - start).to_std()?.as_secs_f64();
    eprintln!(
        "[RadioKing] Rewind starts at {target} (sequence {sequence}, {offset:.3}s into segment)"
    );

    let mut decoder = Command::new(crate::platform::FFMPEG);
    decoder.args([
        "-hide_banner",
        "-nostdin",
        "-loglevel",
        "warning",
        "-xerror",
        "-re",
    ]);
    // The Homebrew FFmpeg used by macOS supports read-rate bursts. Linux
    // distro builds may predate that option, so let -re pace the initial trim.
    #[cfg(target_os = "macos")]
    decoder.args(["-readrate_initial_burst", &format!("{:.6}", offset + 1.0)]);
    let mut child = decoder
        .args([
            "-f",
            "mpegts",
            "-i",
            "pipe:0",
            "-ss",
            &format!("{offset:.6}"),
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
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .spawn()?;
    let mut input = child.stdin.take().expect("piped FFmpeg input");
    let playback = FfmpegStream::from_child(child)?;
    let (tx, rx) = mpsc::sync_channel(LOOKAHEAD);
    thread::spawn(move || {
        if let Err(error) = source.pump(sequence, start, tx) {
            eprintln!("[RadioKing] Rewind source: {error}");
        }
    });
    thread::spawn(move || {
        let feed = || -> io::Result<()> {
            let initial = (0..LOOKAHEAD)
                .map(|_| rx.recv())
                .collect::<std::result::Result<Vec<_>, _>>()
                .map_err(io::Error::other)?;
            for bytes in initial.into_iter().chain(rx) {
                input.write_all(&bytes)?;
            }
            Ok(())
        };
        if let Err(error) = feed()
            && error.kind() != io::ErrorKind::BrokenPipe
        {
            eprintln!("[RadioKing] Rewind input: {error}");
        }
    });
    Ok(playback)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn utc(value: &str) -> DateTime<Utc> {
        value.parse().unwrap()
    }

    #[test]
    fn maps_normal_and_mismatched_dst_offsets() {
        for (now, hours) in [
            ("2026-01-15T18:00:00Z", 5),
            ("2026-03-15T18:00:00Z", 4),
            ("2026-07-15T18:00:00Z", 5),
            ("2026-10-28T18:00:00Z", 4),
            ("2026-11-08T02:00:00Z", 5),
        ] {
            let now = utc(now);
            assert_eq!(playback_target(now).unwrap(), now - TimeDelta::hours(hours));
        }
    }

    #[test]
    fn london_fold_uses_later_occurrence_and_gap_is_reported() {
        assert_eq!(
            playback_target(utc("2026-10-25T05:30:00Z")).unwrap(),
            utc("2026-10-25T01:30:00Z")
        );
        assert!(playback_target(utc("2026-03-29T05:30:00Z")).is_err());
    }

    #[test]
    fn toronto_fold_repeats_wall_time_and_gap_skips_it() {
        assert_eq!(
            playback_target(utc("2026-11-01T05:30:00Z")).unwrap(),
            playback_target(utc("2026-11-01T06:30:00Z")).unwrap()
        );
        let before = playback_target(utc("2026-03-08T06:59:59Z")).unwrap();
        let after = playback_target(utc("2026-03-08T07:00:00Z")).unwrap();
        assert_eq!(after - before, TimeDelta::seconds(3601));
    }

    #[test]
    fn selects_half_open_segments_and_honors_new_anchors() {
        let segments = parse_playlist(concat!(
            "#EXTM3U\n#EXT-X-PROGRAM-DATE-TIME:2026-11-08T13:42:17.600Z\n",
            "#EXTINF:6.4,\na.ts\n#EXTINF:6.4,\nb.ts\n",
            "#EXT-X-DISCONTINUITY\n#EXT-X-PROGRAM-DATE-TIME:2026-11-08T13:43:00Z\n",
            "#EXTINF:6.4,\nc.ts\n"
        ))
        .unwrap();
        assert!(select_segment(&segments, utc("2026-11-08T13:42:17.599Z")).is_none());
        assert_eq!(
            select_segment(&segments, utc("2026-11-08T13:42:24Z"))
                .unwrap()
                .uri,
            "b.ts"
        );
        assert!(select_segment(&segments, utc("2026-11-08T13:42:30.400Z")).is_none());
        assert_eq!(
            select_segment(&segments, utc("2026-11-08T13:43:00Z"))
                .unwrap()
                .uri,
            "c.ts"
        );
        assert!(select_segment(&segments, utc("2026-11-08T13:43:06.400Z")).is_none());
    }
}
