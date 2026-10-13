# RadioKing Live 0.4.0-alpha.4

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
- Allow incoming TCP 8765 to the PeteRadioPlayer Mac through its firewall.
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
- This build has not been compiled or auditioned on the target Macs here.

## Install macOS app

Quit any running player (TCP 8765), then run `./install-app.sh` from this extracted directory. The installer preserves an existing `RadioKing Live.app` as `RadioKing Live (previous alpha).app` when possible. It does not modify `PeteRadioPlayer.app` or the published v0.3.2 release.

## Alpha.4 regression checklist

1. Quit the previous RadioKing app, verify port 8765 is free.
2. Run `./install-app.sh`, then `open "$HOME/Applications/RadioKing Live.app"`.
3. Confirm Dock dot persists without bouncing, browser opens, and Dock click returns to browser.
4. Tune BBC Radio 4 and confirm uninterrupted audio through HQPlayer.
5. Quit via Dock > Quit and verify TCP 8765 is released.

Note: Native launcher is a new component and has not been compiled/tested on macOS in the packaging environment. Do not delete alpha.3 backup before validating.
