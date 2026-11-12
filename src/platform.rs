pub const HQPLAYER: bool = cfg!(target_os = "macos");

pub const FFMPEG: &str = if HQPLAYER {
    "/opt/homebrew/bin/ffmpeg"
} else {
    "ffmpeg"
};
pub const FFPLAY: &str = if HQPLAYER {
    "/opt/homebrew/bin/ffplay"
} else {
    "ffplay"
};
pub const CURL: &str = if HQPLAYER { "/usr/bin/curl" } else { "curl" };
pub const BROWSER: &str = if HQPLAYER { "open" } else { "xdg-open" };
pub const BIND_ADDRESS: &str = if HQPLAYER {
    "0.0.0.0:8765"
} else {
    "127.0.0.1:8765"
};
pub const SYSTEM_OUTPUT: &str = if HQPLAYER {
    "Default Mac Output"
} else {
    "Default System Output"
};
