//! The video under Wayland: a subsurface of the app's own window that the app renders into.
//!
//! X11 lets mpv draw into a child window (`mpv --wid`, see `embed.rs`); Wayland has no such thing,
//! so the app makes a `wl_subsurface` of its window's surface (gpui hands out the `wl_display` and
//! `wl_surface` pointers), puts an EGL surface on it, and renders mpv into that through libmpv.
//! The subsurface takes no input (its input region is empty), so the pointer reaches the app,
//! which passes it on to mpv.
//!
//! Everything is loaded at run time (EGL, libwayland-egl), so a system without them only loses
//! this, not the app.

use libloading::Library;
use std::ffi::{c_char, c_void};
use wayland_backend::client::{Backend, ObjectId};
use wayland_client::globals::{GlobalListContents, registry_queue_init};
use wayland_client::protocol::{
    wl_compositor::WlCompositor, wl_region::WlRegion, wl_registry::WlRegistry, wl_subcompositor::WlSubcompositor,
    wl_subsurface::WlSubsurface, wl_surface::WlSurface,
};
use wayland_client::{Connection, Dispatch, EventQueue, Proxy, QueueHandle};

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
ignore_events!(WlCompositor, WlSubcompositor, WlSurface, WlSubsurface, WlRegion);

type EglDisplay = *mut c_void;
type EglConfig = *mut c_void;
type EglContext = *mut c_void;
type EglSurface = *mut c_void;

const EGL_PLATFORM_WAYLAND_KHR: u32 = 0x31D8;
const EGL_OPENGL_ES_API: u32 = 0x30A0;

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
            let initialize: unsafe extern "C" fn(EglDisplay, *mut i32, *mut i32) -> u32 = sym!(egl, "eglInitialize");
            let bind_api: unsafe extern "C" fn(u32) -> u32 = sym!(egl, "eglBindAPI");
            let choose_config: unsafe extern "C" fn(EglDisplay, *const i32, *mut EglConfig, i32, *mut i32) -> u32 = sym!(egl, "eglChooseConfig");
            let create_context: unsafe extern "C" fn(EglDisplay, EglConfig, EglContext, *const i32) -> EglContext = sym!(egl, "eglCreateContext");
            let mut this = Self {
                initialize,
                bind_api,
                choose_config,
                create_context,
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
            if (this.bind_api)(EGL_OPENGL_ES_API) == 0 {
                return Err("no OpenGL ES".into());
            }
            // EGL_SURFACE_TYPE = WINDOW_BIT, EGL_RENDERABLE_TYPE = OPENGL_ES2_BIT, RGB 8 bits each.
            let want = [0x3033, 0x4, 0x3040, 0x4, 0x3024, 8, 0x3023, 8, 0x3022, 8, 0x3038];
            let mut n = 0;
            if (this.choose_config)(this.display, want.as_ptr(), &mut this.config, 1, &mut n) == 0 || n == 0 {
                return Err("no EGL config".into());
            }
            // EGL_CONTEXT_CLIENT_VERSION = 2
            let attribs = [0x3098, 2, 0x3038];
            this.context = (this.create_context)(this.display, this.config, std::ptr::null_mut(), attribs.as_ptr());
            if this.context.is_null() {
                return Err("no EGL context".into());
            }
            Ok(this)
        }
    }

    fn proc(&self, name: &str) -> *mut c_void {
        let name = std::ffi::CString::new(name).unwrap();
        // SAFETY: eglGetProcAddress takes a NUL-terminated name.
        unsafe { (self.get_proc_address)(name.as_ptr()) }
    }
}

