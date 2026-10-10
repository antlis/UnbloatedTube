//! One HTTP client for the whole app, so connections are kept open and reused: each new
//! connection costs a TCP and TLS handshake (100–300 ms), which a list of thumbnails or a run of
//! account requests would otherwise pay every time (`ureq::get` builds a fresh client per call).

use std::sync::LazyLock;
use std::time::Duration;

static AGENT: LazyLock<ureq::Agent> =
    LazyLock::new(|| ureq::AgentBuilder::new().timeout_connect(Duration::from_secs(10)).timeout(Duration::from_secs(60)).max_idle_connections_per_host(8).build());

pub fn agent() -> &'static ureq::Agent {
    &AGENT
}
