//! The app's own local proxy, for Settings → Network, where YouTube is blocked or slowed down.
//!
//! It listens on 127.0.0.1 (a port of its own) and everything the app sends goes through it once
//! a connection other than Direct is picked: its HTTP client, its yt-dlp runs (`--proxy`) and
//! mpv (`--http-proxy`, and `proxy=` for mpv's yt-dlp). Two ways out:
//!
//! - **Bypass**: straight to the server, but the first packet of each connection (the TLS
//!   ClientHello, which names the server in plain text) goes out split, so a filter that looks
//!   for YouTube's names in it doesn't see them. This is what ByeDPI, zapret and GoodbyeDPI do;
//!   it works against slowdowns by deep packet inspection (Russia's, for one), not against
//!   blocked addresses.
//! - **Upstream**: through a SOCKS5 or HTTP proxy the user gives (Tor, ByeDPI, a V2Ray/Xray or
//!   VPN client's local port, a server of their own). mpv speaks only HTTP proxies; going through
//!   this one, SOCKS works for it too.
//!
//! Addresses on the local network (cast receivers) always go direct. mpv's video comes through
//! in the clear and this proxy makes the TLS connection for it, over OpenSSL (`relay_url`).

use std::io::{self, Read, Write};
use std::net::{IpAddr, Shutdown, TcpListener, TcpStream, ToSocketAddrs};
use std::sync::{OnceLock, RwLock};
use std::time::Duration;

/// Where connections go.
#[derive(Clone, Debug, PartialEq)]
pub enum Route {
    Direct,
    Bypass(Split),
    Upstream(Upstream),
}

/// How the first packet is split.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Split {
    /// Into two TLS records, sent as two TCP segments.
    Both,
    /// Into two TLS records.
    Tls,
    /// Into two TCP segments.
    Tcp,
}

#[derive(Clone, Debug, PartialEq)]
pub struct Upstream {
    socks: bool,
    host: String,
    port: u16,
    auth: Option<(String, String)>,
}

static ROUTE: RwLock<Route> = RwLock::new(Route::Direct);
static PORT: OnceLock<Option<u16>> = OnceLock::new();
const TIMEOUT: Duration = Duration::from_secs(15);

/// A proxy address: `socks5://[user:pass@]host[:port]` (also `socks5h://`, `socks://`) or
/// `http://[user:pass@]host[:port]`.
pub fn parse_upstream(url: &str) -> Result<Upstream, String> {
    let url = url.trim();
    let (scheme, rest) = url.split_once("://").ok_or("give it as socks5://host:port or http://host:port")?;
    let socks = match scheme.to_ascii_lowercase().as_str() {
        "socks5" | "socks5h" | "socks" => true,
        "http" => false,
        other => return Err(format!("{other}:// isn't supported: use socks5:// or http://")),
    };
    let rest = rest.trim_end_matches('/');
    let (auth, hostport) = match rest.rsplit_once('@') {
        Some((cred, hp)) => {
            let (u, p) = cred.split_once(':').unwrap_or((cred, ""));
            (Some((u.to_string(), p.to_string())), hp)
        }
        None => (None, rest),
    };
    let (host, port) = match hostport.rsplit_once(':') {
        Some((h, p)) if !h.ends_with(']') || h.starts_with('[') => (h, p.parse::<u16>().map_err(|_| format!("bad port: {p}"))?),
        _ => (hostport, if socks { 1080 } else { 8080 }),
    };
    let host = host.trim_start_matches('[').trim_end_matches(']');
    if host.is_empty() {
        return Err("no host".into());
    }
    Ok(Upstream { socks, host: host.to_string(), port, auth })
}

/// Pick the route; the listener starts the first time one other than Direct is picked. The
/// app's HTTP client follows right away; yt-dlp and mpv with their next start (`url`).
pub fn set_route(route: Route) {
    if route != Route::Direct {
        PORT.get_or_init(start);
    }
    // Kept stream links are tied to the address they were looked up from.
    crate::prefetch::network_changed(&format!("{route:?}"));
    *ROUTE.write().unwrap() = route;
    crate::http::rebuild(url().as_deref());
    if let Some(url) = url().filter(|_| crate::yt::timing_on()) {
        eprintln!("unbloatedtube: network: {:?} through {url}", ROUTE.read().unwrap());
    }
}

