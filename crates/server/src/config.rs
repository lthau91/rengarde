use anyhow::Result;
use serde::{Deserialize, Serialize};
use tracing::{info, warn};

use crate::wireguard::types::WireGuardConfig;

#[derive(Debug, Serialize, Deserialize)]
pub struct Settings {
    pub server: Server,
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Server {
    pub description: Option<String>,
    pub listen_addr: String,
    pub dst_addr: String,
    // Client timeout in seconds. If a client doesn't send any packet for n seconds, engarde stops sending it packets.
    // You will need to set it to a slightly higher value than the PersistentKeepalive option in WireGuard clients.
    pub client_timeout: Option<u64>,
    // Write timeout in milliseconds for socket writes. You can try to lower it if you're experiencing latency peaks, or raising it if the connection is unstable.
    // You can disable write timeout by setting to 0; but it's easy to have issues if you need low latency.
    pub write_timeout: Option<u64>,
    pub web_manager: Option<WebManager>,
    pub wireguard: Option<WireGuardConfig>,
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct WebManager {
    pub listen_addr: Option<String>,
    pub username: Option<String>,
    pub password: Option<String>,
}

pub fn load_config() -> Result<Settings> {
    let config_path = std::env::args().nth(1).unwrap_or_else(|| {
        String::from("engarde.yml")
    });

    let settings = std::fs::read_to_string(&config_path)?;
    let settings: Settings = serde_yaml::from_str(&settings)?;
    
    if let Some(description) = &settings.server.description {
        info!("{}", description);
    }

    Ok(settings)
}

pub fn validate_settings(mut settings: Settings) -> Result<Settings> {
    // Validate and set default client timeout
    if matches!(settings.server.client_timeout, None | Some(0)) {
        info!("Client timeout not set; setting to 30s.");
        settings.server.client_timeout = Some(30);
    }

    // Validate and set default write timeout
    if settings.server.write_timeout.is_none() {
        info!("Write timeout not set; setting to 10ms.");
        settings.server.write_timeout = Some(10);
    }

    // Warn if write timeout is enabled (not implemented yet)
    if !matches!(settings.server.write_timeout, Some(0)) {
        warn!("Write timeout is not implemented yet: setting to 0 to disable!");
        settings.server.write_timeout = Some(0);
    }

    Ok(settings)
} 