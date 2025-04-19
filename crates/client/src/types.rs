use std::net::SocketAddr;
use std::time::Instant;

use serde::{Deserialize, Serialize};
use tracing::{debug, info, trace, warn};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Settings {
    pub client: ClientSettings,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ClientSettings {
    pub description: Option<String>,
    pub listen_addr: String,
    pub dst_addr: String,
    pub write_timeout: Option<u64>,
    pub excluded_interfaces: Vec<String>,
    pub web_manager: Option<WebManager>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct WebManager {
    pub listen_addr: Option<String>,
    pub username: Option<String>,
    pub password: Option<String>,
}

pub struct SendingRoutine {
    pub ifname: String,
    pub src_socket: std::sync::Arc<tokio::net::UdpSocket>,
    pub src_addr: SocketAddr,
    pub dst_addr: SocketAddr,
    pub last_received_at: Instant,
    pub total_received_bytes: usize,
    pub is_closing: bool,
}

impl SendingRoutine {
    pub fn new(ifname: String, src_socket: std::sync::Arc<tokio::net::UdpSocket>, src_addr: SocketAddr, dst_addr: SocketAddr) -> Self {
        info!(
            event = "added",
            iface_name = ifname,
            src_addr = src_addr.to_string(),
            dst_addr = dst_addr.to_string(),
            "\tAdded interface '{}' to sending routines", ifname
        );
        Self {
            ifname,
            src_socket,
            src_addr,
            dst_addr,
            last_received_at: Instant::now(),
            total_received_bytes: 0,
            is_closing: false,
        }
    }

    pub async fn send_to(&mut self, buf: &[u8]) -> Option<String> {
        match self.src_socket.send_to(buf, self.dst_addr).await {
            Ok(sent_bytes) => {
                trace!(
                    sent_bytes = sent_bytes,
                    dst_ifname = self.ifname,
                    dst_addr = self.dst_addr.to_string(),
                    "\tSent {} bytes on iface {} to client '{:?}'", sent_bytes, self.ifname, self.dst_addr
                );
                None
            }
            Err(err) => {
                warn!(
                    event = "disconnect",
                    dst_addr = self.dst_addr.to_string(),
                    "Error writing to client '{:?}', terminating it: {:?}", self.dst_addr, err
                );
                Some(self.ifname.clone())
            }
        }
    }
}

impl Drop for SendingRoutine {
    fn drop(&mut self) {
        debug!(
            event = "removed",
            iface_name = self.ifname,
            src_addr = self.src_addr.to_string(),
            dst_addr = self.dst_addr.to_string(),
            "\tRemoved interface '{}' from sending routines", self.ifname
        );
    }
} 