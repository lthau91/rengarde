use std::sync::Arc;

use anyhow::Result;
use futures::StreamExt;
use tokio::net::UdpSocket;
use tracing::{debug, trace, warn};

use crate::BUFFER_SIZE;
use crate::client::Clients;
use crate::wireguard::types::WireGuardConfig;

/// Handles receiving data from WireGuard interface and forwarding it to clients
#[tracing::instrument(skip_all)]
pub async fn receive_from_wireguard(
    clients: Clients,
    wireguard_socket: Arc<UdpSocket>,
    client_socket: Arc<UdpSocket>,
    client_timeout: u64,
    _write_timeout: u64,
) -> Result<()> {
    let config = WireGuardConfig::new(client_timeout, _write_timeout);
    let mut buf = [0; BUFFER_SIZE];

    loop {
        let received_bytes = wireguard_socket.recv(&mut buf).await?;
        let received_at = std::time::Instant::now();

        debug!("Received {} bytes from wireguard", received_bytes);

        // Send to clients
        let drop_list: Vec<_> = futures::stream::iter(clients.iter())
            .filter_map(|client| {
                let client_socket = client_socket.clone();
                async move {
                    // Check if the client has timed out
                    if received_at.duration_since(client.last_received_at) > config.client_timeout {
                        warn!("Client '{:?}' timed out", client.addr);
                        return Some(client.addr);
                    }

                    // Send to client
                    if client_socket.send_to(&buf[..received_bytes], &client.addr).await.is_err() {
                        warn!("Error writing to client '{:?}', terminating it", client.addr);
                        return Some(client.addr);
                    }

                    trace!(
                        sent_bytes = received_bytes,
                        dst_addr = client.addr.to_string(),
                        "\tSent {} bytes to client '{:?}'", received_bytes, client.addr
                    );
                    None
                }
            })
            .collect::<Vec<_>>()
            .await;

        // Drop the clients that have timed out
        if !drop_list.is_empty() {
            drop_list.into_iter().for_each(|addr| {
                clients.remove(&addr);
            });
        }
    }
} 