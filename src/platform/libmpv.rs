//! mpv as a library (`libmpv`), loaded at run time: the player the app runs inside its own process.
//! Under Wayland it draws the video itself (see `embed_wayland.rs`), under X11 mpv draws into the
//! window it is given (`Inline`). Only what that needs is declared; the rest of the app still talks
//! to mpv over its IPC socket, as with the mpv program.

use libloading::Library;
use std::ffi::{CStr, CString, c_char, c_int, c_void};

pub const RENDER_PARAM_API_TYPE: c_int = 1;
pub const RENDER_PARAM_OPENGL_INIT_PARAMS: c_int = 2;
pub const RENDER_PARAM_OPENGL_FBO: c_int = 3;
pub const RENDER_PARAM_FLIP_Y: c_int = 4;
pub const RENDER_PARAM_WL_DISPLAY: c_int = 9;
/// `mpv_render_context_update` says a new frame is due.
pub const RENDER_UPDATE_FRAME: u64 = 1;

#[repr(C)]
pub struct RenderParam {
    pub kind: c_int,
    pub data: *mut c_void,
}

#[repr(C)]
pub struct OpenglInitParams {
    pub get_proc_address: unsafe extern "C" fn(*mut c_void, *const c_char) -> *mut c_void,
    pub ctx: *mut c_void,
}

#[repr(C)]
pub struct OpenglFbo {
    pub fbo: c_int,
    pub w: c_int,
    pub h: c_int,
    pub internal_format: c_int,
}

pub struct LibMpv {
    _lib: Library,
    create: unsafe extern "C" fn() -> *mut c_void,
    set_option_string: unsafe extern "C" fn(*mut c_void, *const c_char, *const c_char) -> c_int,
    initialize: unsafe extern "C" fn(*mut c_void) -> c_int,
    terminate_destroy: unsafe extern "C" fn(*mut c_void),
    command: unsafe extern "C" fn(*mut c_void, *mut *const c_char) -> c_int,
    error_string: unsafe extern "C" fn(c_int) -> *const c_char,
    render_create: unsafe extern "C" fn(*mut *mut c_void, *mut c_void, *mut RenderParam) -> c_int,
    render_free: unsafe extern "C" fn(*mut c_void),
    render_set_update_callback: unsafe extern "C" fn(*mut c_void, Option<unsafe extern "C" fn(*mut c_void)>, *mut c_void),
    render_update: unsafe extern "C" fn(*mut c_void) -> u64,
    render: unsafe extern "C" fn(*mut c_void, *mut RenderParam) -> c_int,
    render_report_swap: unsafe extern "C" fn(*mut c_void),
}

impl LibMpv {
    /// `UNBLOATED_LIBMPV` (a path: the Nix package sets it), else the system's `libmpv.so.2`.
    pub fn load() -> Result<Self, String> {
        let mut names: Vec<String> = std::env::var("UNBLOATED_LIBMPV").ok().into_iter().collect();
        names.extend(["libmpv.so.2".to_string(), "libmpv.so.1".to_string()]);
        let mut tried = Vec::new();
        for name in names {
            // SAFETY: loading a shared library runs its initialisers, as any dynamic linking does.
            match unsafe { Library::new(&name) } {
                Ok(lib) => return Self::bind(lib),
                Err(e) => tried.push(e.to_string()),
            }
        }
        Err(format!("libmpv not found ({})", tried.join("; ")))
    }

    fn bind(lib: Library) -> Result<Self, String> {
        // SAFETY: the signatures are those of mpv's client.h and render.h; the pointers stay valid
        // as long as `_lib` is held, which is as long as this struct.
        unsafe {
            macro_rules! sym {
                ($name:literal) => {
                    *lib.get(concat!($name, "\0").as_bytes()).map_err(|e| format!("{}: {e}", $name))?
                };
            }
            Ok(Self {
                create: sym!("mpv_create"),
                set_option_string: sym!("mpv_set_option_string"),
                initialize: sym!("mpv_initialize"),
                terminate_destroy: sym!("mpv_terminate_destroy"),
                command: sym!("mpv_command"),
                error_string: sym!("mpv_error_string"),
                render_create: sym!("mpv_render_context_create"),
                render_free: sym!("mpv_render_context_free"),
                render_set_update_callback: sym!("mpv_render_context_set_update_callback"),
                render_update: sym!("mpv_render_context_update"),
                render: sym!("mpv_render_context_render"),
                render_report_swap: sym!("mpv_render_context_report_swap"),
                _lib: lib,
            })
        }
    }

    pub fn error(&self, code: c_int) -> String {
        // SAFETY: mpv_error_string returns a static NUL-terminated string.
        unsafe { CStr::from_ptr((self.error_string)(code)).to_string_lossy().into_owned() }
    }

    /// A player that isn't started yet: set its options, then `initialize`.
    pub fn create(&self) -> Result<*mut c_void, String> {
        // SAFETY: no arguments; the handle is freed by `terminate_destroy`.
        let handle = unsafe { (self.create)() };
        if handle.is_null() { Err("mpv_create failed".into()) } else { Ok(handle) }
    }

    pub fn set_option(&self, handle: *mut c_void, name: &str, value: &str) -> Result<(), String> {
        let (n, v) = (CString::new(name).map_err(|e| e.to_string())?, CString::new(value).map_err(|e| e.to_string())?);
        // SAFETY: `handle` is live; both strings are NUL-terminated and outlive the call.
        let code = unsafe { (self.set_option_string)(handle, n.as_ptr(), v.as_ptr()) };
        if code < 0 { Err(format!("{name}={value}: {}", self.error(code))) } else { Ok(()) }
    }

