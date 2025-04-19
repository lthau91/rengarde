use anyhow::Result;
use network_interface::{NetworkInterface, NetworkInterfaceConfig};
use tracing::{info, warn};

mod types;
mod service;

use service::Service;
use types::Settings;

#[tokio::main]
async fn main() -> Result<()> {
    let _guard = shared::init()?;

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

    let config_path = std::env::args().nth(1).unwrap_or_else(|| {
        String::from("engarde.yml")
    });

    if config_path == "list-interfaces" {
        return list_interfaces();
    }

    let settings = std::fs::read_to_string(&config_path)?;
    let mut settings: Settings = serde_yaml::from_str(&settings)?;
    if let Some(description) = &settings.client.description {
        info!("{}", description);
    }

    if settings.client.write_timeout.is_none() {
        info!("Write timeout not set; setting to 10ms.");
        settings.client.write_timeout = Some(10);
    }
    if !matches!(settings.client.write_timeout, Some(0)) {
        warn!("Write timeout is not implemented yet: setting to 0 to disable!");
        settings.client.write_timeout = Some(0);
    }

    let service = Service::new(settings.client);
    service.run().await?;
    Ok(())
}

fn list_interfaces() -> Result<()> {
    let interfaces = NetworkInterface::show()?;
    for iface in interfaces {
        println!();
        println!("{}", iface.name);
        let if_addr = get_address_by_interface(&iface)
            .map(|ip| ip.to_string())
            .unwrap_or_default();
        println!("  Address: {}", if_addr);
    }
    Ok(())
}

fn get_address_by_interface(iface: &NetworkInterface) -> Option<std::net::IpAddr> {
    iface.addr.iter().find_map(|addr| {
        let ip = addr.ip();
        match ip {
            std::net::IpAddr::V4(v4) => {
                // Include private network IPs (192.168.x.x, 10.x.x.x, etc.)
                if v4.is_private() {
                    return Some(ip);
                }
                // Include loopback addresses (127.x.x.x)
                if v4.is_loopback() {
                    return Some(ip);
                }
                // Include link-local addresses (169.254.x.x)
                if v4.is_link_local() {
                    return Some(ip);
                }
                // Exclude multicast addresses
                if v4.is_multicast() {
                    return None;
                }
                Some(ip)
            }
            std::net::IpAddr::V6(_) => None,
        }
    })
}
