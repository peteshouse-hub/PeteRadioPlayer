# RadioKing 0.4.0-alpha.8

Development branch: `live-v0.4`. **Do not replace the released v0.3.2 app yet.**

## What changed

- HQPlayer Desktop target discovery using SSDP MediaRenderer advertisements,
  verified using the HQPlayer control utility on TCP 4321.
- Manual hostname/IP entry if SSDP is blocked by the router, firewall, or VPN.
- HQPlayer selection and connection feedback in the local browser UI.
- Control requests and FLAC URLs follow the selected HQPlayer instance.
- A remote HQPlayer receives a URL using the source Mac's LAN IP, not 127.0.0.1.
- Button busy states, progress labels, and disabled controls during operations.
- Existing FFmpeg / raw HTTP/1.0 FLAC transport, including blocking accepted
  sockets, remains in place.

## Requirements

- Both Macs on a mutually reachable LAN (not client-isolated guest Wi-Fi).
- On the *remote* HQPlayer Desktop, enable **Allow control from network**.
- Allow incoming TCP 8765 to the RadioKing Mac through its firewall.
- TCP 4321 must be reachable on the selected HQPlayer host.
- `hqp6-control` and FFmpeg remain installed at the paths used by v0.3.2.

## Test

    cd ~/Projects/PeteRadioPlayer
    cargo run --release

Or run from an extracted ZIP directory without touching the Git working tree.

1. Verify Radio 4 on local HQPlayer first.
2. Select another HQPlayer from the drop-down (or type its LAN IP / .local
   hostname and press Connect). Confirm its **Allow control from network** is on.
3. Play Radio 4 on the remote target; check both control and FLAC reachability.
4. Confirm the UI visibly indicates Validating, Starting, On air, and Stopping.
5. Test an unavailable station and a world search. Errors should be visible.
6. Switch targets while playing and confirm the old target stops.

## Caveats

- SSDP is best-effort and can be blocked by some Wi-Fi networks. Manual host
  selection is the fallback.
- This alpha does not yet persist the chosen HQPlayer across application restarts.
- `Playing` is inferred from FLAC bytes successfully delivered, not from
  HQPlayer's audio device reporting audible sound.
- Remote FLAC is served over unencrypted HTTP on the local network; the
  browser control/API remains loopback-only.

## Install macOS app

Quit any running player on TCP 8765. Extract the release ZIP and open `RadioKing.app`. To install from source, run `./install-app.sh`. The source installer preserves an existing `RadioKing.app` as `RadioKing (previous alpha).app`. An existing `RadioKing Live.app` can be kept for rollback.

## Alpha.8 regression checklist

1. Quit the previous RadioKing app, verify port 8765 is free.
2. Run `./install-app.sh`, then `open "$HOME/Applications/RadioKing.app"`.
3. Confirm Dock dot persists without bouncing, browser opens, and Dock click returns to browser.
4. Tune BBC Radio 4 and confirm uninterrupted audio through HQPlayer.
5. Quit via Dock > Quit and verify TCP 8765 is released.
