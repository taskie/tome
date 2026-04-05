//! Shared output format for CLI commands.

use clap::ValueEnum;

/// Output format for structured CLI output.
#[derive(Debug, Clone, Copy, Default, ValueEnum)]
pub enum OutputFormat {
    /// Human-readable text (default)
    #[default]
    Text,
    /// Machine-readable JSON (one object per top-level output)
    Json,
}
