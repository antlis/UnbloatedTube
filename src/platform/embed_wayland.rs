//! The video under Wayland: a subsurface of the app's own window that the app renders into.
//!
//! X11 lets mpv draw into a child window (`mpv --wid`, see `embed.rs`); Wayland has no such thing,
//! so the app makes a `wl_subsurface` of its window's surface (gpui hands out the `wl_display` and
//! `wl_surface` pointers), puts an EGL surface on it, and renders mpv into that through libmpv.
//! The subsurface takes no input (its input region is empty), so the pointer reaches the app,
//! which passes it on to mpv.
//!
//! Everything is loaded at run time (EGL, libwayland-egl, libmpv), so a system without them only
//! loses this, not the app.
//!
//! Two threads: the UI thread places and sizes the subsurface (`WaylandVideo`); a render thread
//! owns the EGL context and mpv's render context and draws every frame (`render_loop`), because
//! mpv paces its frames by itself and the UI thread must never wait for one. mpv runs in this
//! process with the same options as the mpv program, including its IPC socket, so the rest of the
//! app controls it as before (`Host::start`).

use crate::platform::libmpv::{LibMpv, RENDER_UPDATE_FRAME};
use libloading::Library;
use std::ffi::{CString, c_char, c_void};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{Receiver, Sender, SyncSender, channel, sync_channel};
use std::sync::{Arc, Mutex};
use wayland_backend::client::{Backend, ObjectId};
use wayland_client::globals::{GlobalListContents, registry_queue_init};
use wayland_client::protocol::{
    wl_compositor::WlCompositor, wl_region::WlRegion, wl_registry::WlRegistry, wl_subcompositor::WlSubcompositor,
    wl_subsurface::WlSubsurface, wl_surface::WlSurface,
};
use wayland_client::{Connection, Dispatch, EventQueue, Proxy, QueueHandle};
use wayland_protocols::wp::viewporter::client::{wp_viewport::WpViewport, wp_viewporter::WpViewporter};

struct State;

impl Dispatch<WlRegistry, GlobalListContents> for State {
    fn event(_: &mut Self, _: &WlRegistry, _: <WlRegistry as Proxy>::Event, _: &GlobalListContents, _: &Connection, _: &QueueHandle<Self>) {}
}
macro_rules! ignore_events {
    ($($t:ty),*) => {$(
        impl Dispatch<$t, ()> for State {
            fn event(_: &mut Self, _: &$t, _: <$t as Proxy>::Event, _: &(), _: &Connection, _: &QueueHandle<Self>) {}
        }
    )*};
}
ignore_events!(WlCompositor, WlSubcompositor, WlSurface, WlSubsurface, WlRegion, WpViewporter, WpViewport);

type EglDisplay = *mut c_void;
type EglConfig = *mut c_void;
type EglContext = *mut c_void;
type EglSurface = *mut c_void;

const EGL_PLATFORM_WAYLAND_KHR: u32 = 0x31D8;
const EGL_OPENGL_ES_API: u32 = 0x30A0;
const EGL_OPENGL_API: u32 = 0x30A2;

fn debug() -> bool {
    std::env::var_os("UNBLOATED_WAYLAND_DEBUG").is_some()
}

/// The few EGL and libwayland-egl functions needed.
struct Egl {
    _egl: Library,
    _wl_egl: Library,
    initialize: unsafe extern "C" fn(EglDisplay, *mut i32, *mut i32) -> u32,
    bind_api: unsafe extern "C" fn(u32) -> u32,
    choose_config: unsafe extern "C" fn(EglDisplay, *const i32, *mut EglConfig, i32, *mut i32) -> u32,
    create_context: unsafe extern "C" fn(EglDisplay, EglConfig, EglContext, *const i32) -> EglContext,
    create_window_surface: unsafe extern "C" fn(EglDisplay, EglConfig, *mut c_void, *const i32) -> EglSurface,
    destroy_surface: unsafe extern "C" fn(EglDisplay, EglSurface) -> u32,
    destroy_context: unsafe extern "C" fn(EglDisplay, EglContext) -> u32,
    make_current: unsafe extern "C" fn(EglDisplay, EglSurface, EglSurface, EglContext) -> u32,
    swap_buffers: unsafe extern "C" fn(EglDisplay, EglSurface) -> u32,
    swap_interval: unsafe extern "C" fn(EglDisplay, i32) -> u32,
    get_proc_address: unsafe extern "C" fn(*const c_char) -> *mut c_void,
    window_create: unsafe extern "C" fn(*mut c_void, i32, i32) -> *mut c_void,
    window_resize: unsafe extern "C" fn(*mut c_void, i32, i32, i32, i32),
    window_destroy: unsafe extern "C" fn(*mut c_void),
    display: EglDisplay,
    config: EglConfig,
    context: EglContext,
}

