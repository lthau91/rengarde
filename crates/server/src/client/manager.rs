use std::net::SocketAddr;
use std::sync::Arc;
use std::time::{Duration, Instant};

use dashmap::DashMap;
use tracing::{info, warn};

use crate::client::types::{Client, Clients};

/// Manages client connections and their lifecycle
#[derive(Clone)]
pub struct ClientManager {
    clients: Clients,
    timeout: Duration,
}

impl ClientManager {
    /// Creates a new client manager with the specified timeout
    pub fn new(timeout_seconds: u64) -> Self {
        Self {
            clients: Arc::new(DashMap::new()),
            timeout: Duration::from_secs(timeout_seconds),
        }
    }

    /// Returns a reference to the clients collection
    pub fn clients(&self) -> Clients {
        self.clients.clone()
    }

    /// Adds or updates a client with the given address
    pub fn add_or_update_client(&self, addr: SocketAddr, bytes_received: usize) {
        self.clients.entry(addr).and_modify(|client| {
            client.update(bytes_received);
        }).or_insert_with(|| {
            info!("New client connected: '{:?}'", addr);
            Client::new(addr)
        });
    }

    /// Removes a client by address
    pub fn remove_client(&self, addr: SocketAddr) {
        self.clients.remove(&addr);
        info!("Client removed: '{:?}'", addr);
    }

    /// Checks for and removes timed-out clients
    pub fn cleanup_timeout_clients(&self) {
        let now = Instant::now();
        let timeout_clients: Vec<SocketAddr> = self.clients
            .iter()
            .filter(|client| now.duration_since(client.last_received_at) > self.timeout)
            .map(|client| client.addr)
            .collect();

        for addr in timeout_clients {
            warn!("Client '{:?}' timed out", addr);
            self.remove_client(addr);
        }
    }

    /// Gets the number of connected clients
    pub fn client_count(&self) -> usize {
        self.clients.len()
    }
} 