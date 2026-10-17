//! Everything that talks to YouTube, mpv, the cast receivers, the network or the disk, and has no GPUI in it.

pub(crate) mod account;
pub(crate) mod auth;
pub(crate) mod cast;
pub(crate) mod cli;
pub(crate) mod dearrow;
pub(crate) mod http;
pub(crate) mod links;
pub(crate) mod player;
pub(crate) mod prefetch;
pub(crate) mod proxy;
pub(crate) mod store;
pub(crate) mod storyboard;
pub(crate) mod thumbs;
pub(crate) mod yt;