/// The proxy for yt-dlp, mpv and the HTTP client: None with the Direct route (or if the
/// listener couldn't start).
pub fn url() -> Option<String> {
    if *ROUTE.read().unwrap() == Route::Direct {
        return None;
    }
    PORT.get().copied().flatten().map(|port| format!("http://127.0.0.1:{port}"))
}

/// Settings → Network → Test connection: can the proxy reach `host`, and in how long? The
/// proxy's own answer, which says why when it can't. Blocking.
pub fn check(host: &str) -> Result<Duration, String> {
    let start = std::time::Instant::now();
    let Some(port) = PORT.get().copied().flatten() else { return Err("the app's proxy didn't start".into()) };
    let mut s = TcpStream::connect(("127.0.0.1", port)).map_err(|e| e.to_string())?;
    s.set_read_timeout(Some(Duration::from_secs(12))).map_err(|e| e.to_string())?;
    s.write_all(format!("CONNECT {host}:443 HTTP/1.1\r\nHost: {host}:443\r\n\r\n").as_bytes()).map_err(|e| e.to_string())?;
    let mut answer = [0u8; 512];
    let n = s.read(&mut answer).map_err(|e| if e.kind() == io::ErrorKind::WouldBlock || e.kind() == io::ErrorKind::TimedOut { "no answer within 12 s".to_string() } else { e.to_string() })?;
    let line = String::from_utf8_lossy(&answer[..n]).lines().next().unwrap_or("").to_string();
    match line.split_once(' ').map(|(_, rest)| rest) {
        Some(rest) if rest.starts_with("200") => Ok(start.elapsed()),
        Some(rest) => Err(rest.trim_start_matches("502 ").to_string()),
        None => Err("no answer".into()),
    }
}

fn start() -> Option<u16> {
    let listener = TcpListener::bind("127.0.0.1:0").map_err(|e| eprintln!("unbloatedtube: proxy: {e}")).ok()?;
    let port = listener.local_addr().ok()?.port();
    std::thread::spawn(move || {
        for client in listener.incoming().flatten() {
            std::thread::spawn(move || {
                let _ = serve(client);
            });
        }
    });
    Some(port)
}

/// One client connection: an HTTP proxy request (CONNECT, or a plain http:// request).
fn serve(mut client: TcpStream) -> io::Result<()> {
    client.set_read_timeout(Some(TIMEOUT))?;
    let mut buf = Vec::with_capacity(1024);
    let mut chunk = [0u8; 4096];
    let head_end = loop {
        let n = client.read(&mut chunk)?;
        if n == 0 {
            return Ok(());
        }
        buf.extend_from_slice(&chunk[..n]);
        if let Some(i) = buf.windows(4).position(|w| w == b"\r\n\r\n") {
            break i + 4;
        }
        if buf.len() > 16 * 1024 {
            return Ok(());
        }
    };
    let head = String::from_utf8_lossy(&buf[..head_end]).into_owned();
    let rest = buf[head_end..].to_vec();
    let mut first = head.lines().next().unwrap_or("").split_whitespace();
    let (method, target) = (first.next().unwrap_or(""), first.next().unwrap_or(""));
    let route = ROUTE.read().unwrap().clone();
    if method.eq_ignore_ascii_case("CONNECT") {
        let Some((host, port)) = host_port(target, 443) else { return reply(&mut client, "400 Bad Request") };
        let server = match connect(&route, &host, port) {
            Ok(s) => s,
            Err(e) => {
                // The app's HTTP client keeps only "Proxy failed to connect": say why here.
                eprintln!("unbloatedtube: proxy: can't reach {host}:{port}: {e}");
                return reply(&mut client, &format!("502 {e}"));
            }
        };
        client.write_all(b"HTTP/1.1 200 Connection established\r\n\r\n")?;
        let split = match route {
            Route::Bypass(split) if !is_local(&host) => Some(split),
            _ => None,
        };
        relay(client, server, rest, split)
    } else {
        // A plain http:// request, in absolute form: forward it in origin form.
        let Some(after) = target.strip_prefix("http://") else { return reply(&mut client, "400 Bad Request") };
        let (authority, path) = after.split_once('/').map_or((after, "/".to_string()), |(a, p)| (a, format!("/{p}")));
        let Some((host, port)) = host_port(authority, 80) else { return reply(&mut client, "400 Bad Request") };
        if port == 80 && relayed(&host) {
            return upgrade(client, &route, &host, &head.replacen(target, &path, 1), &rest);
        }
        let mut server = match connect(&route, &host, port) {
            Ok(s) => s,
            Err(e) => return reply(&mut client, &format!("502 {e}")),
        };
        let fixed = head.replacen(target, &path, 1);
        server.write_all(fixed.as_bytes())?;
        relay(client, server, rest, None)
    }
}

