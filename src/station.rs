#[derive(Clone, Debug)]
pub struct Station { pub name: String, pub source_url: String, pub group: String, pub rewind_url: Option<String> }

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
        ("BBC Radio Wales", "http://as-hls-ww-live.akamaized.net/pool_97517794/live/ww/bbc_radio_wales_fm/bbc_radio_wales_fm.isml/bbc_radio_wales_fm-audio%3d96000.norewind.m3u8"),
        ("BBC Radio Ulster", "http://as-hls-ww-live.akamaized.net/pool_31244774/live/ww/bbc_radio_ulster/bbc_radio_ulster.isml/bbc_radio_ulster-audio%3d96000.norewind.m3u8"),
        ("BBC World Service", "https://as-hls-ww-live.akamaized.net/pool_87948813/live/ww/bbc_world_service/bbc_world_service.isml/bbc_world_service-audio%3d96000.norewind.m3u8"),
    ];
    feeds.into_iter().map(|(name,url)| Station {
        name: name.into(),
        source_url: url.into(),
        group: "BBC".into(),
        rewind_url: Some(url.replacen("http:", "https:", 1).replace(".norewind.m3u8", ".m3u8")),
    }).collect()
}

pub fn directory_rewind_url(source: &str) -> Option<String> {
    let mut url = url::Url::parse(source).ok()?;
    if !matches!(url.scheme(), "http" | "https") || !url.username().is_empty()
        || url.password().is_some() || url.port().is_some()
    {
        return None;
    }
    let path = url.path();
    match url.host_str()? {
        // The directory lists a BBC master playlist for Tees. Use the verified
        // worldwide media playlist directly for rewind.
        "a.files.bbci.co.uk" if path.contains("/audio/simulcast/hls/")
            && path.ends_with("/bbc_tees.m3u8") =>
        {
            Some("https://as-hls-ww-live.akamaized.net/pool_08918172/live/ww/bbc_tees/bbc_tees.isml/bbc_tees-audio%3d96000.m3u8".into())
        }
        "as-hls-ww-live.akamaized.net" | "as-hls-uk-live.akamaized.net" => {
            let (_, stream) = path.split_once("/live/")?;
            let mut parts = stream.split('/');
            if !matches!(parts.next()?, "ww" | "uk") { return None; }
            let service = parts.next()?;
            let file = parts.next()?;
            if !service.starts_with("bbc_")
                || file != format!("{service}.isml")
                || !parts.next()?.starts_with(&format!("{service}-audio"))
                || parts.next().is_some()
                || !path.ends_with(".m3u8")
            {
                return None;
            }
            let rewind = path.replace(".norewind.m3u8", ".m3u8");
            url.set_path(&rewind);
            url.set_scheme("https").ok()?;
            url.set_fragment(None);
            Some(url.into())
        }
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn detects_bbc_streams_by_url() {
        let tees = "https://a.files.bbci.co.uk/ms6/live/id/audio/simulcast/hls/nonuk/aks/bbc_tees.m3u8";
        let rewind = directory_rewind_url(tees).unwrap();
        assert!(rewind.contains("pool_08918172"));
        assert_eq!(directory_rewind_url(&rewind).as_deref(), Some(rewind.as_str()));
        let live = rewind.replace(".m3u8", ".norewind.m3u8");
        assert_eq!(directory_rewind_url(&live).as_deref(), Some(rewind.as_str()));
        assert!(directory_rewind_url("https://example.org/bbc_tees.m3u8").is_none());
    }
}
