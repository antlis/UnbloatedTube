# Design notes

Why the app is built the way it is. The map of the code (layers, modules, where new code goes) is [ARCHITECTURE.md](../ARCHITECTURE.md).

```
┌────────────────────────── unbloatedtube (one process) ───────────────────────────┐
│  app/ + ui/ — GPUI: views, state, keyboard, settings, Vim mode, hint mode        │
│     │  background threads                                                        │
│     ▼                                                                            │
│  backend/ — no GPUI                                                              │
│    yt.rs ─ spawns ─► yt-dlp         account.rs ─ HTTPS ─► YouTube InnerTube      │
│    player.rs ─ JSON IPC ─► mpv      cast.rs ─► cast receivers                    │
│    store.rs: config, settings, history, caches on disk                           │
│    proxy.rs, thumbs.rs, dearrow.rs, http.rs, prefetch.rs (yt-dlp shim + helper)  │
│     │                                                                            │
│     ▼                                                                            │
│  platform/ — the only OS-specific code (Linux today)                             │
│    embed.rs: X11 child window mpv draws into    mpris.rs: media keys over D-Bus  │
│    ipc.rs  process.rs  paths.rs  fs.rs  desktop.rs                               │
└──────────────────────────────────────────────────────────────────────────────────┘
```

The layers, every module, how the app is put together and where new code goes are in
[ARCHITECTURE.md](../ARCHITECTURE.md).



- **Nothing blocks the UI thread.** yt-dlp runs, InnerTube requests and mpv queries all happen
  on background threads; results come back to the UI as they arrive.
- **Lists straight from YouTube.** The subscriptions feed, playlists (Watch later and Liked
  included), recommendations, search, channel tabs (Videos, Shorts, Live; also logged out),
  the list of your subscribed channels, a video's description and its comments are asked of YouTube's InnerTube API, one HTTPS request
  per page of about 20–100 videos, instead of starting yt-dlp, which takes seconds for the same
  list. If that fails or finds nothing, yt-dlp fetches the list as before.
- **One HTTP client.** InnerTube, thumbnails, dislike counts and cast receivers share one
  connection pool, so requests to the same host reuse an open connection instead of a new TLS
  handshake each. Thumbnails download at most 8 at a time, so the ones on screen aren't queued
  behind a whole list's.
- **Blocking work on its own threads.** yt-dlp runs, file I/O and HTTP requests run on a thread
  pool meant for blocking calls, so they never hold up GPUI's few background threads (which also
  carry the mpv polling).
- **Timing log.** `UNBLOATEDTUBE_TIMING=1 unbloatedtube` prints on stderr how long each list
  took, which way it came (InnerTube or yt-dlp) and how long a video took to start.
- **Streaming lists.** yt-dlp prints one JSON object per entry; the UI shows entries every
  150 ms while the rest loads. A cached copy of each list is shown immediately on revisit.
- **mpv state polling.** Position, duration, pause, chapters and fullscreen are queried over
  IPC every 250 ms in the background. mpv answers late while it opens a video, which is why
  this never happens on the UI thread.
- **Smooth streaming.** YouTube throttles a single long download to ~150 KB/s. mpv is given
  `request_size=10 MB` (FFmpeg 9), so it fetches in range requests at full speed.
- **Fast yt-dlp starts.** `PYCRYPTODOME_DISABLE_GMP=1` saves ~1.7 s per yt-dlp run on NixOS
  (pycryptodome otherwise invokes the C compiler looking for libgmp).
- **Videos are resolved ahead of time.** Finding a video's streams costs yt-dlp about 3 s. The app
  runs that lookup (mpv's own yt-dlp command, minus the watched ping) for the row the pointer
  rests on for a moment, for the Vim cursor, for the first rows of the list on screen, for the
  head of Up next and for the video Next would play, and keeps the answer for an hour in the
  cache folder (`ytdl/`). mpv is told to use this program as its yt-dlp (`ytdl_hook-ytdl_path`,
  with `UNBLOATED_YTDL_SHIM` set in its environment), and so are the app's own list, search,
  comment and subtitle requests. It prints the kept answer, sends the watched ping in the
  background, or runs the request. Chapters, titles, subtitles and SponsorBlock go through mpv as
  before. Settings → Player → "Load videos ahead" turns the lookups off.
- **A yt-dlp that stays running.** Starting yt-dlp takes about a second before it does anything.
  When the installed yt-dlp is a Python script (Nix, pip, pipx, most distro packages) the app
  starts one Python process that has yt-dlp loaded (`src/ytdl_helper.py`) and forks it for each
  request, handing it the caller's stdout and stderr, so a request starts in milliseconds. The
  helper ends with the app. With the standalone yt-dlp binary (or the AppImage's copy) there is
  nothing to load, and every request starts yt-dlp as before.
  The helper also keeps YouTube's player script and deno's preprocessing of it on disk (in
  yt-dlp's cache folder, per player version, two weeks), which every lookup otherwise downloads
  and redoes before solving YouTube's JavaScript challenge.
- **Faster lookup for ordinary videos, never for live ones.** A single video is looked up first
  with the HLS and DASH manifests skipped, which saves half a second. That answer is used only if
  yt-dlp says `live_status` is `not_live` and it has a length and streams; a live stream, a
  premiere, anything unusual and any failure is simply run again exactly as asked, and such
  answers are never kept.
- **A stuck video is loaded again.** When the playing video doesn't move for 10 seconds
  (YouTube's video server dropped the connection: a black picture at 0:00, or a stop mid-video),
  its looked-up answer is dropped and it is loaded once more at the same place, which usually
  gets another server; a second stall shows a notice instead.
- **mpv dies with the app.** It's started with `PR_SET_PDEATHSIG`, so closing the window never
  leaves audio playing.
- **SponsorBlock** runs inside mpv as the `sponsorblock_minimal` script, with the categories
  you chose.
- **Playing marks it watched.** mpv's yt-dlp gets `mark-watched`, so what you play here lands in
  your YouTube history. Loading a video into mpv (the paused preload of the last watched one)
  counts as playing, so a video that is only shown in the player is never preloaded.