/// YouTube's hosts whose links mpv gets as http:// (see `relay_url`).
fn relayed(host: &str) -> bool {
    let host = host.to_ascii_lowercase();
    host.ends_with(".googlevideo.com") || host == "www.youtube.com"
}

/// mpv's version of a stream or caption link while a route other than Direct is on: http://, so
/// it asks this proxy for it in the clear and the proxy makes the TLS connection (`upgrade`).
/// mpv's own TLS (GnuTLS) gets stopped by a filter even with its first packet split, where
/// OpenSSL's gets through (tested on a Russian network); and mpv uses no proxy for https://.
pub fn relay_url(url: &str) -> Option<String> {
    let rest = url.strip_prefix("https://")?;
    let host = rest.split(['/', '?', ':']).next()?;
    // Of www.youtube.com, only captions: its page links (`webpage_url`) stay as they are.
    let media = !host.eq_ignore_ascii_case("www.youtube.com") || rest[host.len()..].starts_with("/api/timedtext");
    (relayed(host) && media).then(|| format!("http://{rest}"))
}

/// A plain request for a relayed host: made over TLS (OpenSSL, the first packet split in Bypass)
/// to port 443, its answer passed back with any redirect to the host kept in the clear.
fn upgrade(mut client: TcpStream, route: &Route, host: &str, head: &str, body: &[u8]) -> io::Result<()> {
    let server = match connect(route, host, 443) {
        Ok(s) => s,
        Err(e) => {
            eprintln!("unbloatedtube: proxy: can't reach {host}:443: {e}");
            return reply(&mut client, &format!("502 {e}"));
        }
    };
    let _ = server.set_nodelay(true);
    let split = match route {
        Route::Bypass(split) => Some(*split),
        _ => None,
    };
    let tls = native_tls::TlsConnector::new().map_err(|e| bad(e.to_string()))?;
    let mut tls = match tls.connect(host, SplitFirst { s: server, split }) {
        Ok(s) => s,
        Err(e) => {
            eprintln!("unbloatedtube: proxy: TLS with {host}: {e}");
            return reply(&mut client, &format!("502 TLS: {e}"));
        }
    };
    // One request per connection: the answer is passed on as it comes, without reading its length.
    let mut req = String::new();
    for line in head.split("\r\n").filter(|l| !l.is_empty()) {
        let name = line.split(':').next().unwrap_or("").trim().to_ascii_lowercase();
        if name != "connection" && name != "proxy-connection" && name != "keep-alive" {
            req.push_str(line);
            req.push_str("\r\n");
        }
    }
    req.push_str("Connection: close\r\n\r\n");
    tls.write_all(req.as_bytes())?;
    tls.write_all(body)?;
    // The answer's head, its Location (a redirect to another video server) in the clear too.
    let mut answer = Vec::new();
    let mut chunk = [0u8; 16 * 1024];
    let head_end = loop {
        let n = tls.read(&mut chunk)?;
        if n == 0 {
            return Ok(());
        }
        answer.extend_from_slice(&chunk[..n]);
        if let Some(i) = answer.windows(4).position(|w| w == b"\r\n\r\n") {
            break i + 4;
        }
        if answer.len() > 64 * 1024 {
            return Ok(());
        }
    };
    let answer_head = String::from_utf8_lossy(&answer[..head_end]).into_owned();
    let mut out = String::new();
    for line in answer_head.split_inclusive("\r\n") {
        match line.split_once(':').filter(|(name, _)| name.eq_ignore_ascii_case("location")) {
            Some((name, value)) => match relay_url(value.trim()) {
                Some(url) => out.push_str(&format!("{name}: {url}\r\n")),
                None => out.push_str(line),
            },
            None => out.push_str(line),
        }
    }
    client.write_all(out.as_bytes())?;
    client.write_all(&answer[head_end..])?;
    io::copy(&mut tls, &mut client)?;
    Ok(())
}