pub struct WaylandVideo {
    conn: Connection,
    queue: EventQueue<State>,
    state: State,
    child: WlSurface,
    sub: WlSubsurface,
    egl: Egl,
    window: *mut c_void,
    surface: EglSurface,
    /// Device pixels in the app's window, as `place` got them, and the scale it was given.
    rect: (i32, i32, u32, u32),
    scale: f32,
    wanted: bool,
    collapsed: bool,
    shown: bool,
    /// What the surface was last sized to (buffer pixels) and whether it was drawn since.
    sized: (i32, i32),
    dirty: bool,
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
        // SAFETY: `child` is a live wl_surface; the wl_egl_window is destroyed in `drop`.
        let window = unsafe { (egl.window_create)(child.id().as_ptr().cast(), 16, 9) };
        if window.is_null() {
            return Err("wl_egl_window_create failed".into());
        }
        // SAFETY: the display, config and window are the live ones made above.
        let surface = unsafe { (egl.create_window_surface)(egl.display, egl.config, window, std::ptr::null()) };
        if surface.is_null() {
            return Err("no EGL surface".into());
        }
        let _ = conn.flush();
        Ok(Self {
            conn,
            queue,
            state: State,
            child,
            sub,
            egl,
            window,
            surface,
            rect: (0, 0, 16, 9),
            scale: 1.,
            wanted: false,
            collapsed: false,
            shown: false,
            sized: (0, 0),
            dirty: true,
        })
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
        self.dirty = true;
        self.apply();
    }

    pub fn origin(&self) -> (i32, i32) {
        (self.rect.0, self.rect.1)
    }

    pub fn set_visible(&mut self, visible: bool) {
        if self.wanted != visible {
            self.wanted = visible;
            self.dirty = true;
            self.apply();
        }
    }

    /// Bring the subsurface to what was asked: its place and size, shown or hidden.
    fn apply(&mut self) {
        let _ = self.queue.dispatch_pending(&mut self.state);
        let show = self.wanted && !self.collapsed;
        if !show {
            if self.shown {
                // No buffer: nothing shows.
                self.child.attach(None, 0, 0);
                self.child.commit();
                self.shown = false;
                let _ = self.conn.flush();
            }
            return;
        }
        // The window's positions are logical; the buffer is `bs` times as big (whole numbers only).
        let scale = self.scale.max(1.);
        let bs = scale.ceil() as i32;
        let (lx, ly) = ((self.rect.0 as f32 / scale).round() as i32, (self.rect.1 as f32 / scale).round() as i32);
        let (lw, lh) = (((self.rect.2 as f32 / scale).round() as i32).max(1), ((self.rect.3 as f32 / scale).round() as i32).max(1));
        if std::env::var_os("UNBLOATED_WAYLAND_DEBUG").is_some() {
            eprintln!("wayland video: show at {lx},{ly} {lw}x{lh} buffer scale {bs} (device {:?}, scale {})", self.rect, self.scale);
        }
        self.sub.set_position(lx, ly);
        self.child.set_buffer_scale(bs);
        let size = (lw * bs, lh * bs);
        // SAFETY: the EGL objects are live; this runs on the thread that makes the context current.
        unsafe {
            if size != self.sized {
                (self.egl.window_resize)(self.window, size.0, size.1, 0, 0);
                self.sized = size;
            }
            (self.egl.make_current)(self.egl.display, self.surface, self.surface, self.egl.context);
            (self.egl.swap_interval)(self.egl.display, 0);
            // For now a plain colour: the video comes later.
            let clear_color: unsafe extern "C" fn(f32, f32, f32, f32) = std::mem::transmute(self.egl.proc("glClearColor"));
            let clear: unsafe extern "C" fn(u32) = std::mem::transmute(self.egl.proc("glClear"));
            let viewport: unsafe extern "C" fn(i32, i32, i32, i32) = std::mem::transmute(self.egl.proc("glViewport"));
            viewport(0, 0, size.0, size.1);
            clear_color(0.05, 0.55, 0.25, 1.);
            clear(0x4000);
            let ok_make = (self.egl.make_current)(self.egl.display, self.surface, self.surface, self.egl.context);
            let ok_swap = (self.egl.swap_buffers)(self.egl.display, self.surface);
            if std::env::var_os("UNBLOATED_WAYLAND_DEBUG").is_some() {
                eprintln!("wayland video: make_current {ok_make} swap {ok_swap}");
            }
        }
        self.shown = true;
        self.dirty = false;
        let _ = self.conn.flush();
    }
}

impl Drop for WaylandVideo {
    fn drop(&mut self) {
        // SAFETY: destroying what `create` made, in the order EGL needs.
        unsafe {
            (self.egl.make_current)(self.egl.display, std::ptr::null_mut(), std::ptr::null_mut(), std::ptr::null_mut());
            (self.egl.destroy_surface)(self.egl.display, self.surface);
            (self.egl.window_destroy)(self.window);
            (self.egl.destroy_context)(self.egl.display, self.egl.context);
        }
        self.sub.destroy();
        self.child.destroy();
        let _ = self.conn.flush();
    }
}
