// Application workflow.

// This layer should know:

// which entry is currently open
// original vs current content
// what happens when the user selects, edits, saves, reverts

pub mod app_error;
pub mod cache_warmup;
pub mod changes;
pub mod controller;
pub mod group_preview;
pub mod state;

pub use controller::AppController;
