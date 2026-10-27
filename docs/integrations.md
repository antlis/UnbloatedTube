# Command line, media keys and casting

How other programs talk to the app, and how it talks to other devices.

## Command line

```
unbloatedtube                 start the app
unbloatedtube <link>          play a YouTube link (video, channel or playlist)
unbloatedtube <command> ...   control the running app
```

The running app listens on a socket in `$XDG_RUNTIME_DIR`, so a link or command reaches it
instead of opening a second window; if it isn't running, a link starts it (and plays). A bare
`unbloatedtube` always starts a window, as before.

| Command | |
|---|---|
| `open <link>` | play a link (starts the app if needed) |
| `queue <link>` | add a video to Up next |
| `pause`, `play`, `toggle` | pause, resume, or flip; while casting they drive the receiver |
| `next`, `prev` | next or previous video |
| `seek <secs>` | jump to a time; `+10` / `-10` move from the current one |
| `status` | what is playing (state, title, position, cast target, Up next count) |
| `raise` | bring the window to the front |
| `quit` | close the app (a cast keeps playing) |

`-h` / `--help` and `-V` / `--version` work without the app. A command prints its answer and exits
0, or prints why it failed to stderr and exits 1 (2 for a mistake in the command line). Handy for
key bindings, e.g. `bindsym XF86AudioPlay exec unbloatedtube toggle`.

## Media keys (MPRIS)

The app is a media player on the session D-Bus (`org.mpris.MediaPlayer2.unbloatedtube`), so
with no setup the keyboard's media keys, the desktop's player widget (GNOME, KDE, waybar and the
like, with title, channel and thumbnail), Bluetooth headphone buttons, KDE Connect / GSConnect and
`playerctl` show what plays and control it: play/pause, next and previous, seeking, and bringing
the window up. While casting they drive the receiver. Stop only pauses. Linux only; without a
session bus the app runs as before.

## Cast targets

The **Cast** button (or **T**) sends the playing video's page link and position to another device,
and pauses the video here. The app knows no device itself: you tell it what to run. The button shows
only when a target exists (one button per target; **T** casts to the first, in name order), and
Settings → Player buttons → Cast hides it.

**A Chromecast or Google TV** (the common case) works through [`catt`](https://github.com/skorokithakis/catt)
("Cast All The Things"), a small command-line tool; no account or token is needed. Install it
(`pipx install catt`, or `nix run nixpkgs#catt`), find your device with `catt scan`, and put the
command in **Settings → Cast command**, with `{url}` standing for the video:

```
catt -d "Living Room" cast {url} -t {start}
```

`-d` is the device name `catt scan` printed (leave `-d "Living Room"` out if you have only one),
and `-t {start}` makes it start where you are, in whole seconds. That is all: press T on a video.
The same thing in `config.toml`, which also allows several devices, one button each:

```toml
[cast.living-room]
command = ["catt", "-d", "Living Room", "cast", "{url}", "-t", "{start}"]

[cast.bedroom]
command = ["catt", "-d", "Bedroom TV", "cast", "{url}", "-t", "{start}"]
```

Anything else that takes a link works the same way, as an argument list (never a shell string):
`["mpv", "{url}"]`, `["ssh", "tv-box", "play-video", "{url}"]`, a script of your own, `curl` against
some API. A command only *sends* the video: after that the app can't pause or seek the device
(use its own remote, or `catt pause`, `catt stop`).

**A receiver the app can control** is the other kind of target: a `url` and `token` instead of a
`command`, for a receiver speaking the remote API described below (the bot
[tg-mpv-bot](https://github.com/antlis/tg-mpv-bot) does, running mpv on a TV box). The app then
sends the video itself, shows what the device plays, and its controls drive the device:

```toml
[cast.tv]
url = "http://tv-box:8085"
token = "YOUR-TOKEN"
```

The Settings field and `config.toml` combine like this: when **Settings → Cast command** is
filled it wins, and the app has one target, named "device", ignoring the `[cast.*]` tables until
the field is emptied. A `url` target needs `config.toml`, because the field only holds a command.

- Placeholders: `{url}` (the video's page link), `{start}` (the current position, whole seconds),
  `{id}`, `{title}`. The command is an argument list, never a shell string: links and titles come
  from YouTube, so nothing is re-parsed by a shell (over `ssh` the remote shell does parse
  them again, so the receiving script must treat its arguments as data).
- It runs in the background with a 60 second limit. On success the notice says "Sent to tv" and
  the local video pauses, so it doesn't play twice; on failure the notice shows the receiver's own
  reason (the `error` of a JSON answer, else the first line it printed) and the video keeps playing.
- **Controlling the receiver.** With `url` and `token` (the receiver speaks tg-mpv-bot's remote API:
  `POST /play`, `GET /status`, `POST /ctl`) the app sends the video itself and then keeps
  asking the receiver how it is doing. The video area shows "Casting to <name>" with the real
  position, and the usual controls drive the receiver: Space, J/L and the arrow keys, the progress
  bar and the play, back and forward buttons. **Stop** stops the receiver; **Back to this screen**
  only closes the view. The view also closes by itself when the receiver stops (the video
  finished, or someone pressed stop on the TV). Closing the window (or the app's close button) while
  the cast view is open asks whether to stop the receiver, keep it playing, or cancel; "stop"
  waits at most 2 seconds for the receiver. A crash or kill can't ask, so the receiver keeps
  playing. Picking another video while the cast view is open sends it to the receiver too, which
  resolves it with its own login (age-restricted videos included); after **Back to this screen**
  videos play here again. A `command` target has none of this: it is fire and forget.
- The page link, not a stream link, is sent on purpose: the receiver's own yt-dlp resolves it at
  full quality, but it needs its own login for videos that need one. 
- **Casting a list.** With a `url` target (tg-mpv-bot 1.14 or newer) you can send more than one
  video, and the receiver plays them one after another by itself, so the app can close:
  - **right-click a playlist** in the Playlists list, then **Cast**: it sends the whole playlist
    without opening it (the same menu has **Play**, which plays the playlist here as the queue);
  - the **Cast icon in an open playlist's header** (next to "Add all to Up next" and a **Play**
    icon that plays the playlist here as the queue) sends the whole playlist from its first video;
  - **Cast all** at the top of the **Up next** tab sends the queue;
  - **right-click any video, then Cast from here**, sends that video and the ones after it in
    the list it is in (Recommended, History, a channel, search, ...).

  Up to 2000 videos are cast: the first 200 go out at once and the rest follows in chunks as the
  receiver's queue runs low (tg-mpv-bot 1.15 or newer; an older one plays the first 200 only). The
  cast view then shows "3 of 12" and the receiver's current
  title, and the main video area (picture, title, channel, details) follows the receiver to the video
  playing there. **Previous** / **Next** (also **N** and **P**, and the player's own buttons) move
  through the receiver's queue. A `command` target casts only the first video of a list. T and
  the Cast button still send just the playing video.

  **History:** when you are logged in, each video that starts on a `url` receiver is put in your
  YouTube history (the app asks YouTube to mark it watched, through yt-dlp and your browser login,
  so it is the same as watching it here); a `command` target marks its one video when you send it.
  It shows in the History tab after a refresh. The TV box itself doesn't need a YouTube login.
