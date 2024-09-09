use std::sync::OnceLock;

use axum::Router;

pub mod checkbox;
pub mod multipaint_by_numbers;

/// A lazily-created Router, to be used by the SSH client tunnels or directly by the HTTP server.
pub static ROUTER: OnceLock<Router> = OnceLock::new();
