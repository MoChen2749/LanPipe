//! LanPipe 网络层：设备发现与 LP/1 传输引擎。

pub mod discovery;
pub mod journal;
pub mod transfer;

pub use discovery::{start_discovery, DiscoveryEvent, DiscoveryHandle, PeerInfo};
pub use transfer::{send_paths, start_receiver, ReceiverHandle, TransferEvent};

/// 公告周期（毫秒）。
pub const ANNOUNCE_INTERVAL_MS: u64 = discovery::ANNOUNCE_INTERVAL_MS;
/// 对端超时（毫秒）。
pub const PEER_TIMEOUT_MS: u64 = discovery::PEER_TIMEOUT_MS;
