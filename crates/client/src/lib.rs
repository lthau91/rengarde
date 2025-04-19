// The maximum transmission unit (MTU) of an Ethernet frame is 1518 bytes with the normal untagged
// Ethernet frame overhead of 18 bytes and the 1500-byte payload.
pub const BUFFER_SIZE: usize = 1500;

pub mod types;
pub mod service;

pub use types::{Settings, ClientSettings, WebManager};
pub use service::Service; 