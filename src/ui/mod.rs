//! Interface pieces that don't depend on the app's state: theme, tables, widgets, hint mode, text editing.

pub(crate) mod icons;
pub(crate) mod textsel;
mod components;
mod tables;
mod text_edit;
mod theme;

pub(crate) use components::*;
pub(crate) use tables::*;
pub(crate) use text_edit::*;
pub(crate) use theme::*;
