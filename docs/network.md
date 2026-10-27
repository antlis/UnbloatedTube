# Where YouTube is blocked or slowed down

What the app does when your network blocks or throttles YouTube, and how to troubleshoot the connection.

Settings → **Network** (search the Settings page for "proxy", "bypass" or "russia") decides how the
app reaches YouTube. One choice, **Connection**, with three options:

| Your situation | Pick |
| --- | --- |
| YouTube works normally | **Direct** (the default) |
| YouTube is *slowed down* or videos hang, but the site itself opens (Russia since 2024, some providers elsewhere) | **Bypass slowdown** → *TLS split* |
| YouTube's addresses are *blocked* outright, or Bypass doesn't help | **Proxy**, with Tor, ByeDPI, a VPN client or a server of your own |

Changes apply at once: lists, thumbnails and account actions use the new route right away;
yt-dlp and the player with the next video. Press **Test connection** to check (see below).

## Direct

The app connects to YouTube itself, as any program would. Nothing goes through the app's proxy.

## Bypass slowdown

Nothing to install, no other server involved, no speed limit: the connection still goes
straight to YouTube.

**How it works.** Filters like Russia's (TSPU) recognise YouTube by the first packet of each
encrypted connection, the TLS *ClientHello*, which names the server in plain text
(`www.youtube.com`, `rr3---sn-….googlevideo.com`). Once recognised, the connection is slowed to a
crawl or stalled. The app sends that first packet cut in two, right inside the server name, so a
filter that looks at one piece doesn't see the name. The server puts the pieces back together;
nothing about the connection changes for it. This is what ByeDPI, zapret and GoodbyeDPI do, built
in.

**Methods**, as providers' filters differ:

| Method | What it does | The same in ByeDPI |
| --- | --- | --- |
| **TLS split** (recommended) | The ClientHello as two TLS records, in one packet | `--tlsrec 1+s` |
| **TLS + TCP split** | Two TLS records, sent as two separate packets | `--tlsrec 1+s --split 1+s` |
| **TCP split** | One TLS record, sent as two packets | `--split 1+s` |

*TLS split* is the one that got videos playing on a Russian home provider in testing. If videos
stay slow, try the other two.

**OpenSSL.** On that network the filter also stopped connections whose first packet *was* split,
when they came from GnuTLS (which mpv and FFmpeg use) or rustls, and let OpenSSL's through; the
two libraries' handshakes look different on the wire. So with Bypass (and Proxy), every
connection to YouTube is made with OpenSSL:

- the app's own requests (lists, thumbnails, account actions, comments) use OpenSSL;
- yt-dlp uses Python's, which is OpenSSL;
- mpv gets the video's links as `http://…` pointing at the app's proxy, which makes the encrypted
  connection to YouTube for it, with OpenSSL. The unencrypted part never leaves your computer (it
  is between mpv and `127.0.0.1`), and OpenSSL checks YouTube's certificate as usual.

**What it can't do:**

- *Blocked addresses.* If the provider drops all traffic to YouTube's servers, no splitting helps:
  use **Proxy**.
- *A lying DNS.* If the provider's DNS answers YouTube's names with wrong addresses, set another
  DNS for your system (1.1.1.1, 8.8.8.8, or DNS over HTTPS in your network settings).
- *IPv6.* On some networks IPv6 to YouTube goes nowhere; the app tries IPv4 addresses first (6 s
  each) and IPv6 only after them (3 s), so a dead IPv6 doesn't hold up every connection.

## Proxy

Everything goes through a proxy you already have running. The app doesn't start or install it.

- **Presets**: **Tor** (`socks5://127.0.0.1:9050`) and **ByeDPI** (`socks5://127.0.0.1:1080`),
  the programs' default ports.
- **Proxy address**: anything else, typed in:

  | Format | Example |
  | --- | --- |
  | SOCKS5 | `socks5://127.0.0.1:1080` (also `socks5h://` and `socks://`; names are always resolved by the proxy) |
  | HTTP (CONNECT) | `http://192.168.1.10:3128` |
  | With a login | `socks5://user:password@host:1080`, `http://user:password@host:8080` |
  | IPv6 | `socks5://[::1]:1080` |
  | No port | SOCKS5 means 1080, HTTP 8080 |

What to point it at:

- **Tor**: the `tor` service listens on `socks5://127.0.0.1:9050`; Tor Browser, while it is open, on
  `socks5://127.0.0.1:9150`. Gets through nearly anything, but slowly, and YouTube often answers Tor
  with "Sign in to confirm you're not a bot" (being logged in in the app helps).
