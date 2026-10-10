"""A yt-dlp that stays running, for unbloatedtube (see src/prefetch.rs).

Starting yt-dlp costs about a second before it does anything, mostly importing itself. This
imports it once and forks for every request: the request names the arguments and carries the
caller's stdout and stderr (SCM_RIGHTS), so the fork writes straight into them, as a fresh
yt-dlp would. The answer is the exit code, as a line. Usage: python -c <this> SOCKET SCRIPT,
where SCRIPT is the yt-dlp program: its own start-up runs (it sets up the module path), its
main() does not.
"""

import json
import os
import signal
import socket
import sys
import traceback

sock_path, script = sys.argv[1], sys.argv[2]
sys.argv = [script]
with open(script, "rb") as f:
    exec(compile(f.read(), script, "exec"), {"__name__": "ytdl_helper_preload", "__file__": script})
import yt_dlp  # noqa: E402

try:
    # The extractors are what takes the time; load them before the first request.
    yt_dlp.extractor.gen_extractor_classes()
except Exception:
    pass


def keep_player_work():
    """Every request is a fresh fork, so what yt-dlp learns about YouTube's player script dies
    with it: each lookup downloaded the script again (MBs) and had deno parse and preprocess all
    of it before solving the challenge. Keep both on disk, in yt-dlp's own cache folder, per
    player version. Written against yt-dlp's internals: if they change, this changes nothing."""
    try:
        from yt_dlp.extractor.youtube.jsc._builtin import ejs

        # Off upstream only because the files are large (a few MB per player version).
        ejs.EJSBaseJCP._ENABLE_PREPROCESSED_PLAYER_CACHE = True
    except Exception:
        pass
    try:
        from yt_dlp.extractor.youtube import _video

        load_player = _video.YoutubeIE._load_player

        def cached_load_player(self, video_id, player_url, fatal=True):
            try:
                key = self._player_js_cache_key(player_url)
                if key not in self._code_cache:
                    code = self.cache.load("unbloatedtube-player", key)
                    if isinstance(code, str) and code:
                        self._code_cache[key] = code
                        return code
                code = load_player(self, video_id, player_url, fatal)
                if code:
                    self.cache.store("unbloatedtube-player", key, code)
                return code
            except Exception:
                return load_player(self, video_id, player_url, fatal)

        _video.YoutubeIE._load_player = cached_load_player
    except Exception:
        pass


keep_player_work()

server = socket.socket(socket.AF_UNIX, socket.SOCK_STREAM)
try:
    os.unlink(sock_path)
except FileNotFoundError:
    pass
server.bind(sock_path)
os.chmod(sock_path, 0o600)
server.listen(32)
signal.signal(signal.SIGCHLD, signal.SIG_IGN)

while True:
    try:
        conn, _ = server.accept()
    except InterruptedError:
        continue
    if os.fork():
        conn.close()
        continue
    code = 1
    try:
        server.close()
        data, fds, _, _ = socket.recv_fds(conn, 1 << 20, 2)
        argv = json.loads(data)["argv"]
        os.dup2(fds[0], 1)
        os.dup2(fds[1], 2)
        devnull = os.open(os.devnull, os.O_RDONLY)
        os.dup2(devnull, 0)
        for fd in [*fds, devnull]:
            if fd > 2:
                os.close(fd)
        try:
            yt_dlp.main(argv)
            code = 0
        except SystemExit as e:
            if isinstance(e.code, int):
                code = e.code
            elif e.code is None:
                code = 0
            else:
                sys.stderr.write(f"{e.code}\n")
        except BaseException:
            traceback.print_exc()
        sys.stdout.flush()
        sys.stderr.flush()
    except BaseException:
        pass
    finally:
        try:
            conn.sendall(b"%d\n" % code)
        except OSError:
            pass
        os._exit(0)
