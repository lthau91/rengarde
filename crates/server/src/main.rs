use std::sync::Arc;
use std::time::Duration;

use anyhow::Result;
use tokio::net::UdpSocket;
use tracing::{info, warn};

mod config;
mod client;
mod wireguard;

use client::ClientManager;

// The maximum transmission unit (MTU) of an Ethernet frame is 1518 bytes with the normal untagged
// Ethernet frame overhead of 18 bytes and the 1500-byte payload.
const BUFFER_SIZE: usize = 1500;

#[tokio::main]
async fn main() -> Result<()> {
    // Initialize logging and print header
    let _guard = shared::init()?;
    print_header_info()?;

    // Load and validate configuration
    let settings = config::load_config()?;
    let settings = config::validate_settings(settings)?;

    // Initialize client manager and sockets
    let client_manager = ClientManager::new(settings.server.client_timeout.unwrap());
    let wireguard_socket = Arc::new(UdpSocket::bind("0.0.0.0:0").await?);
    let client_socket = Arc::new(UdpSocket::bind(&settings.server.listen_addr).await?);

    info!("Listening on: {}", &settings.server.listen_addr);

    // Start the web manager if configured
    if let Some(web_manager) = &settings.server.web_manager {
        warn!("Web manager is not implemented yet: {:?}", web_manager);
    }

    // Spawn the main processing tasks
    let join_receive_from_client = tokio::spawn({
        let client_manager = client_manager.clone();
        let client_socket = client_socket.clone();
        let wireguard_socket = wireguard_socket.clone();
        async move {
            if let Err(err) = client::receive_from_client(
                client_manager.clients(),
                client_socket,
                wireguard_socket,
                &settings.server.dst_addr,
            ).await {
                warn!("receive_from_client failed: {:?}", err);
            }
        }
    });

    let join_receive_from_wireguard = tokio::spawn({
        let client_manager = client_manager.clone();
        let wireguard_socket = wireguard_socket.clone();
        let client_socket = client_socket.clone();
        async move {
            if let Err(err) = wireguard::receive_from_wireguard(
                client_manager.clients(),
                wireguard_socket,
                client_socket,
                settings.server.client_timeout.unwrap(),
                settings.server.write_timeout.unwrap(),
            ).await {
                panic!("receive_from_wireguard thread failed: {:?}", err);
            }
        }
    });

    // Spawn client cleanup task
    let join_cleanup = tokio::spawn({
        let client_manager = client_manager.clone();
        async move {
            loop {
                tokio::time::sleep(Duration::from_secs(5)).await;
                client_manager.cleanup_timeout_clients();
            }
        }
    });

    // Wait for tasks to complete
    join_receive_from_client.await.unwrap();
    join_receive_from_wireguard.await.unwrap();
    join_cleanup.await.unwrap();
    warn!("All threads joined; exiting...");

    Ok(())
}

fn print_header_info() -> Result<()> {
    let rengarde_official_build = option_env!("RENGARDE_OFFICIAL_BUILD").unwrap_or("false").parse::<bool>()?;
    let cargo_pkg_name = env!("CARGO_PKG_NAME");
    let cargo_pkg_version = env!("CARGO_PKG_VERSION");
    let vergen_git_describe = env!("VERGEN_GIT_DESCRIBE");
    let vergen_git_dirty = env!("VERGEN_GIT_DIRTY");
    let vergen_build_timestamp = env!("VERGEN_BUILD_TIMESTAMP");
    let vergen_cargo_target_triple = env!("VERGEN_CARGO_TARGET_TRIPLE");
    let rust_runtime = if cfg!(feature = "rt-rayon") {
        "rayon"
    } else if cfg!(feature = "rt-tokio") {
        "tokio"
    } else {
        unimplemented!("No runtime feature enabled");
    };

    shared::print_header(
        rengarde_official_build,
        cargo_pkg_name,
        cargo_pkg_version,
        vergen_git_describe,
        vergen_git_dirty,
        vergen_build_timestamp,
        vergen_cargo_target_triple,
        rust_runtime,
    );

    Ok(())
}