/// A connection whose first write (the TLS ClientHello) goes out split: see `split_hello`.
#[derive(Debug)]
struct SplitFirst {
    s: TcpStream,
    split: Option<Split>,
}

impl Read for SplitFirst {
    fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        self.s.read(buf)
    }
}

impl Write for SplitFirst {
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        match self.split.take() {
            Some(split) => {
                for part in split_hello(buf, split) {
                    self.s.write_all(&part)?;
                }
                Ok(buf.len())
            }
            None => self.s.write(buf),
        }
    }

    fn flush(&mut self) -> io::Result<()> {
        self.s.flush()
    }
}

fn reply(client: &mut TcpStream, status: &str) -> io::Result<()> {
    let status: String = status.chars().filter(|c| !c.is_control()).collect();
    client.write_all(format!("HTTP/1.1 {status}\r\nContent-Length: 0\r\nConnection: close\r\n\r\n").as_bytes())
}

fn host_port(s: &str, default: u16) -> Option<(String, u16)> {
    if let Some(v6) = s.strip_prefix('[') {
        let (h, rest) = v6.split_once(']')?;
        let port = rest.strip_prefix(':').map_or(Some(default), |p| p.parse().ok())?;
        return Some((h.to_string(), port));
    }
    match s.rsplit_once(':') {
        Some((h, p)) => Some((h.to_string(), p.parse().ok()?)),
        None => Some((s.to_string(), default)),
    }
    .filter(|(h, _)| !h.is_empty())
}

/// An address on this machine or the local network: never through an upstream proxy.
fn is_local(host: &str) -> bool {
    if host.eq_ignore_ascii_case("localhost") || host.to_ascii_lowercase().ends_with(".local") {
        return true;
    }
    match host.parse::<IpAddr>() {
        Ok(IpAddr::V4(ip)) => ip.is_loopback() || ip.is_private() || ip.is_link_local(),
        Ok(IpAddr::V6(ip)) => ip.is_loopback() || (ip.segments()[0] & 0xfe00) == 0xfc00 || (ip.segments()[0] & 0xffc0) == 0xfe80,
        Err(_) => false,
    }
}

/// IPv4 addresses first, each given a few seconds: where IPv6 to YouTube goes nowhere (as on
/// some Russian networks) a connection would otherwise sit on the dead IPv6 address first.
fn direct(host: &str, port: u16) -> io::Result<TcpStream> {
    let mut addrs: Vec<_> = (host, port).to_socket_addrs()?.collect();
    addrs.sort_by_key(|a| a.is_ipv6());
    let mut last = io::Error::new(io::ErrorKind::NotFound, format!("can't resolve {host}"));
    for addr in addrs {
        let wait = Duration::from_secs(if addr.is_ipv6() { 3 } else { 6 });
        match TcpStream::connect_timeout(&addr, wait) {
            Ok(s) => return Ok(s),
            Err(e) => last = io::Error::new(e.kind(), format!("{addr}: {e}")),
        }
    }
    Err(last)
}

fn connect(route: &Route, host: &str, port: u16) -> io::Result<TcpStream> {
    let up = match route {
        Route::Upstream(up) if !is_local(host) => up,
        _ => return direct(host, port),
    };
    let mut s = direct(&up.host, up.port).map_err(|e| io::Error::new(e.kind(), format!("proxy {}:{} not reachable ({e})", up.host, up.port)))?;
    s.set_read_timeout(Some(TIMEOUT))?;
    if up.socks { socks5(&mut s, up, host, port)? } else { http_connect(&mut s, up, host, port)? }
    s.set_read_timeout(None)?;
    Ok(s)
}

