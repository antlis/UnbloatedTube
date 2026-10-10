# Changelog

All notable changes to unbloated-youtube. Newest first.
Versions follow [Semantic Versioning](https://semver.org): while below 1.0, a minor bump (0.2.0)
may add features or change behaviour, a patch bump (0.1.1) only fixes bugs.

## Unreleased

### Changed
- **Faster channels.** A channel's Videos, Shorts and Live tabs and the list of channels you're
  subscribed to come straight from YouTube's API, a page at a time, instead of through yt-dlp
  (seconds per channel before); yt-dlp remains the fallback when that fails or finds nothing.

## 0.36.0 - 2026-10-10

### Fixed
- Opening Settings closed the app (since 0.31.0): the "Hide videos with words" field had no
  input focus of its own.

### Added
- **DeArrow**: titles and thumbnails the community wrote to replace clickbait ones, from the
  SponsorBlock authors' DeArrow project. Off by default; Settings → DeArrow turns titles and
  thumbnails on separately. A replaced title shows YouTube's own on hover in lists, and under
  it on the player. Videos are asked for by a hash prefix of their id, so DeArrow doesn't learn
  which one; without a submission (or when DeArrow can't be reached) YouTube's stay.

## 0.35.1 - 2026-10-10

### Fixed
- Lists and account actions start seconds sooner when logged in through the browser: reading
  its cookies no longer makes a request to YouTube first, and lists asked for at the same
  time (feed and recommendations at start) share one read instead of each doing their own.
- The browser's cookies are kept between starts (up to 12 hours, in the private runtime folder,
  readable only by you), so lists don't wait for the browser at every start. They are read again
  in the background right away (a running browser renews them, and YouTube soon stops taking the
  old ones), and a list they were refused for waits for that read. Logging out or a login change deletes them.
- Videos start about two seconds sooner (and are looked up ahead sooner): yt-dlp gets the kept
  cookies instead of reading the browser's on every lookup. If YouTube refuses them, the lookup
  runs again with the browser's own.
- **Videos start seconds sooner with SponsorBlock on**: its setting replaced the option that
  makes mpv use the app as its yt-dlp, so mpv ran yt-dlp itself for every video and never used
  what was looked up ahead of time (pointer resting on a row, Next, the list's first rows).
- Video lookups skip work yt-dlp repeated every time: YouTube's player script is kept on disk
  instead of downloaded again for each video, and so is deno's preprocessing of it, which the
  challenge solving otherwise redoes from scratch (with yt-dlp installed as a Python program, as
  on Nix, pip and most distributions). Old player versions are cleaned up after two weeks.
- Playing a video that is still being looked up ahead of time waits for that lookup instead of
  starting a second one that takes as long again.
- A video whose lookup ahead of time fails (upcoming, members only, removed…) is no longer looked
  up again every few seconds while it stays on screen.
- The timing log also shows when a list's first videos appeared, how long reading the
  browser's cookies took, and each video lookup (from a kept answer, the quick or the full one).

## 0.35.0 - 2026-10-10

### Changed
- **Faster lists.** The subscriptions feed, playlists (Watch later and Liked included),
  recommendations and search come straight from YouTube's API, a page at a time, instead of
  through yt-dlp; yt-dlp remains the fallback when that fails or finds nothing.
- HTTP requests (account actions, thumbnails, dislike counts, cast receivers) share one
  connection pool, so they skip a new connection each; at most 8 thumbnails download at once,
  so the visible ones come first.
- Background work that waits (yt-dlp, files, network) runs on its own thread pool and no
  longer slows the player's polling or other loads.
- `UNBLOATEDTUBE_TIMING=1` logs how long each list and video start took.

## 0.34.0 - 2026-10-10

### Changed
- **Renamed to UnbloatedTube** (the repository already was): the command is `unbloatedtube`, with
  `ubt` as a short name and `unbloated-youtube` kept as an alias in the AUR and Nix packages.
  Settings, history, downloads and caches move from `~/.config/unbloated-youtube` (and the data
  and cache folders) to `…/unbloatedtube` on first start. The AUR package is now
  `unbloatedtube-bin` (`unbloated-youtube-bin` becomes a transitional package that installs it),
  the Nix flake output `unbloatedtube` (`unbloated-youtube` stays as an alias), the release
  archives `unbloatedtube-X.Y.Z-…`, the media-player (MPRIS) name `unbloatedtube`.

## 0.33.0 - 2026-10-10

### Added
- **Subtitle language picker**: right-click the video → Subtitles lists the video's caption
  tracks, its auto-generated one and auto-translations into your subtitle languages. A pick
  shows them right away, for that video only; the Transcript tab follows it.

## 0.32.0 - 2026-10-10

### Added
- **Sleep timer**: right-click the video → Sleep: end of this video, or 15 / 30 / 45 / 60 / 90
  minutes. The video pauses when the time is up (also while casting); "End of this video" lets
  it finish without autoplay or Up next going on. A line in the corner counts down.

## 0.31.0 - 2026-10-10

### Added
- **Likes and dislikes** of the playing video under its title, from the Return YouTube Dislike
  API (dislikes are its estimate), also logged out. Settings → Likes and dislikes turns it off.

## 0.30.0 - 2026-10-10

### Added
- **Quality picker** for the playing video: right-click it → Quality shows what plays (e.g.
  "1080p") and lets you pick Auto, 2160p … 144p or Audio only. The video reloads at once from
  the same spot, paused or playing as it was; the pick is for that video only.

## 0.29.0 - 2026-10-10

### Added
- **Hide videos by keyword**: Settings → Hide videos with words (comma-separated, e.g.
  `reaction, prank, #shorts`). Videos whose title says one, as a whole word, disappear from
  feeds, channels, recommendations, Live and search, and send no upload notification; your own
  lists (History, Up next, Watch later, Downloads, playlists) keep them.

## 0.28.0 - 2026-10-10

### Added
- **Transcript tab**: the video's captions as a list of timed lines, the current one highlighted.
  Its search field finds every moment a word is said (marked, with a count); a click jumps
  there, also while casting. It shares the subtitles' captions and cache; Settings → Transcript
  hides it.

## 0.27.1 - 2026-10-10

### Fixed
- **Rebuilding while the app is open** no longer breaks it: subtitles, comments, the description,
  marking watched and pasted links failed with "cannot run yt-dlp: No such file or directory"
  until a restart, because the app runs itself as yt-dlp and the file was replaced. It now runs
  the copy that is running (`/proc/<pid>/exe`), and so does mpv.

## 0.27.0 - 2026-10-10

### Added
- **Media keys (MPRIS)**: the app shows up as a media player on D-Bus, so media keys, the
  desktop's player widget, headphone buttons, KDE Connect and `playerctl` show the playing video
  (title, channel, thumbnail, position) and control it, without any setup; while casting they
  drive the receiver.

## 0.26.0 - 2026-10-10

### Added
- **Clickable timestamps, links, #tags and @handles** in the Description tab and in comments:
  `12:34` (or `1:02:03`) jumps the video there, also while casting, and starts it if it is paused
  or not loaded; a `#tag` runs a search for it; an `@handle` opens that channel; YouTube links
  (videos, channels, playlists) open in the app, other links in the browser.

## 0.25.0 - 2026-10-10

### Added
- **Downloads tab**: the videos you saved with the Download button, newest first, with a filter.
  They play from the file, so also offline and logged out; right-click → "Delete downloaded file"
  deletes one. It appears once something is downloaded; Settings → Downloads tab hides it.
  Videos downloaded before this version aren't listed.

## 0.24.0 - 2026-10-10

### Added
- **Description tab** under the player, next to Recommended: the video's description, fetched
  (with yt-dlp, so no login needed) when you open the tab. Settings → Description turns it off.

## 0.23.1 - 2026-10-10

### Changed
- The README marks the project as **alpha**: a status badge and a short note at the top.

### Fixed
- **Switching videos while casting**: picking another video in the cast view played it here and
  dropped the cast controls (and an age-restricted video asked for cookies). It now goes to the
  receiver, which plays it with its own login, and the cast view stays.
  While the receiver loads it, the view no longer flips back to the old video or closes as
  "Cast ended".

## 0.23.0 - 2026-10-09

### Added
- **Live indicator**: live streams get a red LIVE badge on their thumbnail in every list and a LIVE
  chip beside the playing video, and show "N watching" instead of a view count.
- **Live tab**: Subscriptions has **Channels | Live (N)** at the top; Live lists the streams that
  are on now from all your subscriptions (the group chips narrow it). Channels also get a **Live**
  tab (their streams) next to Videos and Shorts.
- **Back / forward buttons** first in the row under the progress bar (and Alt+← / Alt+→): go to
  the video you played before, and return, like a browser's. The player's own previous / next
  still follow the list. They can be turned off in Settings → Player buttons ("Back / forward").

## 0.22.0 - 2026-10-08

### Changed
- **License**: new releases are under the GNU AGPLv3 (`AGPL-3.0-only`) instead of MIT, in
  `LICENSE`, `Cargo.toml`, the Nix package and the AUR package. Versions up to 0.21.3 stay MIT.

## 0.21.3 - 2026-10-08

### Fixed
- Playlists failing to load in the right-click menu ("No playlists"), and adding to a playlist
  failing, after the app had been open for a while: the login is kept for the whole run and can
  stop being accepted, so a failed account request is now tried once more with the cookies as the
  browser has them at that moment.

## 0.21.2 - 2026-10-07

### Fixed
- Removing a video from a playlist removes that one entry. It used to remove every copy of the
  video, so deleting one of three duplicates deleted all three.
- When YouTube refuses to add or remove a video, the notice says what YouTube answered (its
  status and message) instead of always blaming playlists that aren't yours.

## 0.21.1 - 2026-10-07

### Fixed
- The Save button's playlist list no longer offers playlists you only saved from other people,
  which YouTube refuses to add to ("You did not add it"). It shows what YouTube's own Save menu
  does: your playlists, and Watch later.

## 0.21.0 - 2026-10-07

### Changed
- **Videos start sooner, again**: besides the row under the pointer, the app now looks up the
  first rows of the list on screen, the head of Up next and the next video, and reacts to the
  pointer after 0.15 s instead of 0.35 s.
- **yt-dlp stays running**: when it is a Python script (Nix, pip, pipx, distro packages), one
  helper process keeps it loaded and serves lists, search, comments, subtitles and mpv, saving
  about a second per request. Without that (the standalone binary, the AppImage) nothing changes.
- **A faster lookup for ordinary videos** (the manifests it doesn't need are skipped). Live
  streams, premieres and anything unusual always get the full lookup, and are never kept.

## 0.20.0 - 2026-10-07

### Changed
- **Videos start sooner**: the app looks a video's streams up ahead of time, for the row the
  pointer rests on (or the Vim cursor) and for the video Next would play, and mpv uses that answer
  instead of waiting for yt-dlp (about 3 s saved per video). Settings → Player → "Load videos
  ahead" turns it off.

## 0.19.1 - 2026-10-07

### Changed
- Vim mode: click hints moved from `f` to `Shift+F`, so `f` is fullscreen in every mode (Vim mode used `Shift+F` for it before). The in-app key list and the README say so.

### Fixed
- Starting with a link (`unbloated-youtube <link>`) no longer loads the last-watched video first.
- The group buttons in a channel's header show their channel counts, like the group bar of the list.

## 0.19.0 - 2026-10-07

### Added
- **Command line**: `unbloated-youtube <link>` plays a link, in the running app if there is one
  (a small socket in `$XDG_RUNTIME_DIR`, no second window) or in a new one. Commands control the
  running app: `open`, `queue`, `pause`, `play`, `toggle`, `next`, `prev`, `seek`, `status`,
  `raise`, `quit`; `--help` and `--version` too.

## 0.18.0 - 2026-10-07

### Added
- **AppImage**: each release also has an `.AppImage` (with a checksum): one file with yt-dlp
  and deno as fallbacks (your own on `PATH` win). `mpv`, libxkbcommon, the Vulkan loader and a GPU
  driver still come from the system. Built by the release workflow's new
  `appimage` job (`packaging/appimage/build.sh`).

## 0.17.0 - 2026-10-07

### Changed
- **Faster Nix builds**: the flake builds the Rust dependencies as a derivation of their own
  (crane), so a new release recompiles only this app, not all ~730 crates. The flake gains a
  `crane` input.

### Added
- **Nix binary cache job**: the release workflow can build the package and push it to a Cachix
  cache (needs the repository variable `CACHIX_CACHE` and the secret `CACHIX_AUTH_TOKEN`; without
  them it only prints a notice). See the README's *Nix packaging*.

### Added
- **Long lists are cast in full** (up to 2000 videos, was 200): the first 200 go out at once and
  the next chunk is sent when the receiver's queue is within 50 of its end. Needs tg-mpv-bot 1.15
  (`POST /queue`); with an older bot the cast plays the first 200 and says it couldn't add more.
- **Next says where it goes**: when Next (N, the buttons) takes a video from Up next, a notice
  shows "Next from Up next: ..." and the button's tooltip names it, so a playlist that jumps to
  something queued earlier is no longer a mystery.

## 0.16.0 - 2026-10-07

### Added
- **Cast a whole list**: to a receiver with a `url` target (tg-mpv-bot 1.14 or newer) you can send
  an open playlist (the Cast icon in its header), the Up next queue (**Cast all** on the Up next
  tab), or any video and the ones after it in its list (right-click, **Cast from here**). The
  receiver plays them one after another by itself; the cast view shows "3 of 12" and the current
  title, with Previous / Next (also N and P) driving its queue. A playlist can also be cast
  without opening it: right-click it in the Playlists list, then Cast.
- **Right-click a playlist** in the Playlists list: **Play** plays the whole playlist here as the
  queue (no need to open it), **Cast** casts it. An open playlist's header has the same two as
  icons, next to "Add all to Up next". Playing a playlist this way empties Up next first, so Next
  follows the playlist instead of jumping to something queued earlier.
- **Casts go to your YouTube history** (logged in): each video that starts on the receiver is marked
  watched, like watching it here; before, a cast left no trace.
- While casting a list, the **main video area follows the receiver**: picture, title, channel and
  details switch to the video playing on the TV. Stopping the cast leaves that video ready to play here. Up to 200 videos at once. A
  `command` target casts the first video only.

## 0.15.0 - 2026-10-06

### Added
- **Right-click menu on every video**: Copy link, Add to (or Remove from) Up next, Save to Watch
  later, and your playlists in a scrolling list at the bottom, for videos in any list
  (Recommended, History, channel videos, New uploads, Up next, Watch later, search). Logged out it
  has Copy link and Up next.
- **Right-click on the playing video** opens the same menu at the pointer, with extra rows: copy link
  at the current time, loop, speed, subtitles, stats for nerds, open in browser. mpv's window hides
  while it is open, so the thumbnail shows behind the menu.
- The menu's playlists are only the ones you can edit, and a tick marks those that already hold the
  video (click a ticked one to remove it).

### Fixed
- The right-click menus (videos and subscriptions) scroll as a whole when the window is too short
  to show every row; before, only their lists shrank.

## 0.14.0 - 2026-10-06

### Added
- **Version and GitHub link**: the bottom of Settings shows the installed version and a GitHub button that opens the project's page.
- **Cast**: a button next to the player buttons (and **T**) sends the playing video to another
  device by running a command from `config.toml`: one `[cast.<name>]` table with a `command`
  argument list per target, using `{url}`, `{start}`, `{id}` and `{title}`. The command runs in the
  background (60 s limit, never through a shell); on success the local video pauses and the
  notice says "Sent to <name>", on failure it shows the receiver's own reason. The button shows
  only when a target exists, and Settings → Player buttons → Cast turns it off. Built with
  [tg-mpv-bot](https://github.com/antlis/tg-mpv-bot)'s new remote play API in mind (a `curl`
  target), but it works with anything that takes a link (`catt`, `ssh` plus a script, …). One
  video per cast; playlists aren't sent yet.
- **Cast command setting**: Settings → Cast command takes a command line (e.g.
  `catt -d "Living Room" cast {url}`, no token needed) and, when filled, replaces the `[cast.*]`
  targets of config.toml. Arguments split at spaces and quotes, never through a shell.
- **Cast control**: a target with `url` and `token` (a tg-mpv-bot with its remote API on) is also
  controlled from the app. The video area becomes "Casting to <name>" with the receiver's real
  position; Space, J/L, the arrow keys, the progress bar and the play, back and forward buttons
  drive the receiver, **Stop** stops it, and the view closes by itself when the receiver stops
  (finished, or stopped on the TV). Closing the window while casting asks whether to stop the
  receiver too, keep it playing, or cancel. Targets with only a `command` stay fire and forget.

## 0.13.0 - 2026-10-06

### Added
- **Settings → Subtitles**: a switch, the language (or several, `en,ru`; empty uses the system
  language), whether auto-generated and auto-translated captions count, and the size. Settings
  that had a subtitle language keep subtitles on. The CC button's first click turns the
  switch on.

### Fixed
- **Subtitles were missing for many videos**, such as the ones with only auto-generated captions
  or those YouTube serves only to browser-like requests: it answers mpv's own request for the
  caption file with HTTP 429. The app now downloads the captions with yt-dlp (cached for two
  weeks) and gives mpv the file. Without a login YouTube still refuses some videos; the
  subtitle notice says so.
- **Auto-generated captions were out of sync and hard to read.** YouTube writes them as "rolling"
  captions: two scrolling lines, with each new line showing before its words are spoken. They
  are now rewritten from the word times into short two-line phrases, each shown while it is
  spoken.
- Subtitles turned off with the CC button (or V) stayed off for every later video, though the
  language setting was still on. Each new video now starts with subtitles on again.

## 0.12.0 - 2026-10-06

### Added
- **Subtitles**: a CC button next to the player buttons (and the V key) turns subtitles on and
  off. The first use picks your system language; Settings → Other → Subtitles changes it, and
  several languages work (`en,ru`). The language setting now also makes yt-dlp fetch
  auto-generated and auto-translated captions, which mpv ignored before, so most videos have
  subtitles. A Settings → Player buttons toggle hides the button.
- **Pasted channel and playlist links open in the app**: `/@handle`, `/channel/UC…`, `/c/…`,
  `/user/…` and `/playlist?list=` links (in search, or with Ctrl+V) open that channel or
  playlist in the left column; video links play as before.

### Changed
- With nothing playing and logged out, the Recommendations pane shows the channel of the
  video waiting in the player, instead of repeating the home list.
- When the saved login stops working (the browser profile is gone, or YouTube no longer accepts
  the cookies), the account lists say so, and Settings → Account shows the Connection problem
  state with *Try again* and *Choose another browser*, instead of only failed lists.

## 0.11.0 - 2026-10-05

### Added
- **Play a YouTube link**: paste it into search and press Enter, or press Ctrl+V outside a
  text field to play the link on the clipboard (`watch?v=`, `youtu.be/`, `/shorts/`,
  `/live/`, `/embed/`; a start time from `t=` is honored, e.g. `t=1m30s`). The shortcut
  help lists it.
- **The repository is a Nix flake**: `nix profile install github:antlis/unbloated-youtube`,
  `nix run github:antlis/unbloated-youtube`, or the package / overlay as an input of a
  flake-based NixOS or home-manager config. It uses the same package and the same pinned
  nixpkgs as `default.nix`.

## 0.10.0 - 2026-10-05

### Added
- **Connect YouTube** (offered on first run, then in Settings → Account): pick a browser and
  the app reads that browser's own YouTube session — you sign in in your own browser, no
  password is ever asked or stored — then checks it in three steps (browser profile found,
  YouTube session valid, subscription feed reachable) and shows the account. **Log out**
  forgets the app's copy — and for a `config.toml` login comments those lines out instead
  (uncomment them to return). An exported `cookies.txt` can be imported under Advanced
  instead; a hand-written `config.toml` login still wins over the app's choice.
- Chromium's cookies live in the system keyring, which yt-dlp sometimes can't read on its
  first try (`cannot decrypt v11 cookies`): the login now retries with an explicit keyring
  suffix (`brave+gnomekeyring`) and remembers the spec that worked in `auth.json`.
- **An anonymous home feed** while logged out: YouTube's own home, trending and popular
  pages need a login, so instead the app shuffles random topic searches (music, gaming,
  documentaries, cooking, …) into a mixed video list — a new mix with each refresh, cached
  for the next logged-out launch.

### Changed
- Account lists that need a login now say "This list needs your YouTube account. Connect it
  in Settings → Connect YouTube." instead of showing the `config.toml` path to edit by hand.
- **Logged out, the app starts on the Connect YouTube screen** (dismissable with "Continue
  without account") and the account lists — subscriptions, feed, history, playlists,
  recommendations, Watch later — are not offered. Local data (groups, searches, resume
  positions) stays; the caches return with the next login.
- **Logged out, the header's account tabs are replaced by a Home pill and a Sign in button**
  (which opens Settings). Settings hides the
  toggles that do nothing without a login. The left column and the Recommendations pane show the
  anonymous feed instead, and the previously watched video is no longer restored — a
  logged-out launch starts with a clean player.
- **A video waits in the player without playing**: logged out, a random one from the
  anonymous feed; right after the first Connect, the latest one from your YouTube history.
- Settings → Account is always the first section (it used to move to the bottom once
  connected).
- **The History tab shows when you watched**: each YouTube history entry has its day
  ("Today", "Yesterday", "Saturday", …) in its details line, and the list follows YouTube's
  own order, then what only this app has seen. It is read from YouTube directly (with
  yt-dlp as the fallback), so it now includes Shorts (shown with the Shorts setting) and the
  most recent days, which yt-dlp's history missed. With Shorts on, History has Videos and
  Shorts tabs, so Shorts don't bury the videos.

### Fixed
- Videos that were only shown in the player (the random one logged out, the latest history
  video after Connect) are no longer loaded into mpv, whose `mark-watched` would have added
  them to your YouTube history without you playing them.
- Recommendations never loaded after connecting from Settings (the pane stayed a skeleton
  until a restart); the home feed now loads as soon as the connection is verified.
- A first Connect that fails is rolled back, so the account tabs no longer appear empty; the
  error now says what went wrong (no browser profile, browser not signed in to YouTube) and
  the panel says the check can take up to half a minute.
- Concurrent cookie exports (the Connect check next to a like/subscribe lookup) shared one
  temp file and failed with "couldn't read browser cookies"; each export has its own now.
- When the keyring retry fails, the first export's cookies are used instead of failing.
- A slow cookie export can no longer write a login back after logging out, and account
  fetches still running at log out no longer refill the cleared lists.
- An imported `cookies.txt` is created private (0600) from the start.
- The embedded video no longer floats above the Connect YouTube screen as a black rectangle
  (the mpv child window stacked over the overlay).
- Logging out stops the playing video and clears it from the player and lower pane, instead
  of leaving it running behind the connect screen.

## 0.9.0 - 2026-10-03

### Added
- **Watch later button** (and `W` / `w` in Vim mode) next to Save to playlist: adds the playing
  video to Watch later in one click. Every video in every list also gets a Watch later button on
  hover, so you don't have to open a video first. One Settings toggle, on by default.
- **Watch later tab** under the player (Settings → Watch later tab, off by default, needs your
  login): your Watch later list, fetched when you open the tab, with a remove button on each row.
  The lower-pane number keys now run 5–9.
- **Remove from history**: hover a video in History or Continue watching and click the trash icon.
  It forgets the entry (and its resume position) here; YouTube's own history is not changed.
- **Groups in the right-click menu**: right-click a subscribed channel to see every group with a
  tick on the ones it is in, and click one to add or remove it. Unsubscribe stays at the top; a
  long list of groups scrolls, and the menu stays inside the window. A folder icon in the channel
  list marks channels that are in a group (hover for the names).
- **A count badge on a channel's folder button** shows how many groups it is in; hover for names.
- The Open in browser tooltip shows its hotkey.
- The comments list ends with "All N comments shown" when there are no more to load, and its
  loading placeholder no longer shows thumbnails.

### Fixed
- The header's Search, Refresh and Settings tooltips open to the left, so the video no longer
  covers them.
- The window opens no bigger than the screen (at 1280×800 it could be taller than a scaled
  display, putting the bottom of the right column, such as Load more comments, off-screen).
- The channel right-click menu stays inside the left column, so the video no longer covers it.

## 0.8.2 - 2026-10-03

### Added
- **Automatic AUR publishing**: each release tag now updates the `unbloated-youtube-bin` package
  on the AUR (`packaging/aur/publish.sh`, run by the release workflow).

## 0.8.1 - 2026-10-03

### Added
- **MIT license** (`LICENSE`), also included in the release archive.
- **AUR package files** (`packaging/aur`, `unbloated-youtube-bin`), installing the prebuilt binary.

## 0.8.0 - 2026-10-03

### Added
- **`O` opens the video in your browser** (`o` in Vim mode), like the Open in browser button.
- **Prebuilt Linux binary** on each GitHub release (x86_64, glibc 2.35 or newer), with a desktop
  entry and icon, so you don't have to compile it.
- **Nix package** (`default.nix`, `package.nix`): installs the app with mpv, yt-dlp and deno on
  its `PATH`, plus the desktop entry and icon.

## 0.7.0 - 2026-10-03

### Added
- **Comments tab** under the player, off by default (Settings → Comments). The top 40 comments
  are fetched only when you open the tab, once per video; "Load more comments" at the bottom
  fetches 40 more.

## 0.6.0 - 2026-10-03

### Added
- **Hover controls on the video**: a YouTube-style bar appears when the pointer is over the video
  and hides after a moment: a seek line, play/pause, previous/next, mute and volume, the time,
  ±10 s, speed, picture-in-picture and fullscreen. While it is on, the playback row under the video
  is gone (the account/action buttons stay); turn it off to get the two button rows back.
  Settings > Player > *Hover controls on the video* (on by default), independent of mpv's own
  controls. It is a small Lua script, `src/controls.lua`, run by mpv, because the video is a native
  window that the app can't draw over.

### Changed
- **mpv's own controls and key bindings are off over the video by default**: no on-screen
  controller on hover, no mpv hotkeys, and the app's shortcuts now also work while the pointer is
  over the video. Settings > Player > *mpv controls and hotkeys* brings them back. The
  picture-in-picture window always keeps them, since it has nothing else.
- `f` and `Esc` leave fullscreen from the app, and a double click still toggles it.

## 0.5.0 - 2026-10-03

### Changed
- One tab scheme for both columns: numbers `1`–`4` pick the header tabs and `5`–`7` the lower
  pane's (Recommended, Chapters, Up next), and `[` / `]` cycle through all seven in that order,
  wrapping around, in regular and Vim mode. **`[` / `]` used to step only the lower pane's tabs.**

## 0.4.0 - 2026-10-03

### Added
- **Keyboard focus ring**: `Tab` / `⇧Tab` move a visible ring through every clickable thing, the
  left column first and then the right one; `Enter` or `Space` presses what has the ring, `Esc`
  or a click clears it. Works in regular and Vim mode; the click targets are the same ones hint
  mode uses, so they are now collected in regular mode too.

## 0.3.0 - 2026-10-03

### Added
- **Copy link at the current time**: a clock button after Share, `⇧C` (`y t` in Vim mode); the
  link opens the video at the current position (`?t=`). Switch in Settings > Player buttons.
- Right-click a channel in Subscriptions for a menu with **Unsubscribe** (asks for a second click
  to confirm). The menu closes by itself 2 seconds after the pointer leaves it, or on Esc or a click elsewhere.
- **Lower pane tab keys**: `[` and `]` switch between Recommended, Chapters and Up next (both modes).
- **Tab keys**: `1`–`4` jump to Subscriptions, Playlists, History and Settings (as shown), in
  both regular and Vim mode.
- **Player full height** with `⇧E`: hides the lower pane, the counterpart of `E`.
- **Collapse the right column** with `⇧B`, or by dragging the divider to the right edge. Only one
  column is hidden at a time.
- **Collapse the left column** with `B` (`b` in Vim mode) or by dragging its divider to the window
  edge; drag it back out to restore your width. Search, filter and tab keys bring it back.

### Changed
- The divider between the player and the lower pane (Recommended, Chapters, Up next) can be
  dragged all the way up, and `E` (`e` in Vim mode) toggles the lower pane to the full height of the
  right column. The video window hides itself when there is no room for it.
- Player buttons are in two rows again: playback and volume on top, account and other actions
  (subscribe, save, like, download, share, share at time, open in browser) below. The time stays
  beside the views and date.

### Fixed
- `j`/`k` in Vim mode: going up now scrolls the list back, so the selected row can no longer
  end up out of view.

## 0.2.0 - 2026-10-03

### Added
- **Volume**: mute button and a ten-step volume bar next to the speed button (hide it in
  Settings > Player buttons > Volume), `↑`/`↓` keys (`+`/`-` in Vim mode) that also work with
  the pointer over the video, listed in the `?` cheat sheets. The volume is remembered.
- **Open in browser** button: opens the video's page in your default browser
  (Settings > Player buttons > Open in browser).
- **Chapters tab** under the player (Settings > Show > Chapters): a clickable list of the
  video's chapters that highlights the current one. It appears only for videos with chapters.

### Changed
- The playback time moved to the right end of the views and date line, which keeps all
  player buttons in one row. The "Loading…" text next to it is gone; the red bar shows loading.
- The player pane has a minimum height so the controls always fit under the video.

## 0.1.0 - 2026-10-03

First tagged release. It includes everything under "Development history" below, plus:

### Added
- **Light theme**: Settings > Show > Light theme. Applies immediately and is remembered.
- **Video info**: views, upload date and channel subscriber count, each with its own switch in
  Settings > Video info. The playing video shows `views · date` under its title and the
  subscriber count on its channel chip (needs the login). Lists show view counts where yt-dlp
  provides them (search, playlists); the channel list shows subscriber counts.

### Fixed
- View count missing for videos uploaded in the last day or so (YouTube omits the short form).

## Development history before 0.1.0

### 2026-10-03

#### Added
- Filter any list as you type (Ctrl+F): channels, playlists, a channel's or playlist's videos,
  history. Local and instant.
- Resizable *Continue watching* in History (drag bar).
- Remove a video from a playlist (trash icon on hover; in Liked videos it unlikes).
- Add a whole playlist to Up next.
- Notices as toasts in the bottom-right corner, fading after a few seconds.
- Text fields: real cursor, Home/End, word jumps, Shift selection, Ctrl+A/C/X/V.
- Picture-in-picture: play in a small always-on-top mpv window.
- Channel groups: filter Subscriptions and New uploads by group.
- Mute a channel in New uploads; desktop notifications for uploads of channels with the bell on.
- Shorts tab, keyboard control, Up next queue, watched state, Vim mode with click hints.
- Copy-link hotkeys: `C`, or `yy` in Vim mode.
- Clickable channel chip under the player.
- `./run` launcher that caches the nix-shell environment.
- Full title on hover for truncated titles.
- README: motivation, features, settings, keys, architecture.

#### Changed
- Search is a mode instead of a tab.
- Compact channel header with icon buttons.
- Group bar: full-width filter on top, group chips with dim counts below.
- Left and right headers have the same height when window buttons are on.
- Shortcuts sheet scrolls when it doesn't fit the window.
- Clearer like/dislike/save feedback; refused playlist edits say why.

#### Fixed
- Playlists now load in full and saving to one confirms with a notice.
- "N new" tooltip no longer cropped by the video.
- Black video after changing player settings.

### 2026-10-02

First version: yt-dlp listings with settings, history and caches; a single mpv embedded in the
window over IPC; account actions (subscribe, like, dislike, save to playlist) through
InnerTube; built-in SVG icons; the GPUI interface.
