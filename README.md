# unbloated-youtube

A minimal YouTube desktop client in Rust + [GPUI](https://crates.io/crates/gpui).

- **Subscriptions**: your channels A–Z with avatars; channels with new uploads come first with a
  count, plus a **New uploads** feed across all of them.
- **Playlists** (incl. Watch later / Liked), **History**, **Search**.
- **Player** embedded in the window (mpv), opening on the last video you watched at the
  position you left; Prev/Next through the list you picked from, autoplay, fullscreen.
- **Subscribe / Save to playlist / Like / Share** buttons for the logged-in account.
- **Settings** to hide tabs and buttons, and player options: max quality, hardware decoding,
  codec preference, speed, SponsorBlock, audio only, subtitles, extra mpv options.
- Draggable column and player/recommendations splits.

Data comes from `yt-dlp`; playback is a single reused `mpv`, embedded via X11 (`--wid`).

## Run (NixOS)

```sh
nix-shell                      # once per terminal (pins a recent yt-dlp, SponsorBlock script)
cargo run                      # or ./target/debug/unbloated-youtube
```

## Login

Subscriptions, playlists, history, recommendations and the account buttons need cookies.
Create `~/.config/unbloated-youtube/config.toml`:

```toml
cookies_from_browser = "brave+gnomekeyring"   # yt-dlp syntax, e.g. "firefox"
# cookies_file = "/path/to/cookies.txt"       # alternative: exported Netscape cookies
```

## Files

- `~/.config/unbloated-youtube/` — `config.toml` (login), `settings.json` (in-app settings)
- `~/.local/share/unbloated-youtube/` — watch history with resume positions, seen videos
- `~/.cache/unbloated-youtube/` — thumbnails, cached lists, `mpv.log`

Folders from the app's old name (`jtube`) are moved over automatically on first start.