fn bad(msg: impl Into<String>) -> io::Error {
    io::Error::new(io::ErrorKind::Other, msg.into())
}

/// SOCKS5 CONNECT by name (the proxy resolves it: also what Tor needs).
fn socks5(s: &mut TcpStream, up: &Upstream, host: &str, port: u16) -> io::Result<()> {
    let method = if up.auth.is_some() { 2 } else { 0 };
    s.write_all(&[5, 1, method])?;
    let mut r = [0u8; 2];
    s.read_exact(&mut r)?;
    if r[0] != 5 || r[1] != method {
        return Err(bad("the proxy doesn't speak SOCKS5 (or wants a login)"));
    }
    if let Some((u, p)) = &up.auth {
        let mut m = vec![1, u.len() as u8];
        m.extend_from_slice(u.as_bytes());
        m.push(p.len() as u8);
        m.extend_from_slice(p.as_bytes());
        s.write_all(&m)?;
        s.read_exact(&mut r)?;
        if r[1] != 0 {
            return Err(bad("the proxy refused the login"));
        }
    }
    let mut req = vec![5, 1, 0];
    match host.parse::<IpAddr>() {
        Ok(IpAddr::V4(ip)) => {
            req.push(1);
            req.extend_from_slice(&ip.octets());
        }
        Ok(IpAddr::V6(ip)) => {
            req.push(4);
            req.extend_from_slice(&ip.octets());
        }
        Err(_) => {
            req.push(3);
            req.push(host.len() as u8);
            req.extend_from_slice(host.as_bytes());
        }
    }
    req.extend_from_slice(&port.to_be_bytes());
    s.write_all(&req)?;
    let mut head = [0u8; 4];
    s.read_exact(&mut head)?;
    if head[1] != 0 {
        return Err(bad(format!("the proxy couldn't reach {host} (SOCKS error {})", head[1])));
    }
    let skip = match head[3] {
        1 => 4,
        4 => 16,
        3 => {
            let mut n = [0u8; 1];
            s.read_exact(&mut n)?;
            n[0] as usize
        }
        _ => return Err(bad("odd SOCKS5 reply")),
    };
    let mut rest = vec![0u8; skip + 2];
    s.read_exact(&mut rest)
}

fn http_connect(s: &mut TcpStream, up: &Upstream, host: &str, port: u16) -> io::Result<()> {
    let target = if host.contains(':') { format!("[{host}]:{port}") } else { format!("{host}:{port}") };
    let mut req = format!("CONNECT {target} HTTP/1.1\r\nHost: {target}\r\n");
    if let Some((u, p)) = &up.auth {
        req.push_str(&format!("Proxy-Authorization: Basic {}\r\n", base64(format!("{u}:{p}").as_bytes())));
    }
    req.push_str("\r\n");
    s.write_all(req.as_bytes())?;
    // Read the answer's head byte by byte: what follows it belongs to the tunnel.
    let mut head = Vec::new();
    let mut b = [0u8; 1];
    while !head.ends_with(b"\r\n\r\n") {
        if s.read(&mut b)? == 0 || head.len() > 8192 {
            return Err(bad("the proxy closed the connection"));
        }
        head.push(b[0]);
    }
    let status = String::from_utf8_lossy(&head).lines().next().unwrap_or("").to_string();
    match status.split_whitespace().nth(1) {
        Some("200") => Ok(()),
        _ => Err(bad(format!("the proxy said: {status}"))),
    }
}

fn base64(data: &[u8]) -> String {
    const A: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut out = String::new();
    for c in data.chunks(3) {
        let n = (c[0] as u32) << 16 | (*c.get(1).unwrap_or(&0) as u32) << 8 | *c.get(2).unwrap_or(&0) as u32;
        for i in 0..4 {
            out.push(if i <= c.len() { A[(n >> (18 - 6 * i) & 63) as usize] as char } else { '=' });
        }
    }
    out
}