impl Egl {
    fn load(wl_display: *mut c_void) -> Result<Self, String> {
        // SAFETY: the signatures are those of EGL 1.5 and libwayland-egl; the libraries stay loaded
        // as long as the struct (`_egl`, `_wl_egl`), which holds the function pointers.
        unsafe {
            let egl = Library::new("libEGL.so.1").map_err(|e| format!("libEGL: {e}"))?;
            let wl_egl = Library::new("libwayland-egl.so.1").map_err(|e| format!("libwayland-egl: {e}"))?;
            macro_rules! sym {
                ($lib:expr, $name:literal) => {
                    *$lib.get(concat!($name, "\0").as_bytes()).map_err(|e| format!("{}: {e}", $name))?
                };
            }
            let get_platform_display: unsafe extern "C" fn(u32, *mut c_void, *const isize) -> EglDisplay = sym!(egl, "eglGetPlatformDisplay");
            let mut this = Self {
                initialize: sym!(egl, "eglInitialize"),
                bind_api: sym!(egl, "eglBindAPI"),
                choose_config: sym!(egl, "eglChooseConfig"),
                create_context: sym!(egl, "eglCreateContext"),
                create_window_surface: sym!(egl, "eglCreateWindowSurface"),
                destroy_surface: sym!(egl, "eglDestroySurface"),
                destroy_context: sym!(egl, "eglDestroyContext"),
                make_current: sym!(egl, "eglMakeCurrent"),
                swap_buffers: sym!(egl, "eglSwapBuffers"),
                swap_interval: sym!(egl, "eglSwapInterval"),
                get_proc_address: sym!(egl, "eglGetProcAddress"),
                window_create: sym!(wl_egl, "wl_egl_window_create"),
                window_resize: sym!(wl_egl, "wl_egl_window_resize"),
                window_destroy: sym!(wl_egl, "wl_egl_window_destroy"),
                display: std::ptr::null_mut(),
                config: std::ptr::null_mut(),
                context: std::ptr::null_mut(),
                _egl: egl,
                _wl_egl: wl_egl,
            };
            this.display = get_platform_display(EGL_PLATFORM_WAYLAND_KHR, wl_display, std::ptr::null());
            if this.display.is_null() {
                return Err("no EGL display for the Wayland connection".into());
            }
            let (mut major, mut minor) = (0, 0);
            if (this.initialize)(this.display, &mut major, &mut minor) == 0 {
                return Err("eglInitialize failed".into());
            }
            // Desktop OpenGL 3.3 (core) first, what mpv likes best; else OpenGL ES 3.
            // Attributes: SURFACE_TYPE = WINDOW_BIT, RENDERABLE_TYPE, RGB 8 bits each, NONE.
            let desktop_config = [0x3033, 0x4, 0x3040, 0x8, 0x3024, 8, 0x3023, 8, 0x3022, 8, 0x3038];
            let es_config = [0x3033, 0x4, 0x3040, 0x40, 0x3024, 8, 0x3023, 8, 0x3022, 8, 0x3038];
            // Context: version 3.3 and the core profile (0x30FD mask, bit 1); or ES major version 3.
            let desktop_context = [0x3098, 3, 0x30FB, 3, 0x30FD, 1, 0x3038];
            let es_context = [0x3098, 3, 0x3038];
            for (api, config, context) in [(EGL_OPENGL_API, &desktop_config[..], &desktop_context[..]), (EGL_OPENGL_ES_API, &es_config[..], &es_context[..])] {
                let mut n = 0;
                if (this.bind_api)(api) == 0 || (this.choose_config)(this.display, config.as_ptr(), &mut this.config, 1, &mut n) == 0 || n == 0 {
                    continue;
                }
                this.context = (this.create_context)(this.display, this.config, std::ptr::null_mut(), context.as_ptr());
                if !this.context.is_null() {
                    if debug() {
                        eprintln!("wayland video: EGL {major}.{minor}, {}", if api == EGL_OPENGL_API { "OpenGL 3.3" } else { "OpenGL ES 3" });
                    }
                    return Ok(this);
                }
            }
            Err("no OpenGL context (needs OpenGL 3.3 or OpenGL ES 3)".into())
        }
    }

