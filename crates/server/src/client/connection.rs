use std::sync::Arc;

use anyhow::Result;
use tokio::net::UdpSocket;
use tracing::{info, trace};

use crate::BUFFER_SIZE;
use crate::client::types::{Client, Clients};

/// Handles receiving data from clients and forwarding it to the WireGuard interface
#[tracing::instrument(skip_all)]
pub async fn receive_from_client(
    clients: Clients,
    client_socket: Arc<UdpSocket>,
    wireguard_socket: Arc<UdpSocket>,
    wireguard_addr: &str,
) -> Result<()> {
    let mut buf = [0; BUFFER_SIZE];
    loop {
        let (received_bytes, src_addr) = client_socket.recv_from(&mut buf).await?;

        trace!(
            received_bytes = received_bytes,
            src_addr = src_addr.to_string(),
            "Received {} bytes from client '{:?}'", received_bytes, src_addr
        );

        // Update client state
        clients.entry(src_addr).and_modify(|client| {
            client.update(received_bytes);
        }).or_insert_with(|| {
            info!("New client connected: '{:?}'", src_addr);
            Client::new(src_addr)
        });

        // Forward to WireGuard
        wireguard_socket.send_to(&buf[..received_bytes], wireguard_addr).await?;
        trace!(
            "\tSent {} bytes to wireguard on '{:?}'", received_bytes, wireguard_addr
        );
    }
} 