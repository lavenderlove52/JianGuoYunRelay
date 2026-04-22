//! 对外 WebDAV 子集（设计 §6）。

mod handlers;
mod propfind;

pub use handlers::dispatch_vault;