    fn proc(&self, name: &str) -> *mut c_void {
        let name = CString::new(name).unwrap();
        // SAFETY: eglGetProcAddress takes a NUL-terminated name.
        unsafe { (self.get_proc_address)(name.as_ptr()) }
    }
}

/// What the render thread owns, moved to it whole: raw EGL and Wayland handles that nothing else uses.
struct Owned {
    egl: Egl,
    window: *mut c_void,
    surface: EglSurface,
    wl_display: usize,
}
// SAFETY: the handles are used by the render thread only, after the move.
unsafe impl Send for Owned {}

/// How the video area is, for the render thread (the UI thread writes it).
#[derive(Clone, Copy, PartialEq)]
struct View {
    /// Buffer pixels.
    w: i32,
    h: i32,
    visible: bool,
}

enum Msg {
    /// The view changed: resize, show or hide, and draw.
    View,
    /// mpv has something to say (its update callback).
    Update,
    /// Start mpv with these options and load `url`; the answer is whether it came up.
    Start(Vec<(String, String)>, String, SyncSender<Result<(), String>>),
    Stop(SyncSender<()>),
    /// A command for the running mpv (the pointer: `mouse x y [button [mode]]`).
    Command(Vec<String>),
    Quit,
}

/// mpv in this process, as far as the rest of the app sees it: start it, stop it, ask if it runs.
/// (Everything else goes over mpv's IPC socket, like the mpv program.)
pub struct Host {
    tx: Sender<Msg>,
    view: Mutex<View>,
    alive: AtomicBool,
}

impl Host {
    /// Start mpv with `options` (name, value) and load `url`. Waits until it is up (a fraction of a
    /// second), so its IPC socket exists afterwards.
    pub fn start(&self, options: Vec<(String, String)>, url: String) -> Result<(), String> {
        let (reply, answer) = sync_channel(1);
        self.tx.send(Msg::Start(options, url, reply)).map_err(|_| "the video thread is gone".to_string())?;
        let result = answer.recv().map_err(|_| "the video thread is gone".to_string())?;
        self.alive.store(result.is_ok(), Ordering::SeqCst);
        result
    }

    /// Stop mpv (and wait for it to finish).
    pub fn stop(&self) {
        let (reply, answer) = sync_channel(1);
        if self.tx.send(Msg::Stop(reply)).is_ok() {
            let _ = answer.recv();
        }
        self.alive.store(false, Ordering::SeqCst);
    }

    pub fn alive(&self) -> bool {
        self.alive.load(Ordering::SeqCst)
    }

    /// Send mpv a command without waiting for it.
    pub fn command(&self, args: Vec<String>) {
        let _ = self.tx.send(Msg::Command(args));
    }
}

pub struct WaylandVideo {
    conn: Connection,
    queue: EventQueue<State>,
    state: State,
    child: WlSurface,
    sub: WlSubsurface,
    /// Sizes the surface exactly, whatever the display scale (None: the compositor has no
    /// wp_viewporter, and the buffer is a whole-number multiple of the logical size instead).
    viewport: Option<WpViewport>,
    host: Arc<Host>,
    thread: Option<std::thread::JoinHandle<()>>,
    /// Device pixels in the app's window, as `place` got them, and the scale it was given.
    rect: (i32, i32, u32, u32),
    scale: f32,
    wanted: bool,
    collapsed: bool,
}

impl WaylandVideo {
    /// `display` and `surface`: the `wl_display` and `wl_surface` of the app's window (gpui).
    pub fn new(display: usize, surface: usize) -> Option<Self> {
        Self::create(display, surface).map_err(|e| eprintln!("unbloatedtube: Wayland video: {e}")).ok()
    }

