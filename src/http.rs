//! One HTTP client for the whole app, so connections are kept open and reused: each new
//! connection costs a TCP and TLS handshake (100–300 ms), which a list of thumbnails or a run of
//! account requests would otherwise pay every time (`ureq::get` builds a fresh client per call).
//! Rebuilt when Settings → Network changes, to go through the app's proxy (see proxy.rs) or not.

use std::sync::{LazyLock, RwLock};
use std::time::Duration;

static AGENT: LazyLock<RwLock<ureq::Agent>> = LazyLock::new(|| RwLock::new(build(None)));

fn build(proxy: Option<&str>) -> ureq::Agent {
    let mut b = ureq::AgentBuilder::new().timeout_connect(Duration::from_secs(10)).timeout(Duration::from_secs(60)).max_idle_connections_per_host(8);
    if let Some(p) = proxy.and_then(|p| ureq::Proxy::new(p).ok()) {
        b = b.proxy(p);
    }
    b.build()
}

/// The client (cheap to clone: it shares the connection pool).
pub fn agent() -> ureq::Agent {
    AGENT.read().unwrap().clone()
}

/// Go through `proxy` from now on (None: directly).
pub fn rebuild(proxy: Option<&str>) {
    *AGENT.write().unwrap() = build(proxy);
}
