# JavaScript runtimes for yt-dlp

Measured 2026-10-28 on one Linux machine and one network, with the standalone `yt-dlp_linux`
2026.10.26 (the binary the AppImage bundles), deno 2.9.7 and quickjs-ng 0.17.0, logged out, on a
few public videos. Treat the numbers as one data point, not a benchmark.

## Why a runtime at all

YouTube hides the stream URLs behind JavaScript challenges (the signature and the "n" parameter).
yt-dlp solves them by running YouTube's player code in an external JavaScript runtime. Only deno
is enabled by default; yt-dlp also accepts node, bun and QuickJS (`--js-runtimes`). Without any,
yt-dlp still lists formats, with a warning that the challenges are unsolved.

## Time to look up one video (`-g`, caches off)

| Runtime | Time | Notes |
|---|---|---|
| none | ~2.8 s | warning: no supported JavaScript runtime; challenges unsolved |
| deno | ~4.3 to 5.0 s | |
| QuickJS (quickjs-ng) | ~10.7 to 11.8 s | about 6.5 s slower than deno |

With yt-dlp's cache on (a cold run, then three other videos) the numbers did not change: the cache
saves the player download, not the per-video solving. The `qjs` binary is 160 KB; deno is about
100 MB, which is most of the AppImage's size.

## Things that misled the first attempt

- **`--js-runtimes quickjs` adds to the defaults, it does not replace them.** With deno also
  present, the log says `[jsc:deno] Solving JS challenges using deno`. Check the log line, not the
  option.
- **The NixOS `yt-dlp` hardcodes deno's store path**, so it uses deno even with deno off `PATH`.
  To test another runtime, use the standalone `yt-dlp_linux` binary (checksum from
  `SHA2-256SUMS`) with only that runtime on `PATH`.
- My first comparison therefore showed "all runtimes about 4 s". It had used deno every time.

## Other ideas checked

- **`player_client=tv` (or `ios`) to skip the challenge.** With no runtime, the `tv` client
  returned no formats at all for the test video (`ERROR: The page needs to be reloaded`);
  `tv_simply` and `web_safari` warned that signature solving failed; `android_vr` skipped its
  formats for lack of a PO token. Logged out and one video, so a logged-in session might differ,
  but it is not a way to drop the runtime.
- **ytdlp-jsc** (github.com/ahaoboy/ytdlp-jsc, MIT, about 10 stars): a yt-dlp challenge provider
  plugin. Its README says it is a native Rust extension compiled per Python version (`cp313` as an
  example), installed with pip into yt-dlp's plugin folder, and does not say which JS engine it
  embeds. It lists no benchmark numbers (the "deno 1.59x slower than QuickJS" table that
  circulates is not in it, and contradicts the measurement above). From memory, not checked: the
  standalone yt-dlp binary bundles its own Python, so an extension would have to match that exact
  version. Using it would mean either shipping a Python install or maintaining a matching build.
- **node and bun** work as runtimes but are not smaller than deno.

## Decision

Keep deno as the default. QuickJS is only worth offering as an opt-in "small download" mode for
people who accept a slower start of every video (about 6.5 s). If the AppImage size ever matters
more, the open experiment is pip-installed yt-dlp plus ytdlp-jsc in a throwaway folder, to see its
real speed.

## Repeat it

```sh
# standalone yt-dlp and a runtime; each run uses only the runtime on PATH
env -i HOME=$HOME PATH=/path/to/runtime-dir ./yt-dlp_linux -v --no-cache-dir --no-config \
    --js-runtimes quickjs -g -f 'bv*+ba/b' 'https://www.youtube.com/watch?v=dQw4w9WgXcQ'
```
