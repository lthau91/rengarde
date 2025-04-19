use std::net::SocketAddr;
use std::sync::Arc;
use std::time::Instant;

use dashmap::DashMap;

/// Represents a connected client with its state and statistics
#[derive(Debug)]
pub struct Client {
    /// The client's socket address
    pub addr: SocketAddr,
    /// Timestamp of the last received packet
    pub last_received_at: Instant,
    /// Total number of bytes received from this client
    pub total_received_bytes: usize,
}

impl Client {
    /// Creates a new client with the given address and current timestamp
    pub fn new(addr: SocketAddr) -> Self {
        Self {
            addr,
            last_received_at: Instant::now(),
            total_received_bytes: 0,
        }
    }

    /// Updates the client's last received timestamp and adds to total bytes
    pub fn update(&mut self, bytes_received: usize) {
        self.last_received_at = Instant::now();
        self.total_received_bytes += bytes_received;
    }
}

/// Thread-safe collection of connected clients
pub type Clients = Arc<DashMap<SocketAddr, Client>>; 