    fn create(display: usize, surface: usize) -> Result<Self, String> {
        // SAFETY: the pointers are the live display and window surface of gpui's window, which
        // outlives this (it is dropped with the app); the backend doesn't own the display.
        let (conn, parent) = unsafe {
            let backend = Backend::from_foreign_display(display as *mut _);
            let conn = Connection::from_backend(backend);
            let id = ObjectId::from_ptr(WlSurface::interface(), surface as *mut _).map_err(|e| format!("window surface: {e}"))?;
            let parent = WlSurface::from_id(&conn, id).map_err(|e| format!("window surface: {e}"))?;
            (conn, parent)
        };
        let (globals, queue) = registry_queue_init::<State>(&conn).map_err(|e| format!("registry: {e}"))?;
        let qh = queue.handle();
        let compositor: WlCompositor = globals.bind(&qh, 1..=4, ()).map_err(|e| format!("wl_compositor: {e}"))?;
        let subcompositor: WlSubcompositor = globals.bind(&qh, 1..=1, ()).map_err(|e| format!("wl_subcompositor: {e}"))?;
        let child = compositor.create_surface(&qh, ());
        let viewport = globals.bind::<WpViewporter, _, _>(&qh, 1..=1, ()).ok().map(|v| v.get_viewport(&child, &qh, ()));
        let sub = subcompositor.get_subsurface(&child, &parent, &qh, ());
        // Commits of its own, not waiting for the app's window to commit.
        sub.set_desync();
        // Nothing here takes the pointer: it goes to the app's window underneath.
        let none = compositor.create_region(&qh, ());
        child.set_input_region(Some(&none));
        none.destroy();
        // Opaque: lets the compositor skip what is behind.
        let opaque = compositor.create_region(&qh, ());
        opaque.add(0, 0, 16384, 16384);
        child.set_opaque_region(Some(&opaque));
        opaque.destroy();

        let egl = Egl::load(display as *mut c_void)?;
        // SAFETY: `child` is a live wl_surface; the wl_egl_window is destroyed by the render thread.
        let window = unsafe { (egl.window_create)(child.id().as_ptr().cast(), 16, 9) };
        if window.is_null() {
            return Err("wl_egl_window_create failed".into());
        }
        // SAFETY: the display, config and window are the live ones made above.
        let egl_surface = unsafe { (egl.create_window_surface)(egl.display, egl.config, window, std::ptr::null()) };
        if egl_surface.is_null() {
            return Err("no EGL surface".into());
        }
        let _ = conn.flush();

        let (tx, rx) = channel();
        let host = Arc::new(Host { tx: tx.clone(), view: Mutex::new(View { w: 16, h: 9, visible: false }), alive: AtomicBool::new(false) });
        let owned = Owned { egl, window, surface: egl_surface, wl_display: display };
        let (thread_host, thread_child, thread_conn) = (host.clone(), child.clone(), conn.clone());
        let thread = std::thread::Builder::new()
            .name("video".into())
            .spawn(move || render_loop(owned, rx, tx, thread_host, thread_child, thread_conn))
            .map_err(|e| format!("video thread: {e}"))?;
        Ok(Self { conn, queue, state: State, child, sub, viewport, host, thread: Some(thread), rect: (0, 0, 16, 9), scale: 1., wanted: false, collapsed: false })
    }

    /// The handle the player starts and stops mpv through.
    pub fn host(&self) -> Arc<Host> {
        self.host.clone()
    }

    /// Position in device pixels relative to the app's window (the video area).
    pub fn place(&mut self, x: i32, y: i32, w: u32, h: u32, scale: f32) {
        let collapsed = w < 40 || h < 40;
        let rect = (x, y, w, h);
        if rect == self.rect && scale == self.scale && collapsed == self.collapsed {
            return;
        }
        self.rect = rect;
        self.scale = scale;
        self.collapsed = collapsed;
        self.apply();
    }

    pub fn origin(&self) -> (i32, i32) {
        (self.rect.0, self.rect.1)
    }

    /// Buffer pixels per logical pixel: the display scale with a viewport, else its ceiling.
    fn buffer_scale(&self) -> i32 {
        if self.viewport.is_some() { 1 } else { self.scale.max(1.).ceil() as i32 }
    }

    /// A pointer position in the window (logical pixels, `scale` the display scale) as a position
    /// in the video, in the pixels mpv draws.
    pub fn video_point(&self, x: f32, y: f32, scale: f32) -> (i32, i32) {
        if self.viewport.is_some() {
            return ((x * scale).round() as i32 - self.rect.0, (y * scale).round() as i32 - self.rect.1);
        }
        let (s, bs) = (self.scale.max(1.), self.buffer_scale() as f32);
        (((x - self.rect.0 as f32 / s) * bs).round() as i32, ((y - self.rect.1 as f32 / s) * bs).round() as i32)
    }

