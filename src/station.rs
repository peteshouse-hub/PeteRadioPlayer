#[derive(Clone, Debug)]
pub struct Station { pub name: String, pub source_url: String, pub group: String }

pub fn bbc() -> Vec<Station> {
    // Explicit worldwide endpoints from the maintained BBCRadioStreams list.
    // Radio 4 is confirmed by listening; others must pass local FFmpeg preflight.
    let feeds = [
        ("BBC Radio 4", "http://as-hls-ww-live.akamaized.net/pool_55057080/live/ww/bbc_radio_fourfm/bbc_radio_fourfm.isml/bbc_radio_fourfm-audio%3d96000.norewind.m3u8"),
        ("BBC Radio 1", "http://as-hls-ww-live.akamaized.net/pool_01505109/live/ww/bbc_radio_one/bbc_radio_one.isml/bbc_radio_one-audio%3d96000.norewind.m3u8"),
        ("BBC Radio 2", "http://as-hls-ww-live.akamaized.net/pool_74208725/live/ww/bbc_radio_two/bbc_radio_two.isml/bbc_radio_two-audio%3d96000.norewind.m3u8"),
        ("BBC Radio 3", "http://as-hls-ww-live.akamaized.net/pool_23461179/live/ww/bbc_radio_three/bbc_radio_three.isml/bbc_radio_three-audio%3d96000.norewind.m3u8"),
        ("BBC Radio 4 Extra", "http://as-hls-ww-live.akamaized.net/pool_26173715/live/ww/bbc_radio_four_extra/bbc_radio_four_extra.isml/bbc_radio_four_extra-audio%3d96000.norewind.m3u8"),
        ("BBC Radio 5 Live", "http://as-hls-ww-live.akamaized.net/pool_89021708/live/ww/bbc_radio_five_live/bbc_radio_five_live.isml/bbc_radio_five_live-audio%3d96000.norewind.m3u8"),
        ("BBC Radio 6 Music", "http://as-hls-ww-live.akamaized.net/pool_81827798/live/ww/bbc_6music/bbc_6music.isml/bbc_6music-audio%3d96000.norewind.m3u8"),
        ("BBC Radio 1Xtra", "http://as-hls-ww-live.akamaized.net/pool_92079267/live/ww/bbc_1xtra/bbc_1xtra.isml/bbc_1xtra-audio%3d96000.norewind.m3u8"),
        ("BBC Asian Network", "http://as-hls-ww-live.akamaized.net/pool_22108647/live/ww/bbc_asian_network/bbc_asian_network.isml/bbc_asian_network-audio%3d96000.norewind.m3u8"),
        ("BBC Radio Wales", "http://as-hls-ww-live.akamaized.net/pool_904/live/ww/bbc_radio_wales_fm/bbc_radio_wales_fm.isml/bbc_radio_wales_fm-audio%3d96000.norewind.m3u8"),
        ("BBC Radio Ulster", "http://as-hls-ww-live.akamaized.net/pool_904/live/ww/bbc_radio_ulster/bbc_radio_ulster.isml/bbc_radio_ulster-audio%3d96000.norewind.m3u8"),
        ("BBC World Service", "http://a.files.bbci.co.uk/media/live/manifesto/audio/simulcast/hls/nonuk/sbr_low/ak/bbc_world_service.m3u8"),
    ];
    feeds.into_iter().map(|(name,url)| Station {
        name: name.into(), source_url: url.into(), group: "BBC".into()
    }).collect()
}
