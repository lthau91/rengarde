use std::net::SocketAddr;
use std::sync::{Arc, Mutex};

use anyhow::{anyhow, Result};
use dashmap::DashMap;
use futures::StreamExt;
use network_interface::{NetworkInterface, NetworkInterfaceConfig};
use tokio::net::UdpSocket;
use tokio::select;
use tokio::time::sleep;
use tokio_util::sync::CancellationToken;
use tracing::{debug, info, info_span, Instrument, trace, warn};

use crate::types::{ClientSettings, SendingRoutine};

// The maximum transmission unit (MTU) of an Ethernet frame is 1518 bytes with the normal untagged
// Ethernet frame overhead of 18 bytes and the 1500-byte payload.
const BUFFER_SIZE: usize = 1500;

type SendingRoutines = Arc<DashMap<String, SendingRoutine>>;

#[derive(Clone)]
pub struct Service {
    shutdown: CancellationToken,
    settings: ClientSettings,
    routines: SendingRoutines,
    source_addr: Arc<Mutex<SocketAddr>>,
}

impl Service {
    pub fn new(settings: ClientSettings) -> Self {
        Self {
            shutdown: CancellationToken::new(),
            settings,
            routines: Arc::new(DashMap::new()),
            source_addr: Arc::new(Mutex::new(
                SocketAddr::new(std::net::IpAddr::V4(std::net::Ipv4Addr::new(0, 0, 0, 0)), 0)
            )),
        }
    }

    pub async fn run(&self) -> Result<()> {
        let settings = &self.settings;
        let wireguard_socket = Arc::new(UdpSocket::bind(&settings.listen_addr).await?);

        info!("Listening on: {}", &settings.listen_addr);

        if let Some(web_manager) = &settings.web_manager {
            warn!("Web manager is not implemented yet: {:?}", web_manager);
        }

        let join_update_available_interfaces = tokio::spawn({
            let service = self.clone();
            let wireguard_socket = wireguard_socket.clone();
            async move {
                if let Err(err) = service.update_available_interfaces(wireguard_socket).await {
                    warn!("update_available_interfaces thread failed: {:?}", err);
                }
            }
        });

        let join_receive_from_wireguard = tokio::spawn({
            let service = self.clone();
            async move {
                if let Err(err) = service.receive_from_wireguard(wireguard_socket).await {
                    warn!("receive_from_wireguard thread failed: {:?}", err);
                }
            }
        });

        select! {
            _ = tokio::signal::ctrl_c() => {
                info!("ctrl + c received; shutting down...");
            }
            _ = join_update_available_interfaces => {
                warn!("update_available_interfaces thread closed");
            }
            _ = join_receive_from_wireguard => {
                warn!("receive_from_wireguard thread closed");
            }
        }

        debug!("shutdown starting; sending cancel");
        self.shutdown.cancel();
        debug!("shutdown finished; cancel returned");
        Ok(())
    }

    async fn update_available_interfaces(&self, wireguard_socket: Arc<UdpSocket>) -> Result<()> {
        loop {
            debug!("Checking available interfaces...");
            let interfaces = NetworkInterface::show()?;

            let drop_list: Vec<_> = self.routines.iter().filter_map(|routine| {
                if self.settings.excluded_interfaces.contains(routine.key()) {
                    warn!("Interface '{}' is excluded; removing it", routine.key());
                    return Some(routine.key().clone());
                }
                match interfaces.iter().find(|interface| &interface.name == routine.key()) {
                    Some(iface) => {
                        match get_address_by_interface(iface) {
                            Some(addr) => {
                                if addr != routine.value().src_addr.ip() {
                                    info!("Interface '{}' address changed; re-creating it", routine.key());
                                    Some(routine.key().clone())
                                } else {
                                    None
                                }
                            }
                            None => {
                                warn!("Interface '{}' has no address; removing it", routine.key());
                                Some(routine.key().clone())
                            }
                        }
                    }
                    None => {
                        warn!("Interface '{}' no longer exists; removing it", routine.key());
                        Some(routine.key().clone())
                    }
                }
            }).collect();

            if !drop_list.is_empty() {
                drop_list.into_iter().for_each(|key| {
                    self.routines.remove(&key);
                });
            }

            for iface in interfaces {
                if self.settings.excluded_interfaces.contains(&iface.name) {
                    continue;
                }
                if self.routines.contains_key(&iface.name) {
                    continue;
                }

                if let Some(source_addr) = get_address_by_interface(&iface) {
                    if let Err(err) = self.create_send_thread(&iface, source_addr, wireguard_socket.clone()).await {
                        warn!("Failed to create send thread for interface '{}': {:?}", iface.name, err);
                    }
                    debug!("Created send thread for interface '{}'", iface.name);
                }
            }

            debug!("Checking available interfaces finished; sleeping...");
            select! {
                _ = self.shutdown.cancelled() => {
                    debug!("Shutdown signal received; closing update_available_interfaces thread");
                    return Ok(());
                }
                _ = sleep(std::time::Duration::from_secs(1)) => {}
            }
        }
    }

