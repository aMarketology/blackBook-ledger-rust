// Routes module - organizes all HTTP endpoints
// Each sub-module handles a specific domain

pub mod auth;

// Re-export route handlers for convenience
pub use auth::*;
