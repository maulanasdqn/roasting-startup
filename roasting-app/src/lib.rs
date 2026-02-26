pub mod domain;

#[cfg(feature = "ssr")]
pub mod application;

#[cfg(feature = "ssr")]
pub mod infrastructure;

#[cfg(feature = "ssr")]
mod app_context;

#[cfg(feature = "ssr")]
pub use app_context::AppContext;
