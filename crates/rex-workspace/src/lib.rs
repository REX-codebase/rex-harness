//! The folders the desktop app's Terminal, Editor and Git tabs work in, and
//! the operations they need on them.
//!
//! Three parts, none of which depends on the UI toolkit so each can be tested
//! on its own:
//!
//! - [`registry`]: which folders the person has opened. Only a folder chosen
//!   through the native folder dialog can be added, so the webview cannot
//!   point a terminal, an editor or git at a path of its own choosing.
//! - [`git`]: status, diff, stage, commit and init through the person's own
//!   `git`, with their own configuration. This is the person's action on their
//!   own repository, not a model tool call, so it does not run under the tool
//!   sandbox (which replaces `HOME` and would hide their commit identity).
//! - [`files`]: listing, reading and writing text files inside a folder, with
//!   symlink-escape checks, size limits and atomic writes.

pub mod files;
pub mod git;
pub mod registry;

pub use files::{FileListing, TextFile};
pub use git::{CommitInfo, CommitSummary, Git, GitChange, GitOverview};
pub use registry::{display_path, WorkspaceInfo, WorkspaceRegistry, WorkspaceSnapshot};
