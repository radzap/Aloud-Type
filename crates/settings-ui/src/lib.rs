//! Settings window logic: Dictation key capture rules, the resident-process
//! pipe protocol and the rebinding session. The GUI shell is a later step.
pub mod capture;
pub mod ipc;
pub mod keys;
pub mod rules;
pub mod session;
pub mod codes;
pub mod items;
pub mod items_view;
pub mod items_win;
mod window;
pub use window::{run, HELP_LINE};