/// Copy both ways until either side closes. `first`: what the client sent already; `split`: send
/// the client's first TLS record split (Bypass).
fn relay(mut client: TcpStream, mut server: TcpStream, first: Vec<u8>, split: Option<Split>) -> io::Result<()> {
    client.set_read_timeout(None)?;
    let _ = server.set_nodelay(true);
    let mut pending = first;
    if let Some(split) = split {
        // The whole ClientHello: its record header says how long it is.
        client.set_read_timeout(Some(Duration::from_secs(5)))?;
        let mut chunk = [0u8; 16 * 1024];
        while record_missing(&pending) {
            match client.read(&mut chunk) {
                Ok(0) | Err(_) => break,
                Ok(n) => pending.extend_from_slice(&chunk[..n]),
            }
        }
        client.set_read_timeout(None)?;
        for part in split_hello(&pending, split) {
            server.write_all(&part)?;
            server.flush()?;
        }
        pending.clear();
    }
    server.write_all(&pending)?;
    let (mut c2, mut s2) = (client.try_clone()?, server.try_clone()?);
    let back = std::thread::spawn(move || {
        let _ = io::copy(&mut s2, &mut c2);
        let _ = c2.shutdown(Shutdown::Write);
    });
    let _ = io::copy(&mut client, &mut server);
    let _ = server.shutdown(Shutdown::Write);
    let _ = back.join();
    Ok(())
}

/// A TLS handshake record whose bytes haven't all arrived yet.
fn record_missing(data: &[u8]) -> bool {
    match data {
        [] => true,
        [0x16, _, _, hi, lo, ..] => data.len() < 5 + (((*hi as usize) << 8) | *lo as usize),
        [0x16, ..] => true,
        _ => false,
    }
}

/// Where the server name starts in a ClientHello record's payload (after the 5-byte header).
fn sni_offset(payload: &[u8]) -> Option<usize> {
    let get = |i: usize| payload.get(i).copied().map(usize::from);
    let u16_at = |i: usize| Some(get(i)? << 8 | get(i + 1)?);
    if get(0)? != 1 {
        return None;
    }
    // type(1) length(3) version(2) random(32)
    let mut i = 4 + 2 + 32;
    i += 1 + get(i)?; // session id
    i += 2 + u16_at(i)?; // cipher suites
    i += 1 + get(i)?; // compression methods
    let end = (i + 2 + u16_at(i)?).min(payload.len());
    i += 2;
    while i + 4 <= end {
        let (kind, len) = (u16_at(i)?, u16_at(i + 2)?);
        if kind == 0 {
            // list length(2) name type(1) name length(2) name
            let name = i + 4 + 5;
            return (name < payload.len()).then_some(name);
        }
        i += 4 + len;
    }
    None
}