    pub fn set_visible(&mut self, visible: bool) {
        if self.wanted != visible {
            self.wanted = visible;
            self.apply();
        }
    }

    /// Bring the subsurface to what was asked: its place and size, shown or hidden.
    fn apply(&mut self) {
        let _ = self.queue.dispatch_pending(&mut self.state);
        // The window's positions are logical. With a viewport the buffer is the video area in device
        // pixels, whatever the scale, and the compositor maps it onto its logical size; without
        // one it is `bs` times the logical size (whole numbers only).
        let scale = self.scale.max(1.);
        let bs = self.buffer_scale();
        let (lx, ly) = ((self.rect.0 as f32 / scale).round() as i32, (self.rect.1 as f32 / scale).round() as i32);
        let (lw, lh) = (((self.rect.2 as f32 / scale).round() as i32).max(1), ((self.rect.3 as f32 / scale).round() as i32).max(1));
        if debug() {
            eprintln!("wayland video: at {lx},{ly} {lw}x{lh}, scale {scale}, buffer scale {bs}, viewport {}, shown {}", self.viewport.is_some(), self.wanted && !self.collapsed);
        }
        // Takes effect with the next commit of the subsurface (the render thread's) and of the
        // window (the app's own frame).
        self.sub.set_position(lx, ly);
        let (bw, bh) = match &self.viewport {
            Some(v) => {
                v.set_destination(lw, lh);
                self.child.set_buffer_scale(1);
                (self.rect.2.max(1) as i32, self.rect.3.max(1) as i32)
            }
            None => {
                self.child.set_buffer_scale(bs);
                (lw * bs, lh * bs)
            }
        };
        let view = View { w: bw, h: bh, visible: self.wanted && !self.collapsed };
        let changed = std::mem::replace(&mut *self.host.view.lock().unwrap(), view) != view;
        if changed {
            let _ = self.host.tx.send(Msg::View);
        }
        let _ = self.conn.flush();
    }
}

impl Drop for WaylandVideo {
    fn drop(&mut self) {
        self.host.stop();
        let _ = self.host.tx.send(Msg::Quit);
        if let Some(t) = self.thread.take() {
            let _ = t.join();
        }
        self.sub.destroy();
        self.child.destroy();
        let _ = self.conn.flush();
    }
}

/// mpv running, and what draws it.
struct Playing {
    handle: *mut c_void,
    render: *mut c_void,
    /// The update callback's argument; dropped after the render context, which stops calling it.
    _wake: Box<Sender<Msg>>,
}

unsafe extern "C" fn get_proc_address(ctx: *mut c_void, name: *const c_char) -> *mut c_void {
    // SAFETY: `ctx` is the `Egl` of the render thread, which outlives the render context.
    let egl = unsafe { &*(ctx as *const Egl) };
    unsafe { (egl.get_proc_address)(name) }
}

unsafe extern "C" fn on_update(ctx: *mut c_void) {
    // SAFETY: `ctx` is the `Sender` boxed in `Playing`, alive until the render context is freed.
    let tx = unsafe { &*(ctx as *const Sender<Msg>) };
    let _ = tx.send(Msg::Update);
}