    async fn create_send_thread(&self, iface: &NetworkInterface, source_addr: std::net::IpAddr, wireguard_socket: Arc<UdpSocket>) -> Result<()> {
        info!("New interface '{}' with IP '{}', adding it", iface.name, source_addr);

        let dst_addr = tokio::net::lookup_host(&self.settings.dst_addr)
            .await
            .map_err(|err| anyhow!("Failed to resolve destination address '{}': {:?}", self.settings.dst_addr, err))
            .and_then(|mut addrs| {
                addrs.next().ok_or_else(|| anyhow!("No address found for destination address '{}'", self.settings.dst_addr))
            })?;
        debug!("\tDestination address: '{:?}'", dst_addr);

        let src_addr = SocketAddr::new(source_addr, 0);
        debug!("\tSource address: '{:?}'", src_addr);

        let src_socket = UdpSocket::bind(src_addr).await?;
        debug!("\tBound udp socket to '{}'", src_addr);

        if !iface.name.is_empty() {
            src_socket.bind_device(Some(iface.name.as_bytes()))?;
            debug!("\tBound udp socket to interface '{}'", iface.name);
        }

        let src_socket = Arc::new(src_socket);

        let routine = SendingRoutine::new(
            iface.name.to_owned(),
            src_socket,
            src_addr,
            dst_addr,
        );

        if let Some(routine) = self.routines.insert(iface.name.to_owned(), routine) {
            panic!("Interface '{}' already existed when we tried to add it", routine.ifname);
        };

        tokio::spawn({
            let this = self.clone();
            let ifname = iface.name.to_owned();
            let wireguard_socket = wireguard_socket.clone();
            async move {
                if let Err(err) = this.wireguard_write_back(ifname.clone(), wireguard_socket).await {
                    warn!("wireguard_write_back thread failed: {:?}", err);
                };
                debug!("wireguard_write_back thread closed: '{}'", ifname);
            }
        });
        debug!("\tStarted wireguard_write_back thread for interface '{}'", iface.name);

        Ok(())
    }

    async fn wireguard_write_back(&self, ifname: String, wireguard_socket: Arc<UdpSocket>) -> Result<()> {
        let mut buf = [0; BUFFER_SIZE];
        loop {
            let routine = self.routines.get(&ifname).ok_or_else(|| anyhow!("Interface '{}' not found", ifname))?;
            debug!("Got interface {} from routines", ifname);
            if routine.is_closing {
                warn!("Interface '{}' is closing; closing thread", ifname);
                return Ok(());
            }
            let socket = routine.src_socket.clone();
            drop(routine);

            debug!("Waiting for data from interface '{}'", ifname);
            select! {
                t = socket.recv_from(&mut buf) => {
                    match t {
                        Ok((received_bytes, _)) => {
                            debug!("Received {} bytes from interface '{}'", received_bytes, ifname);
                            let mut routine = self.routines.get_mut(&ifname).ok_or_else(|| anyhow!("Interface '{}' not found", ifname))?;
                            routine.last_received_at = std::time::Instant::now();
                            routine.total_received_bytes += received_bytes;
                            drop(routine);

                            let wg_addr = *self.source_addr.lock().unwrap();
                            wireguard_socket.send_to(&buf[..received_bytes], wg_addr).await?;
                            trace!("\tSent {} bytes to wireguard", received_bytes);
                        }
                        Err(err) => {
                            warn!("Error receiving from interface '{}': {:?}", ifname, err);
                            let mut routine = self.routines.get_mut(&ifname).ok_or_else(|| anyhow!("Interface '{}' not found", ifname))?;
                            routine.is_closing = true;
                        }
                    }
                }
                _ = self.shutdown.cancelled() => {
                    debug!("Shutdown signal received; closing thread");
                    return Ok(());
                }
            }
        }
    }

    async fn receive_from_wireguard(&self, wireguard_socket: Arc<UdpSocket>) -> Result<()> {
        let mut buf = [0; BUFFER_SIZE];
        loop {
            let span = info_span!("receive_from_wireguard_loop");
            select! {
                _ = self.shutdown.cancelled() => {
                    debug!("Shutdown signal received; closing thread");
                    return Ok(());
                }
                result = wireguard_socket.recv_from(&mut buf).instrument(span) => {
                    match result {
                        Ok((received_bytes, src_addr)) => {
                            *self.source_addr.lock().unwrap() = src_addr;
                            trace!(
                                received_bytes = received_bytes,
                                src_addr = src_addr.to_string(),
                                "Received {} bytes from wireguard on '{:?}'", received_bytes, src_addr
                            );
                            trace!("\tSending to {} clients", self.routines.len());

                            let drop_list = futures::stream::iter(self.routines.iter_mut())
                                .filter_map(|mut routine| async move {
                                    routine.send_to(&buf[..received_bytes]).await
                                })
                                .collect::<Vec<String>>()
                                .await;

                            if !drop_list.is_empty() {
                                drop_list.into_iter().for_each(|ifname| {
                                    self.routines.remove(&ifname);
                                });
                            }

                            trace!("Sent to {} clients", self.routines.len());
                        }
                        Err(err) => {
                            warn!("Error receiving from wireguard: {:?}", err);
                        }
                    }
                }
            }
        }
    }
}

fn get_address_by_interface(iface: &NetworkInterface) -> Option<std::net::IpAddr> {
    iface.addr.iter().find_map(|addr| {
        let ip = addr.ip();
        match ip {
            std::net::IpAddr::V4(v4) => {
                if v4.is_private() {
                    return Some(ip);
                }
                if v4.is_loopback() {
                    return Some(ip);
                }
                if v4.is_link_local() {
                    return Some(ip);
                }
                if v4.is_multicast() {
                    return None;
                }
                Some(ip)
            }
            std::net::IpAddr::V6(_) => None,
        }
    })
} 