    pub fn initialize(&self, handle: *mut c_void) -> Result<(), String> {
        // SAFETY: `handle` is live and not initialized yet.
        let code = unsafe { (self.initialize)(handle) };
        if code < 0 { Err(format!("mpv_initialize: {}", self.error(code))) } else { Ok(()) }
    }

    /// Run a command ("loadfile", url, ...).
    pub fn command(&self, handle: *mut c_void, args: &[&str]) -> Result<(), String> {
        let owned: Vec<CString> = args.iter().map(|a| CString::new(*a).unwrap_or_default()).collect();
        let mut ptrs: Vec<*const c_char> = owned.iter().map(|c| c.as_ptr()).collect();
        ptrs.push(std::ptr::null());
        // SAFETY: a NULL-terminated array of NUL-terminated strings, alive for the call.
        let code = unsafe { (self.command)(handle, ptrs.as_mut_ptr()) };
        if code < 0 { Err(format!("{}: {}", args.first().unwrap_or(&""), self.error(code))) } else { Ok(()) }
    }

    /// Stop the player and free it; waits for it to finish. The render context must be gone.
    pub fn terminate_destroy(&self, handle: *mut c_void) {
        // SAFETY: `handle` is live and not used again.
        unsafe { (self.terminate_destroy)(handle) }
    }

    /// A render context drawing with OpenGL into whatever framebuffer is current. Call it with
    /// the GL context current. `wl_display` lets mpv use the hardware decoder's Wayland interop.
    pub fn render_context(
        &self,
        handle: *mut c_void,
        get_proc_address: unsafe extern "C" fn(*mut c_void, *const c_char) -> *mut c_void,
        proc_ctx: *mut c_void,
        wl_display: *mut c_void,
    ) -> Result<*mut c_void, String> {
        let mut gl = OpenglInitParams { get_proc_address, ctx: proc_ctx };
        let api = CString::new("opengl").unwrap();
        let mut params = [
            RenderParam { kind: RENDER_PARAM_API_TYPE, data: api.as_ptr() as *mut c_void },
            RenderParam { kind: RENDER_PARAM_OPENGL_INIT_PARAMS, data: (&mut gl as *mut OpenglInitParams).cast() },
            RenderParam { kind: RENDER_PARAM_WL_DISPLAY, data: wl_display },
            RenderParam { kind: 0, data: std::ptr::null_mut() },
        ];
        let mut ctx = std::ptr::null_mut();
        // SAFETY: the parameter array is terminated by an INVALID entry and, with `gl` and `api`,
        // lives through the call; libmpv copies what it needs.
        let code = unsafe { (self.render_create)(&mut ctx, handle, params.as_mut_ptr()) };
        if code < 0 { Err(format!("mpv_render_context_create: {}", self.error(code))) } else { Ok(ctx) }
    }

    /// `callback(data)` is called, from any thread, when `render_update` has something to say.
    pub fn render_set_update_callback(&self, ctx: *mut c_void, callback: unsafe extern "C" fn(*mut c_void), data: *mut c_void) {
        // SAFETY: `ctx` is live; the callback only wakes the render thread.
        unsafe { (self.render_set_update_callback)(ctx, Some(callback), data) }
    }

    pub fn render_update(&self, ctx: *mut c_void) -> u64 {
        // SAFETY: `ctx` is live; called from the render thread.
        unsafe { (self.render_update)(ctx) }
    }

    /// Draw the current frame into the default framebuffer of `w` x `h` pixels (upside down, as
    /// that framebuffer is).
    pub fn render(&self, ctx: *mut c_void, w: i32, h: i32) -> Result<(), String> {
        let mut fbo = OpenglFbo { fbo: 0, w, h, internal_format: 0 };
        let mut flip: c_int = 1;
        let mut params = [
            RenderParam { kind: RENDER_PARAM_OPENGL_FBO, data: (&mut fbo as *mut OpenglFbo).cast() },
            RenderParam { kind: RENDER_PARAM_FLIP_Y, data: (&mut flip as *mut c_int).cast() },
            RenderParam { kind: 0, data: std::ptr::null_mut() },
        ];
        // SAFETY: as in `render_context`; the GL context is current on this thread.
        let code = unsafe { (self.render)(ctx, params.as_mut_ptr()) };
        if code < 0 { Err(format!("mpv_render_context_render: {}", self.error(code))) } else { Ok(()) }
    }

    pub fn render_report_swap(&self, ctx: *mut c_void) {
        // SAFETY: `ctx` is live.
        unsafe { (self.render_report_swap)(ctx) }
    }

    pub fn render_free(&self, ctx: *mut c_void) {
        // SAFETY: `ctx` is live and not used again; the GL context is current.
        unsafe { (self.render_free)(ctx) }
    }
}

/// mpv in this process drawing into a window of its own (`wid`, X11): no render context and no
/// thread, libmpv does the drawing. Dropping it stops mpv and waits for it to finish.
pub struct Inline {
    lib: LibMpv,
    handle: *mut c_void,
}

// SAFETY: libmpv's client API may be used from any thread; `handle` is only used through `lib`.
unsafe impl Send for Inline {}

impl Inline {
    /// Start mpv with `options` (name, value) and load `url`.
    pub fn start(options: Vec<(String, String)>, url: &str) -> Result<Self, String> {
        let lib = LibMpv::load()?;
        let handle = lib.create()?;
        // From here on, dropping `inline` destroys the player.
        let inline = Self { lib, handle };
        for (name, value) in &options {
            if let Err(e) = inline.lib.set_option(handle, name, value) {
                eprintln!("unbloatedtube: mpv option {e}");
            }
        }
        inline.lib.initialize(handle)?;
        inline.lib.command(handle, &["loadfile", url])?;
        Ok(inline)
    }
}

impl Drop for Inline {
    fn drop(&mut self) {
        self.lib.terminate_destroy(self.handle);
    }
}
