# PeteRadioPlayer — Live Edition

A lightweight, local macOS radio interface that streams BBC and worldwide radio through **HQPlayer Desktop**, using FFmpeg to decode incoming audio into lossless FLAC over localhost HTTP.

**Release:** v0.3.2 (listening-tested on the original developer’s Mac; not a broadly validated release).

## Features

- BBC-first station list, prioritizing international streams
- Worldwide station exploration via Radio Browser
- Local browser GUI
- FFmpeg source preflight, bounded retries and inline playback errors
- Raw FLAC HTTP/1.0 transport with blocking accepted sockets (critical for reliable playback)

## Requirements

- macOS on Apple Silicon; Rust/Cargo
- FFmpeg at `/opt/homebrew/bin/ffmpeg`
- HQPlayer Desktop running locally, with `hqp6-control` at `/Applications/hqp6-control.app/Contents/MacOS/hqp6-control`
- Network connectivity; audio output configured in HQPlayer

## Run

```sh
cargo run --release
```

The application launches a local browser interface. It is not an independent audio player: HQPlayer is required and is **not** bundled. FFmpeg and HQPlayer are separate third-party software governed by their respective licences.

## Audio architecture

BBC / internet stream → FFmpeg decode → lossless FLAC → localhost HTTP/1.0 → HQPlayer → NAA → DAC

The app does not add DSP, normalization, or resampling. Source audio may already be lossy; FLAC prevents an additional lossy encode, but cannot restore lost information.

## Limitations

- Non-UK BBC stream availability varies; candidate addresses are not guaranteed.
- Preflight checks initial decodability, not long-term reliability.
- A source that hangs without terminating is not yet covered by a full watchdog.
- Radio Browser entries are community-maintained and may be broken or unsuitable.
- The localhost HTTP server is intended for local use, not exposure to untrusted networks.

## Development policy

Live Edition v0.3.2 is the stable baseline for the planned separate **PeteRadioPlayerTimelapse** project. Avoid changing the proven FLAC transport without a regression test.

## Licensing

**Licence selection pending.** No open-source licence is granted by this repository until a licence file is added. Choose an appropriate licence before making the repository public or accepting contributions.

## Acknowledgements

BBC stream directory reference: https://github.com/groupmsl/BBCRadioStreams

Worldwide station directory: https://www.radio-browser.info/
