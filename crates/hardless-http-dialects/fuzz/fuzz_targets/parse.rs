//! FR-31: both XML dialects must never panic on any document.
#![no_main]

use hardless_http_dialects::{epos, webprnt};
use libfuzzer_sys::fuzz_target;

fuzz_target!(|data: &[u8]| {
    if let Ok(text) = std::str::from_utf8(data) {
        let _ = epos::parse(text);
        let _ = webprnt::parse(text);
    }
});
