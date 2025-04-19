use std::time::Duration;
use serde::{Deserialize, Serialize};

/// Configuration for WireGuard interface handling
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WireGuardConfig {
    /// Client timeout in seconds
    pub client_timeout: Duration,
    /// Write timeout in milliseconds (currently unused)
    pub write_timeout: Duration,
}

impl WireGuardConfig {
    /// Creates a new WireGuard configuration
    pub fn new(client_timeout_seconds: u64, write_timeout_ms: u64) -> Self {
        Self {
            client_timeout: Duration::from_secs(client_timeout_seconds),
            write_timeout: Duration::from_millis(write_timeout_ms),
        }
    }
} 