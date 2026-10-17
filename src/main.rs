//! UnbloatedTube: a lightweight, configurable YouTube desktop client (GPUI, embedded mpv, yt-dlp).

mod app;
mod backend;
mod platform;
mod ui;

use std::time::Duration;

use gpui::{App, Application, Bounds, TitlebarOptions, WindowBounds, WindowOptions, prelude::*, px, size};

use app::Unbloated;
use backend::{account, cli, prefetch, store, yt};
use platform::mpris;
use ui::icons;

/// Print a command's answer and exit with its status.
fn finish(reply: cli::Reply) -> ! {
    match reply {
        Ok(message) => {
            println!("{message}");
            std::process::exit(0)
        }
        Err(error) => {
            eprintln!("unbloatedtube: {error}");
            std::process::exit(1)
        }
    }
}

fn main() {
    // mpv runs this program as its yt-dlp (see backend/prefetch.rs); nothing else of the app starts then.
    if std::env::var_os(prefetch::ENV).is_some() {
        prefetch::run_shim();
    }
    let args: Vec<String> = std::env::args().skip(1).collect();
    let start = match cli::parse(&args) {
        Ok(cli::Parsed::Print(text)) => return println!("{text}"),
        Ok(cli::Parsed::Run(command)) => match cli::send(&command) {
            Ok(reply) => finish(reply),
            Err(_) => {
                eprintln!("unbloatedtube: not running");
                std::process::exit(1);
            }
        },
        // A link goes to the running app; only without one does it start the app.
        Ok(cli::Parsed::Launch(Some(command))) => match cli::send(&command) {
            Ok(reply) => finish(reply),
            Err(_) => {
                if let cli::Command::Open { url } = &command {
                    if yt::parse_link(url).is_none() {
                        finish(Err(format!("not a YouTube link: {url}")));
                    }
                }
                Some(command)
            }
        },
        Ok(cli::Parsed::Launch(None)) => None,
        Err(message) => {
            eprintln!("unbloatedtube: {message}");
            std::process::exit(2);
        }
    };
    store::migrate_old_dirs();
    std::thread::spawn(prefetch::sweep);
    prefetch::start_helper();
    std::thread::spawn(|| account::keep_fresh());
    Application::new().with_assets(icons::Assets).run(move |cx: &mut App| {
        // At 1280x800, but no bigger than the screen: with display scaling that can be larger than it,
        // and the bottom of the window (and anything there) would be off-screen.
        let want = size(px(1280.), px(800.));
        let fit = cx.primary_display().map_or(want, |d| {
            let screen = d.bounds().size;
            size(px(f32::from(want.width).min(f32::from(screen.width) - 40.)), px(f32::from(want.height).min(f32::from(screen.height) - 80.)))
        });
        let bounds = Bounds::centered(None, fit, cx);
        let window = cx.open_window(
            WindowOptions {
                window_bounds: Some(WindowBounds::Windowed(bounds)),
                app_id: Some("unbloatedtube".into()),
                titlebar: Some(TitlebarOptions { title: Some("UnbloatedTube".into()), ..Default::default() }),
                ..Default::default()
            },
            |window, cx| cx.new(|cx| Unbloated::new(window, cx)),
        )
        .unwrap();
        // The link or command the app was started with.
        if let Some(command) = start {
            window.update(cx, |this, window, cx| this.run_command(command, window, cx)).ok();
        }
        // Commands from other command lines; the socket is only kept while this window lives.
        if let Some(requests) = cli::listen() {
            cx.spawn(async move |cx| {
                loop {
                    cx.background_executor().timer(Duration::from_millis(100)).await;
                    while let Ok(request) = requests.try_recv() {
                        let reply = window
                            .update(cx, |this, window, cx| this.run_command(request.command, window, cx))
                            .unwrap_or_else(|_| Err("the window is closed".into()));
                        let _ = request.reply.send(reply);
                    }
                }
            })
            .detach();
            cx.on_app_quit(|_| async { cli::cleanup() }).detach();
        }
        // Media keys, the desktop's player widget and playerctl (MPRIS, see platform/mpris.rs).
        let mpris = mpris::Mpris::start();
        cx.spawn(async move |cx| {
            loop {
                cx.background_executor().timer(Duration::from_millis(100)).await;
                let alive = window.update(cx, |this, window, cx| {
                    for command in mpris.commands() {
                        this.run_mpris(command, window, cx);
                    }
                    let (now, position) = this.mpris_snapshot();
                    mpris.update(now, position);
                });
                if alive.is_err() {
                    break;
                }
            }
        })
        .detach();
        cx.on_app_quit(|_| async { prefetch::cleanup() }).detach();
        // Quit once GPUI is done closing the window: quitting from inside its close handling
        // panics in its X11 client ("RefCell already borrowed").
        cx.on_window_closed(|cx| {
            cx.spawn(async |cx| {
                cx.update(|cx| cx.quit()).ok();
            })
            .detach();
        })
        .detach();
    });
}
