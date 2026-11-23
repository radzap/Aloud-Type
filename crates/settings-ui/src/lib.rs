//! Settings window logic: Dictation key capture rules, the resident-process
//! pipe protocol and the rebinding session. The GUI shell is a later step.
pub mod background;
pub mod capture;
pub mod codes;
pub mod download;
pub mod ipc;
pub mod items;
pub mod items_view;
pub mod items_win;
pub mod keys;
pub mod manager;
pub mod rules;
pub mod session;
pub mod update;
mod window;
pub use window::{run, HELP_LINE};