/// The first bytes to send, in the pieces to send them in: a TLS ClientHello cut inside the server
/// name (into two records, two segments, or both); anything else unchanged.
fn split_hello(data: &[u8], split: Split) -> Vec<Vec<u8>> {
    let whole = || vec![data.to_vec()];
    if data.len() < 6 || data[0] != 0x16 {
        return whole();
    }
    let len = ((data[3] as usize) << 8) | data[4] as usize;
    if data.len() < 5 + len {
        return whole();
    }
    let (header, payload, after) = (&data[..5], &data[5..5 + len], &data[5 + len..]);
    // Inside the name, past its first letter; without one, early in the handshake.
    let cut = sni_offset(payload).map_or(1, |o| o + 1).min(len.saturating_sub(1)).max(1);
    let mut parts = match split {
        Split::Tcp => vec![data[..5 + cut].to_vec(), data[5 + cut..5 + len].to_vec()],
        Split::Tls | Split::Both => {
            let record = |body: &[u8]| {
                let mut r = vec![header[0], header[1], header[2], (body.len() >> 8) as u8, body.len() as u8];
                r.extend_from_slice(body);
                r
            };
            let (a, b) = (record(&payload[..cut]), record(&payload[cut..]));
            if split == Split::Tls { vec![[a, b].concat()] } else { vec![a, b] }
        }
    };
    if !after.is_empty() {
        parts.push(after.to_vec());
    }
    parts
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A minimal ClientHello for "www.youtube.com".
    fn hello() -> Vec<u8> {
        let name = b"www.youtube.com";
        let mut sni = vec![0, 0]; // extension type 0
        let list_len = 3 + name.len();
        sni.extend_from_slice(&((list_len + 2) as u16).to_be_bytes());
        sni.extend_from_slice(&(list_len as u16).to_be_bytes());
        sni.push(0);
        sni.extend_from_slice(&(name.len() as u16).to_be_bytes());
        sni.extend_from_slice(name);
        let mut body = vec![3, 3];
        body.extend_from_slice(&[7; 32]);
        body.push(0); // session id
        body.extend_from_slice(&[0, 2, 0x13, 0x01]); // one cipher suite
        body.extend_from_slice(&[1, 0]); // compression
        body.extend_from_slice(&(sni.len() as u16).to_be_bytes());
        body.extend_from_slice(&sni);
        let mut hs = vec![1, 0, (body.len() >> 8) as u8, body.len() as u8];
        hs.extend_from_slice(&body);
        let mut rec = vec![0x16, 3, 1, (hs.len() >> 8) as u8, hs.len() as u8];
        rec.extend_from_slice(&hs);
        rec
    }

    #[test]
    fn the_name_is_found_and_cut() {
        let h = hello();
        let o = sni_offset(&h[5..]).unwrap();
        assert_eq!(&h[5 + o..5 + o + 15], b"www.youtube.com");
        let tcp = split_hello(&h, Split::Tcp);
        assert_eq!(tcp.len(), 2);
        assert_eq!(tcp.concat(), h);
        assert!(tcp[0].ends_with(b"w") && tcp[1].starts_with(b"ww.youtube"));
        let tls = split_hello(&h, Split::Tls);
        assert_eq!(tls.len(), 1);
        assert_eq!(tls[0].len(), h.len() + 5);
        let both = split_hello(&h, Split::Both);
        assert_eq!(both.len(), 2);
        assert_eq!(both.concat(), tls[0]);
        // Two records, each a handshake record with its own length.
        let l1 = ((both[0][3] as usize) << 8) | both[0][4] as usize;
        assert_eq!(both[0].len(), 5 + l1);
        assert_eq!(&both[1][..3], &[0x16, 3, 1]);
        // Not TLS: untouched.
        assert_eq!(split_hello(b"GET / HTTP/1.1\r\n\r\n", Split::Both), vec![b"GET / HTTP/1.1\r\n\r\n".to_vec()]);
    }

    #[test]
    fn stream_and_caption_links_go_in_the_clear() {
        let v = "https://rr2---sn-gvnuxaxjvh-jx3z.googlevideo.com/videoplayback?expire=1&ip=1.2.3.4";
        assert_eq!(relay_url(v).as_deref(), Some(&v.replacen("https", "http", 1)[..]));
        assert!(relay_url("https://www.youtube.com/api/timedtext?v=x&lang=en").is_some());
        assert_eq!(relay_url("https://www.youtube.com/watch?v=x"), None);
        assert_eq!(relay_url("https://i.ytimg.com/vi/x/hq720.jpg"), None);
        assert_eq!(relay_url("http://rr2---sn-x.googlevideo.com/videoplayback"), None);
        assert_eq!(relay_url("https://googlevideo.com.example/x"), None);
    }

    #[test]
    fn proxy_addresses() {
        assert_eq!(parse_upstream("socks5://127.0.0.1:9050").unwrap(), Upstream { socks: true, host: "127.0.0.1".into(), port: 9050, auth: None });
        assert_eq!(parse_upstream("http://u:p@proxy.example:3128/").unwrap(), Upstream { socks: false, host: "proxy.example".into(), port: 3128, auth: Some(("u".into(), "p".into())) });
        assert_eq!(parse_upstream("socks5h://[::1]:1080").unwrap().host, "::1");
        assert_eq!(parse_upstream("socks5://host").unwrap().port, 1080);
        assert!(parse_upstream("ftp://x:1").is_err());
        assert!(parse_upstream("127.0.0.1:9050").is_err());
        assert!(is_local("192.168.1.20") && is_local("tv.local") && is_local("::1") && !is_local("www.youtube.com") && !is_local("8.8.8.8"));
        assert_eq!(host_port("[::1]:443", 1), Some(("::1".into(), 443)));
        assert_eq!(base64(b"u:p"), "dTpw");
    }
}
