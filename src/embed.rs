//! X11 child window inside unbloated-youtube's own window that mpv renders into (`mpv --wid`).
//! gpui doesn't expose its X11 window id, so we find it via the WM's _NET_CLIENT_LIST + _NET_WM_PID.

use x11rb::connection::Connection;
use x11rb::protocol::xproto::{
    AtomEnum, ConfigureWindowAux, ConnectionExt, CreateWindowAux, EventMask, WindowClass,
};
use x11rb::rust_connection::RustConnection;

pub struct Embed {
    conn: RustConnection,
    child: u32,
    mapped: bool,
    rect: (i32, i32, u32, u32),
}

impl Embed {
    /// None on Wayland or if our top-level window can't be found yet.
    pub fn new() -> Option<Self> {
        let (conn, screen) = x11rb::connect(None).ok()?;
        let scr = &conn.setup().roots[screen];
        let (root, depth, visual, colormap) = (scr.root, scr.root_depth, scr.root_visual, scr.default_colormap);
        let parent = find_own_window(&conn, root)?;
        let child = conn.generate_id().ok()?;
        conn.create_window(
            // gpui's window is 32-bit ARGB; mpv draws black into such a child, so use the plain screen visual.
            depth,
            child,
            parent,
            0,
            0,
            16,
            9,
            0,
            WindowClass::INPUT_OUTPUT,
            visual,
            &CreateWindowAux::new().background_pixel(0).border_pixel(0).colormap(colormap).event_mask(EventMask::NO_EVENT),
        )
        .ok()?
        .check()
        .map_err(|e| eprintln!("unbloated-youtube: creating video window failed: {e}"))
        .ok()?;
        Some(Self { conn, child, mapped: false, rect: (0, 0, 16, 9) })
    }

    pub fn id(&self) -> u32 {
        self.child
    }

    /// Position in device pixels relative to unbloated-youtube's window.
    pub fn place(&mut self, x: i32, y: i32, w: u32, h: u32) {
        if self.rect == (x, y, w, h) || w == 0 || h == 0 {
            return;
        }
        self.rect = (x, y, w, h);
        let aux = ConfigureWindowAux::new().x(x).y(y).width(w).height(h);
        let _ = self.conn.configure_window(self.child, &aux);
        let _ = self.conn.flush();
    }

    pub fn set_visible(&mut self, visible: bool) {
        if self.mapped == visible {
            return;
        }
        self.mapped = visible;
        let _ = if visible { self.conn.map_window(self.child) } else { self.conn.unmap_window(self.child) };
        let _ = self.conn.flush();
    }
}

fn find_own_window(conn: &RustConnection, root: u32) -> Option<u32> {
    let atom = |name: &[u8]| Some(conn.intern_atom(false, name).ok()?.reply().ok()?.atom);
    let (client_list, pid_atom) = (atom(b"_NET_CLIENT_LIST")?, atom(b"_NET_WM_PID")?);
    let clients = conn.get_property(false, root, client_list, AtomEnum::WINDOW, 0, 4096).ok()?.reply().ok()?;
    let pid = std::process::id();
    clients.value32()?.find(|&w| {
        conn.get_property(false, w, pid_atom, AtomEnum::CARDINAL, 0, 1)
            .ok()
            .and_then(|c| c.reply().ok())
            .and_then(|r| r.value32()?.next())
            == Some(pid)
    })
}
