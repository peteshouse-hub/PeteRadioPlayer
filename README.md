# RadioKing

A lightweight, local macOS radio interface that streams BBC and worldwide radio through **HQPlayer Desktop**, using FFmpeg to decode incoming audio into lossless FLAC over localhost HTTP.

**Development build:** v0.4.0-alpha.12. Previous stable release: PeteRadioPlayer v0.3.2.

## Features

- BBC-first station list, prioritizing international streams
- Worldwide station exploration via Radio Browser
- Local browser GUI
- FFmpeg source preflight, bounded retries and inline playback errors
- Raw FLAC HTTP/1.0 transport with blocking accepted sockets (critical for reliable playback)

## macOS requirements

- macOS on Apple Silicon; Rust/Cargo
- FFmpeg at `/opt/homebrew/bin/ffmpeg`
- For HQPlayer output, HQPlayer Desktop running locally or on a reachable LAN host, with `hqp6-control` at `/Applications/hqp6-control.app/Contents/MacOS/hqp6-control`
- Network connectivity; audio output configured in HQPlayer

## Install the alpha

Download the macOS arm64 ZIP from [GitHub Releases](https://github.com/peteshouse-hub/PeteRadioPlayer/releases), extract it, and open `RadioKing.app`. The ZIP contains the launcher and Rust server. It requires macOS 13 or later and FFmpeg. See Output below for playback requirements. The app is ad hoc signed and unnotarized.

Quit any older RadioKing Live instance before opening RadioKing. Both use TCP 8765. To build and install from source, run `./install-app.sh`.

## Fedora RPM

Install the RPM for your architecture with `sudo dnf install ./radioking-*.rpm` (pass one RPM file). DNF resolves the FFmpeg, FFplay, curl, and browser launcher dependencies from the enabled repositories. Launch **RadioKing** from the application menu and keep its terminal open while listening.

To build an RPM, install `cargo-generate-rpm`, build for a static Linux musl target, and then package it. For example, for x86-64:

```sh
CARGO_TARGET_X86_64_UNKNOWN_LINUX_MUSL_LINKER=rust-lld \
  cargo build --release --locked --target x86_64-unknown-linux-musl
cargo generate-rpm --target x86_64-unknown-linux-musl --metadata-overwrite packaging/fedora/rpm.toml
```

## Linux

Linux uses **Default System Output** through FFplay and binds the web interface to loopback. HQPlayer Desktop controls are available in the macOS build.

Install `ffmpeg`, `ffplay` (often included with FFmpeg), and `curl` on your PATH. RadioKing uses `xdg-open` to launch the browser when available. Run the extracted `radio-king` executable from a terminal, or build from source with Cargo. If the browser does not open, visit [RadioKing on this computer](http://127.0.0.1:8765/). Press Ctrl+C in the terminal to quit.

## Run from source

```sh
cargo run --release
```

The application launches a local browser interface. FFmpeg, FFplay and HQPlayer are external dependencies governed by their respective licences.

## Output

Select **Default Mac Output** from the **Output** menu to use the audio device selected in macOS Sound settings. This mode requires FFplay at `/opt/homebrew/bin/ffplay`. Select an HQPlayer device from the same menu for HQPlayer playback. Live and BBC Rewind work with either output. After switching outputs, press Play or Live to tune again.

## BBC Rewind

Choose **Live** to hear the current broadcast. Choose **Rewind** on a built-in BBC station to hear the London programme scheduled for the current Toronto wall-clock time. The player shows Toronto time, the estimated London programme time, and the applied shift. Rewind is available for the 12 built-in BBC stations. Worldwide search results use live playback.

For a BBC station, drag the **Time shift** slider to tune to a specific delay. **Back** and **Forward** move the programme by one or five minutes. Each adjustment retunes the stream. The slider ranges from the station’s available rewind limit on the left to live on the right. RadioKing refreshes the window while the page is open. It also monitors active rewind playback and retunes five minutes inside the nearest edge if the current delay falls outside the window. For windows shorter than ten minutes, it retunes to the midpoint. RadioKing validates the requested delay before retuning. Playback continues at the current delay if the requested programme is outside that history.

The shift is usually five hours and falls to four when the UK and Canada are on different daylight-saving schedules. RadioKing retains the starting shift during playback and follows HLS media sequences. Select Rewind on the station again to recalculate it after a clock change. RadioKing uses the later occurrence of an ambiguous London time and reports an error for a nonexistent London time or when the requested programme is outside the CDN's rolling window. Startup buffering and HQPlayer latency can put audio slightly behind the displayed programme time.

## Audio architecture

BBC / internet stream → FFmpeg decode → lossless FLAC → localhost HTTP/1.0 → HQPlayer → NAA → DAC

The app does not add DSP, normalization, or resampling. Source audio may already be lossy; FLAC prevents an additional lossy encode, but cannot restore lost information.

## Limitations

- Non-UK BBC stream availability varies; candidate addresses are not guaranteed.
- Preflight checks initial decodability, not long-term reliability.
- A source that hangs without terminating is not yet covered by a full watchdog.
- Radio Browser entries are community-maintained and may be broken or unsuitable.
- The GUI is loopback-only; remote FLAC delivery requires TCP 8765 access on a trusted LAN.

## Development policy

Live Edition v0.3.2 is the stable baseline for the planned separate **PeteRadioPlayerTimelapse** project. Avoid changing the proven FLAC transport without a regression test.

## Licensing

MIT; see LICENSE. HQPlayer and FFmpeg are external dependencies with their own licences.

## Acknowledgements

BBC stream directory reference: https://github.com/groupmsl/BBCRadioStreams

Worldwide station directory: https://www.radio-browser.info/

## Native macOS launcher

The `.app` now contains a minimal Swift/AppKit executable (`RadioKing`) which owns the normal macOS application event loop. It starts the unchanged Rust streaming server (`Contents/Resources/RadioKingServer`) and opens the browser interface. Clicking the Dock icon reopens the browser UI; quitting the app sends SIGINT to the Rust server for its existing HQPlayer shutdown path. Running `cargo run` directly retains the earlier browser-opening behaviour. Requires Xcode Command Line Tools (`swiftc`) to package on macOS.
