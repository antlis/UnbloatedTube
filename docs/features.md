# Features

Everything the app can do today. For installing it see [install.md](install.md); for logging in, [login.md](login.md).

## Browsing

- **Subscriptions as a channel list**: avatars, A–Z, with channels that have new uploads on
  top with an unseen count. Click a channel for its videos (**Videos | Shorts | Live** tabs).
- **Live tab in Subscriptions**: **Channels | Live (N)** at the top; Live lists the streams that are
  on now from your subscriptions (the group chips narrow it), each with a red LIVE badge.
- **Unsubscribe from the list**: right-click a channel, then confirm with a second click.
- **New uploads**: one feed of the latest videos from all your subscriptions.
- **Channel groups**: put channels into groups (e.g. *Music*, *Tech*) and filter the channel
  list and the New uploads feed by group. Right-click a channel to tick the groups it belongs to;
  a folder icon in the list marks channels that are in a group (hover for the names). Groups are
  kept only in this app (`groups.json` in the data folder), not on YouTube.
- **Mute a channel**: hide its uploads from New uploads without unsubscribing.
- **Hide videos by keyword**: Settings → Hide videos with words takes comma-separated words or
  phrases (`reaction, prank, #shorts`); a video whose title says one, as a word of its own
  ("art" hides "Art school", not "start"; list "reactions" too if you want both), is hidden from
  feeds, channels, recommendations, Live and search, and gets no upload notification. Your own
  lists (History, Up next, Watch later, Downloads, your playlists) are left alone.
- **Upload notifications**: turn the bell on in a channel's header to get a desktop
  notification when it uploads (checked every 5–60 minutes, configurable).
- **Playlists**: Watch later, Liked videos and your own playlists, loaded in full.
- **History**: *Continue watching* (started but not finished, resizable with a drag bar), then
  your YouTube history in YouTube's own order, each entry with the day you watched it
  ("Today", "Saturday", …), then what only this app has seen. With Shorts on, Videos and
  Shorts have their own tabs.
- **Search**: a search mode with recent searches. Nothing is sent to YouTube until you press
  Enter.
- **Play a YouTube link**: paste it into search and press Enter, or press Ctrl+V anywhere
  outside a text field to play the link on the clipboard. Understands `watch?v=`, `youtu.be/`,
  `/shorts/`, `/live/` and `/embed/` links, with a start time from `t=` (`90`, `1m30s`). Channel
  links (`/@handle`, `/channel/UC…`, `/c/…`, `/user/…`) and playlist links (`/playlist?list=`)
  open that channel or playlist in the left column.
