# Logging in

How the app uses your YouTube account, and the limits of the browser-cookie login.

Subscriptions, playlists, history, recommendations and the account buttons need your YouTube
login, taken from your browser's cookies. On first run the app offers **Connect YouTube**, and
later under Settings → Connect YouTube: pick a browser, the app reads that browser's own
YouTube session (you stay signed in in your browser; no password is ever asked or stored),
checks it in three quick steps and shows your account. An exported `cookies.txt` can be
imported under Advanced instead. **Log out** forgets the app's copy; for a `config.toml`
login it comments those lines out instead (uncomment them to return).

Reading a browser's cookies takes a few seconds, so the app keeps the last read for the next
start, for up to 12 hours: in `$XDG_RUNTIME_DIR/unbloatedtube-cookies.txt` (a private folder
of your login session, emptied when it ends), readable only by you. When YouTube no longer
accepts them, whether the kept ones or the ones a running app started with, the app reads the
browser again, once; logging out or changing the login deletes the kept ones. A running app
also reads the browser again every 15 minutes, so its copy follows the cookies the browser
rotates, and a yt-dlp run that the kept copy failed (a list, captions, a video) drops it, so
the next run reads the browser. yt-dlp (the app's and mpv's) gets a copy of them too, made for
each run and deleted after it. Your playlists, like the other lists, are read with this login
(yt-dlp only as a fallback); if they can't be read, the lists you had stay on screen and
Settings → Account shows why.

If YouTube still refuses (lists don't load, Like says it no longer accepts your login), the
browser's own login has gone stale: open youtube.com in that browser (log in again if it asks),
then press **Try again** in Settings → Account. **Sign in again** next to it opens YouTube's
sign-in page in that browser for you. If signing in keeps failing, clear the cookies of
youtube.com and google.com in the browser (the app can't do that for it), then sign in again.

## Limits of the cookie login

The app has no login of its own: it reads your browser's YouTube session (with yt-dlp, from a
copy of the cookie file; it never writes to the browser) and presents it to YouTube. That is
convenient, nothing to type, and it has limits worth knowing:

- **YouTube rotates the cookies.** A copy that is some hours or days old can be refused, and
  YouTube then answers as if you were signed out: lists come back empty, Like, Save and the like
  fail. The app recognises this (it says "YouTube no longer accepts your saved login"), re-reads
  the browser once, and shows the failed step in Settings → Account. It also re-reads every 15
  minutes, and a list or caption request refused with the old copy still fails once before the
  next one succeeds. It can only read what the browser has written down.
- **The browser has to keep its session on disk.** A private window keeps its cookies in memory
  only, so they can't be read. A browser that doesn't write its cookies out (this happened once
  with Brave, for about a day, cause unknown) leaves the app with an old copy.
- **Using one session from two places may upset YouTube.** Suspected, not proven: if reading the
  browser's session while the browser uses it makes YouTube rotate it, either side can end up
  with a refused copy.
- **Chromium-based browsers need the system keyring** to decrypt their cookies (the app tries
  `gnomekeyring` when the plain read fails). If no keyring is running, the cookies can't be read.
- **One profile.** yt-dlp reads one profile of the browser (the default). Another profile, or a
  Firefox-based browser that isn't Firefox (Zen, LibreWolf), can be named in `config.toml` with
  yt-dlp's syntax, for example `cookies_from_browser = "firefox:/path/to/profile"`; the Settings
  list doesn't offer those.
- **Signing in to Google can fail in the browser itself** (a loop back to "There was an issue
  signing you in", Google showing you signed in while YouTube shows Sign in). That is the
  browser's or Google's side, not the app's. A private window that works while the normal one
  doesn't points at that profile's cookies; clearing the cookies of `youtube.com` and
  `google.com` fixed it once. A VPN exit Google distrusts is another possible cause. The app
  can't clear a browser's cookies for you: it is not its data to edit, and the browser keeps
  them in an encrypted database it holds open.
- **A login set in `config.toml` wins** over the one in Settings, and Log out comments it out
  there instead of forgetting it.
- **Windows and macOS** are untested; reading cookies may differ there (newer Chromium versions
  on Windows encrypt them in a way yt-dlp may not read).

What would remove most of these: a login the app owns (its own browser profile you sign in to
once, refreshed in the background). Not built yet; see the TODO.

While logged out, the app starts on that same **Connect YouTube** screen (or shows it after
logging out), the header's account tabs are replaced by **Home** and **Sign in** (which opens
Settings, with Account at the top), and the left column and Recommendations pane show an
**anonymous feed** — YouTube's own home and trending
need a login, so the app shuffles random topic searches into a mixed video list instead (a
new mix per refresh, cached); a random video of it waits in the player, not playing. Search
and channels work without an account; groups, searches and resume positions are kept locally.
Right after the first Connect, the latest video of your YouTube history waits there instead.

A `~/.config/unbloatedtube/config.toml` login still works and wins over the app's choice:

```toml
cookies_from_browser = "brave+gnomekeyring"   # yt-dlp syntax: "firefox", "chromium", …
# cookies_file = "/path/to/cookies.txt"       # or an exported Netscape cookies file
```

Without a login, search and playback still work; the account lists (subscriptions,
playlists, history, Watch later) stay hidden until you connect.