fn render_loop(owned: Owned, rx: Receiver<Msg>, tx: Sender<Msg>, host: Arc<Host>, child: WlSurface, conn: Connection) {
    let Owned { egl, window, surface, wl_display } = owned;
    // SAFETY: the EGL objects are live and used on this thread only from here on.
    unsafe {
        (egl.make_current)(egl.display, surface, surface, egl.context);
        // Not waiting for the compositor on each swap: mpv paces its frames itself.
        (egl.swap_interval)(egl.display, 0);
    }
    // SAFETY: GL entry points of the current context, with their C signatures.
    let gl_viewport: unsafe extern "C" fn(i32, i32, i32, i32) = unsafe { std::mem::transmute(egl.proc("glViewport")) };
    let gl_clear_color: unsafe extern "C" fn(f32, f32, f32, f32) = unsafe { std::mem::transmute(egl.proc("glClearColor")) };
    let gl_clear: unsafe extern "C" fn(u32) = unsafe { std::mem::transmute(egl.proc("glClear")) };

    let mut lib: Option<LibMpv> = None;
    let mut playing: Option<Playing> = None;
    let mut size = (16, 9);
    let mut attached = false;

    // Draw a frame, or hide the surface when the video isn't to be seen.
    let draw = |playing: &Option<Playing>, lib: &Option<LibMpv>, size: &mut (i32, i32), attached: &mut bool| {
        let view = *host.view.lock().unwrap();
        if !view.visible {
            if *attached {
                child.attach(None, 0, 0);
                child.commit();
                let _ = conn.flush();
                *attached = false;
            }
            return;
        }
        // SAFETY: the EGL context is current on this thread; `window` and `surface` are live.
        unsafe {
            if (view.w, view.h) != *size {
                (egl.window_resize)(window, view.w, view.h, 0, 0);
                *size = (view.w, view.h);
            }
            gl_viewport(0, 0, view.w, view.h);
            match (playing, lib) {
                (Some(p), Some(lib)) => {
                    if let Err(e) = lib.render(p.render, view.w, view.h) {
                        eprintln!("unbloatedtube: {e}");
                    }
                }
                _ => {
                    gl_clear_color(0., 0., 0., 1.);
                    gl_clear(0x4000);
                }
            }
            (egl.swap_buffers)(egl.display, surface);
        }
        if let (Some(p), Some(lib)) = (playing, lib) {
            lib.render_report_swap(p.render);
        }
        *attached = true;
    };

    while let Ok(msg) = rx.recv() {
        match msg {
            Msg::Quit => break,
            Msg::View => draw(&playing, &lib, &mut size, &mut attached),
            Msg::Update => {
                if let (Some(p), Some(l)) = (&playing, &lib) {
                    if l.render_update(p.render) & RENDER_UPDATE_FRAME != 0 {
                        draw(&playing, &lib, &mut size, &mut attached);
                    }
                }
            }
            Msg::Start(options, url, reply) => {
                if let Some(p) = playing.take() {
                    stop_playing(lib.as_ref(), p);
                }
                let result = (|| {
                    if lib.is_none() {
                        lib = Some(LibMpv::load()?);
                    }
                    let l = lib.as_ref().unwrap();
                    let handle = l.create()?;
                    for (name, value) in &options {
                        if let Err(e) = l.set_option(handle, name, value) {
                            eprintln!("unbloatedtube: mpv option {e}");
                        }
                    }
                    l.set_option(handle, "vo", "libmpv")?;
                    if let Err(e) = l.initialize(handle) {
                        l.terminate_destroy(handle);
                        return Err(e);
                    }
                    let wake = Box::new(tx.clone());
                    let render = match l.render_context(handle, get_proc_address, &egl as *const Egl as *mut c_void, wl_display as *mut c_void) {
                        Ok(r) => r,
                        Err(e) => {
                            l.terminate_destroy(handle);
                            return Err(e);
                        }
                    };
                    l.render_set_update_callback(render, on_update, &*wake as *const Sender<Msg> as *mut c_void);
                    let started = l.command(handle, &["loadfile", &url]);
                    playing = Some(Playing { handle, render, _wake: wake });
                    started
                })();
                let _ = reply.send(result);
                draw(&playing, &lib, &mut size, &mut attached);
            }
            Msg::Command(args) => {
                if let (Some(p), Some(l)) = (&playing, &lib) {
                    let refs: Vec<&str> = args.iter().map(String::as_str).collect();
                    let _ = l.command(p.handle, &refs);
                }
            }
            Msg::Stop(reply) => {
                if let Some(p) = playing.take() {
                    stop_playing(lib.as_ref(), p);
                }
                draw(&playing, &lib, &mut size, &mut attached);
                let _ = reply.send(());
            }
        }
    }
    if let Some(p) = playing.take() {
        stop_playing(lib.as_ref(), p);
    }
    // SAFETY: tearing down what this thread made current, in the order EGL needs.
    unsafe {
        (egl.make_current)(egl.display, std::ptr::null_mut(), std::ptr::null_mut(), std::ptr::null_mut());
        (egl.destroy_surface)(egl.display, surface);
        (egl.window_destroy)(window);
        (egl.destroy_context)(egl.display, egl.context);
    }
}

/// Free mpv's render context (GL context current), then the player, which may take a moment.
fn stop_playing(lib: Option<&LibMpv>, p: Playing) {
    if let Some(lib) = lib {
        lib.render_free(p.render);
        lib.terminate_destroy(p.handle);
    }
}