- **ByeDPI**: `ciadpi --tlsrec 1+s` (or any other strategy it offers) listens on
  `socks5://127.0.0.1:1080`. Useful when none of the built-in methods works: ByeDPI has many more.
  zapret and GoodbyeDPI work without a proxy address: leave the app on Direct.
- **A VPN or proxy client** (V2Ray, Xray, sing-box, Hiddify, NekoRay, Shadowsocks, Clash…): its
  local SOCKS or HTTP port, often `socks5://127.0.0.1:1080`, `:10808` or `:7890`. Only the app goes
  through it, not the whole system.
- **A server of your own**: an SSH tunnel (`ssh -D 1080 user@server` gives
  `socks5://127.0.0.1:1080`), Squid, Dante, 3proxy.

mpv itself only speaks HTTP proxies; since it goes through the app's proxy, SOCKS ones work for
it too.

## Test connection

The button (shown with Bypass or Proxy) asks YouTube once, the way the app does, within 12
seconds:

- **YouTube answers: 240 ms**: the route works.
- **Doesn't work: …** says why, for example:
  - `no answer within 12 s`: the filter swallows the connection; try another method, or Proxy;
  - `proxy 127.0.0.1:9050 not reachable (… Connection refused …)`: the proxy program isn't
    running, or listens on another port;
  - `the proxy doesn't speak SOCKS5 (or wants a login)`: wrong scheme (`http://` vs `socks5://`)
    or missing `user:password@`;
  - `the proxy refused the login`;
  - `the proxy couldn't reach www.youtube.com (SOCKS error 4)`: the proxy itself can't get to
    YouTube;
  - `the proxy said: HTTP/1.1 407 …`: what an HTTP proxy answered.

It checks the route to `www.youtube.com`. Videos come from other servers (`*.googlevideo.com`),
which some filters treat differently: if the test passes but videos still hang, see
[Troubleshooting](#troubleshooting-the-connection).

## What goes through it

| Traffic | Route |
| --- | --- |
| Lists, search, thumbnails, avatars, account actions, comments, upload notifications, DeArrow | The app's proxy, OpenSSL |
| yt-dlp (finding a video's streams, captions, downloads) | The app's proxy (`--proxy`) |
| The video and its captions in mpv, also in a separate window | The app's proxy, which makes the TLS connection |
| Cast receivers and anything else on your own network | Always direct |
| SponsorBlock segments (looked up by mpv's script, not from YouTube) | Direct |
| **Open in browser** | Your browser, with its own settings |

The app's proxy listens on `127.0.0.1` only, on a port of its own (it starts the first time Bypass
or Proxy is picked); nothing else on the network can use it.

Switching the connection also throws away the stream links the app kept ahead of time: YouTube
ties each link to the address it was asked from, and refuses it from another (links found with a
VPN on stop working once it is off).

In `~/.config/unbloatedtube/settings.json` the choice is kept as `"connection"` (`"direct"`,
`"bypass"` or `"proxy"`), `"bypass_method"` (`"tls"`, `"both"` or `"tcp"`) and `"proxy"` (the
address).

## Troubleshooting the connection

Start the app from a terminal with the timing log on to see what the network does:

```sh
UNBLOATEDTUBE_TIMING=1 unbloatedtube
```

- `unbloatedtube: network: Bypass(Tls) through http://127.0.0.1:41234`: the route in use and the
  address of the app's proxy (the port changes on every start).
- `unbloatedtube: proxy: can't reach rr3---sn-….googlevideo.com:443: …`: a connection that failed,
  and why (`timed out`, `Connection refused`, the upstream proxy's answer).
- `unbloatedtube: proxy: TLS with …: …`: the connection opened but the encrypted handshake failed
  (often: the filter reset it).

Then, in order:

1. Bypass: try each **method**.
2. Run ByeDPI with other strategies and use it through **Proxy** (`--disorder 1`, `--split 1+s`,
   `--tlsrec 1+s --split 1+s`…): `curl -x socks5h://127.0.0.1:1080 -o /dev/null -w '%{http_code}\n'
   https://www.youtube.com/` printing `200` means that strategy gets through.
3. Tor or a VPN client through **Proxy**.

Live streams are the one known gap with Bypass and Proxy: their video comes in pieces listed in a
playlist file whose links mpv fetches itself, so they may not play where only OpenSSL gets
through.