- **Cast**: a button (and **T**) sends the playing video to another device by running a command you
  set up per target in `config.toml`, for example a TV box running mpv ([see below](integrations.md#cast-targets)).
- **Subtitles**: a CC button next to the player's other buttons (or **V**) turns them on and
  off for the playing video; the first click turns subtitles on. Settings → Subtitles has the
  on/off switch, the language (or several, `en,ru`; empty uses your system language), whether
  YouTube's auto-generated and auto-translated captions count, and the size. The captions are
  downloaded with yt-dlp (and cached for two weeks), because YouTube refuses to serve them to mpv
  itself. Auto-generated captions are rewritten into short phrases that follow the speech
  (YouTube's own file scrolls two lines and shows words early). For some videos YouTube also refuses yt-dlp without a login: Connect YouTube for those.
- **Filter any list as you type** (Ctrl+F): channels, playlists, a channel's or playlist's
  videos, history. This is local and instant, with no request to YouTube.
- **Video info**: views, upload date and subscriber count for the playing video (needs your login),
  view counts in lists that provide them, subscriber counts in the channel list. Each is a Settings toggle.
- Watched videos are dimmed, and partly watched ones show a progress bar on the thumbnail.
- Lists appear instantly from a local cache and refresh in the background; long lists stream
  in as yt-dlp produces them.

## Playlists done right

- **Save to playlist** opens a full-height picker **with search**: type a few letters, press
  Enter. It lists the playlists you can add to (what YouTube's own Save menu offers) and Watch
  later, not the ones you only saved from other people; those stay in the Playlists tab.
- **Remove from playlist**: hover a video in an open playlist and click the trash icon (in
  Liked videos this unlikes it). It removes that one entry; if the video is in the playlist
  several times, the first copy goes.
- **Create, rename and delete your playlists**: **+ Playlist** in the Playlists tab makes a new
  private one; right-click a playlist for **Rename** and **Delete** (asks for a second click;
  the videos themselves stay on YouTube).
- **Save the Up next queue as a playlist**: the **Save as playlist** chip in Up next asks for a
  name and makes a private playlist of the queue.
- **Remove watched from Watch later**: one chip takes out everything you have finished or have in
  your YouTube history (asks for a second click).
- **Playlist settings**: right-click one of your playlists for **Edit description** and
  **Make public / unlisted / private** (the one it already has is left out).
- **Who can see a playlist** shows as an icon in the Playlists list: a lock (private), a link
  (unlisted) or a globe (public). The **Mine** and **Saved** chips show only your own playlists or
  only the ones you saved from other people.
- **Add a whole playlist to Up next** with one button in the playlist header.
- **Right-click any video** (Recommended, History, a channel's videos, New uploads, Up next, Watch
  later, search results): **Copy link**, **Add to Up next** (or remove it), **Save to Watch later**,
  and below a divider your own playlists in a scrolling list (the ones YouTube's Save menu offers),
  ticked where the video already is; click one to add the video, or a ticked one to remove it.
  On the playing video the same menu also has copy link at the current time, loop, speed, subtitles,
  stats for nerds and open in browser (the video is replaced by its thumbnail while it is open).
  Logged out you get Copy link and Up next only. Escape or a click elsewhere closes it.
- A notice confirms every action (or says why YouTube refused it, e.g. for playlists you
  saved from someone else, which you can't edit).

## Player

- mpv **embedded in the window**, with seek bar, chapters (hover for title and time, and a small picture of the video at that moment, like YouTube's, on the bar over the video and the one under it), time
  display, prev/next, ±10 s, fullscreen and speed.
- Opens on **the last video you watched, at the position you left it**; every video resumes
  where you stopped.
- **Up next** queue (add with the `+` on any row); otherwise Next/autoplay continues through the
  list you picked the video from. When Next takes a video from Up next, a notice says so (and
  the Next button's tooltip names it).
  Playing a whole playlist (the **Play** icon in its header, or right-click, **Play**) empties
  Up next first, so Next follows the playlist.
- **Picture-in-picture**: move playback into a small always-on-top mpv window and keep
  browsing.
- **Play in separate window** (right click on a video, in a list or on the player): the video
  in an mpv window of its own, from where you left it, while the app's player keeps what it
  plays: two videos at once, or one kept on another screen. mpv's own controls and keys; the same
  quality, login and SponsorBlock settings; it closes with the app.
- **Volume**: mute button and a volume bar next to the speed button, or `↑`/`↓` (`+`/`-` in Vim
  mode), also while the pointer is over the video. Remembered between runs.
- **Chapters tab** under the player for videos that have chapters: click one to jump there; the
  current chapter is highlighted and kept in view.
- **Description tab** under the player (Settings → Description turns it off): the video's
  description, fetched when you open the tab. Timestamps in it (`12:34`, `1:02:03`) jump the
  video there, `#tags` are searched for, `@handles` and YouTube links open in the app, and other
  links in the browser; the same works in comments.
- **Likes and dislikes** of the playing video in the line under its title ("412 likes · 23
  dislikes"), from the [Return YouTube Dislike](https://returnyoutubedislike.com) project: YouTube
  hides dislikes, so the number is the project's estimate. Works logged out too. The request
  tells the project which video you watch; Settings → Likes and dislikes turns it off.
- **Better titles and thumbnails (DeArrow)**, off by default: titles and thumbnails the
  community wrote to replace clickbait ones, from the [DeArrow](https://dearrow.ajay.app) project
  (by the SponsorBlock authors). Settings → DeArrow turns titles and thumbnails on separately. A
  replaced title shows YouTube's own when you hover it in a list, and under it on the player.
  Only what is on screen is asked for, by the first 4 hex digits of the SHA-256 of the video id
  (as the browser extension does), so DeArrow doesn't learn which video; without a submission,
  or when DeArrow can't be reached, YouTube's title and thumbnail stay.
- **Subtitle language picker**: right-click the video → Subtitles: … lists the video's own
  caption tracks, its auto-generated one, and auto-translations into your languages (Settings →
  Subtitle language); a pick shows those captions at once, for this video only, and the
  Transcript tab follows it. Off hides them.
- **Sleep timer**: right-click the video → Sleep: Off, End of this video, or 15 to 90 minutes.
  When the time is up the video pauses (the receiver too while casting); "End of this video"
  lets it finish without autoplay or Up next starting another. The corner shows what it waits
  for ("Sleep in 23 min"); the same menu turns it off.
- **Quality picker**: right-click the video → Quality: 1080p shows the playing quality and a
  list (Auto, 2160p … 144p, Audio only); a pick reloads the video at once from where it was, for
  this video only (the next one plays at Settings → Max quality again). A height is a maximum:
  a video without it plays the best below. Not for downloaded files or while casting.
- **Transcript tab** under the player: the video's captions as a list of lines with their times,
  the one being said highlighted. Type a word in its search field to see every moment it is
  said (the word is marked, with a count); click a line to jump there. It uses the same captions
  as the subtitles (Settings → Subtitles: languages, auto-generated captions) and the same
  cache, so it is instant for a video whose subtitles you had on. Settings → Transcript hides it.
- **Downloads tab**: videos saved with the Download button, newest first, with a filter. They
  play from the file, so also offline (and logged out). Right-click → "Delete downloaded file"
  deletes it; a file deleted or moved outside the app drops off the list. The tab shows once
  something is downloaded (Settings → Downloads tab hides it).
- **Comments tab** under the player, off by default (Settings → Comments): the top 40 comments are
  fetched only when you open the tab, and "Load more comments" at the bottom gets 40 more.
- **Resizable player**: drag the divider between the video and the lower pane anywhere, up to
  the top; `E` gives the lower pane the whole right column and back; `⇧E` does the same for the player. The left column collapses too
  (`B`, or drag its divider to the edge), and so does the right one (`⇧B`, or drag the divider
  to the right edge).
- **Hover controls**: move the pointer over the video and a YouTube-style bar appears: a seek
  line, then play/pause, previous/next, mute and volume and the time on the left, and ±10 s,
  subtitles (CC: filled while they show, like V), speed, picture-in-picture and fullscreen on the
  right. The CC button under the player then only shows where the bar can't be used (hover
  controls off, picture-in-picture, casting); both follow Settings → Player buttons → Subtitles. It hides itself after a moment. With it on, the
  row of playback buttons under the video is gone and only the account/action buttons remain; with
  it off (Settings, *Hover controls on the video*) you get the two button rows instead. The bar is
  drawn by mpv from a small bundled script, since the video is a separate native window the app
  can't draw over.
- **No mpv chrome by default**: mpv's own on-screen controls and key bindings are switched off
  over the video, so the app's shortcuts work there too. Turn them back on in Settings (*mpv
  controls and hotkeys*) if you want mpv's `osc`, wheel volume, `s` for screenshots and so on.
- Click the video to pause, double-click for fullscreen. The app's own shortcuts work with the
  pointer over the video too, since mpv's key bindings are off by default.
- **Recommendations** under the player (your YouTube home feed), or turn them off.
- **Watch later tab** under the player (off by default, Settings → Watch later tab): your Watch
  later list, loaded when you open the tab, with a remove button on each row.

## Account actions

Subscribe/unsubscribe, save to playlist, Watch later (one click, `W`; also on every video row), like, dislike, copy link (also at the current time), open in browser and download, all as
icon buttons with tooltips. Each one can be hidden in Settings (like and dislike are hidden by
default).

## Settings

Everything is a toggle or a field on the Settings page, which has its own search box:

| Group | Options |
| --- | --- |
| Account | **Connect YouTube**: pick a browser and test its session, or import a `cookies.txt` (see [Login](login.md)) |
| Tabs & lists | Subscriptions, Playlists, History, Downloads, Recommendations, Description, Transcript, Chapters, Comments and Watch later tabs (the last two off by default), **Shorts** (everywhere) |
| Buttons | Back / forward, Subscribe, Save to playlist, Watch later, Like, Dislike, Volume, Subtitles, Cast, Share, Share at current time, Open in browser, Download |
| Subtitles | On/off, language(s), auto-generated captions, size |
| Player | Max quality (480p–4K), autoplay, **audio only**, prefer hardware-friendly codecs (skip AV1), hardware decoding, hover controls on the video, mpv's own controls and hotkeys, speed |
| SponsorBlock | **Skip sponsored segments**, choosing which: sponsor, self-promotion, like/subscribe reminders, intro, credits, preview, filler, non-music |
| Video info | **Views**, **upload date**, **likes and dislikes** (playing video), **subscriber counts** (channels) |
| Network | **Connection**: Direct, **Bypass slowdown** (three methods) or **Proxy** (Tor and ByeDPI presets, or any address), **Test connection** (see [below](network.md)) |
| Other | Extra mpv options, download folder, words to hide videos by, upload notifications and their interval, Vim mode, window buttons, **light theme** |

At the very bottom of the page, a **GitHub** button opens the project's page and the installed version is shown next to it.

Window layout (column width, player height, Continue watching height) is set by dragging and
remembered.


## Keyboard

`?` shows every shortcut. Defaults:

| Key | Action |
| --- | --- |
| Space / K | Play / pause |
| ← / → | Back / forward 5 s |
| J / L | Back / forward 10 s |
| F | Fullscreen |
| M | Mute |
| V | Subtitles on / off (`v` in Vim mode) |
| ↑ / ↓ | Volume up / down 5% (`+` / `-` in Vim mode) |
| N / P | Next / previous (in the list, Up next first) |
| Alt+← / Alt+→ | Back / forward through the videos you played (also the arrows at the start of the button row) |
| C | Copy the video's link |
| ⇧C | Copy the link at the current time (`y t` in Vim mode) |
| O | Open the video in your browser (`o` in Vim mode) |
| W | Add the video to Watch later (`w` in Vim mode) |
| E | Lower pane (Recommended, Description, Transcript, Chapters, Watch later, Comments, Up next) full height, and back (`e` in Vim mode) |
| ⇧E | Player full height (hide the lower pane), and back |
| Tab / ⇧Tab | Move a focus ring through every clickable thing (tabs, rows, buttons, switches); Enter or Space presses it, Esc clears it |
| 1 – 4 | Switch tab: Subscriptions, Playlists, History, Settings (with the Downloads tab: 1 – 5, and the lower pane's tabs start at 6) |
| 5 – 9 | Switch lower pane tab: Recommended, Description, Transcript, Chapters, Watch later, Comments, Up next (as shown; the first five) |
| [ / ] | Previous / next tab: through 1–4, then 5–9, wrapping around |
| B | Hide or show the left column (`b` in Vim mode) |
| ⇧B | Hide or show the right column |
| / | Search |
| Ctrl+F | Filter the list |
| Ctrl+V | Play the YouTube link on the clipboard |
| T | Cast the video to the first cast target (`t` in Vim mode) |
| Esc | Close / back |

Text fields support Home/End, arrows (Ctrl: by word), Shift to select, Ctrl+A/C/X/V.

### Vim mode

Turn it on in Settings. It adds `j`/`k`, `gg`/`G`, `Ctrl-d`/`Ctrl-u` to move through lists,
`Enter`/`l` to open, `h` to go back, `H`/`L` to switch tabs, `x` to queue a video, `yy` to copy
the link (`yt` at the current time), and the rest of the keys marked "Vim mode" above.
`f` stays fullscreen, as without Vim mode; `Shift+F` shows the click hints.

**Hint mode.** Press `Shift+F` and every clickable thing on screen (tabs, rows, buttons, chips, switches)
gets a short letter label; type the label to click it, `Esc` to cancel. It is the same idea as
the link hints in browser extensions like [Vimium](https://github.com/philc/vimium),
[Tridactyl](https://github.com/tridactyl/tridactyl) and
[Surfingkeys](https://github.com/brookhong/Surfingkeys): you never need the mouse, even for
things that have no shortcut of their own. See
[Vimium's description of link hints](https://github.com/philc/vimium#keyboard-bindings)
(the `f` command there) if you haven't used one.
