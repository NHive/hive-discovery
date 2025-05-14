use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet, HashMap};
use std::net::IpAddr;
use std::net::SocketAddr;

/// Represents various events that occur during the service discovery process.
#[derive(Clone, Debug)]
pub enum DiscoveryEvent {
    /// A new service has been found, or an existing service's information has been updated.
    /// Contains the details of the discovered or updated service.
    ServiceFound(DiscoveryServiceDetails),
    /// A service has gone offline or become unavailable.
    /// The `String` parameter is the full instance name of the lost service.
    ServiceLost(String),
    /// The service discovery process has successfully started.
    DiscoveryStarted,
    /// The service discovery process has stopped.
    DiscoveryStopped,
}

/// Represents the current operational status of the service discovery component.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum DiscoveryServiceStatus {
    /// The service is stopped and not performing any discovery or registration.
    Stopped,
    /// The service is actively running, discovering and/or registering services.
    Running,
    /// The service is in the process of stopping.
    Stopping,
}

/// Detailed information about a discovered remote service.
///
/// This structure contains metadata obtained from network discovery,
/// such as mDNS/DNS-SD. It's designed to be serializable and hashable.
#[derive(Clone, Debug, Serialize, Deserialize, Hash, PartialEq, Eq)]
pub struct DiscoveryServiceDetails {
    /// The type of the service (e.g., `"_http._tcp.local."`), specifying the protocol and domain.
    pub service_type: String,
    /// The unique instance name of the service (e.g., `"My Web Server._http._tcp.local."`).
    /// This distinguishes different instances of the same service type.
    pub instance_name: String,
    /// The domain name for the service, typically a local network domain like `".local."`.
    pub domain_name: String,
    /// The hostname of the machine providing the service. May be `None` if not available.
    pub host_name: Option<String>,
    /// A set of IP addresses associated with the service.
    /// `BTreeSet` is used to ensure consistent ordering for hashing and comparison.
    pub addresses: BTreeSet<IpAddr>,
    /// A set of socket addresses (IP address and port) for the service.
    /// `BTreeSet` is used for consistent ordering.
    pub socket_addresses: BTreeSet<SocketAddr>,
    /// The network port on which the service is listening.
    pub port: u16,
    /// A map of additional properties (TXT records in mDNS) associated with the service.
    /// `BTreeMap` is used for consistent ordering.
    pub properties: BTreeMap<String, String>,
    /// Timestamp (seconds since UNIX_EPOCH) of when this service was last seen or updated.
    /// Used to determine service liveness and for TTL management.
    pub last_seen: Option<u64>,
}

/// Configuration for registering a local service and for discovery behavior.
#[derive(Clone, Debug)]
pub struct LocalServiceConfig {
    /// A unique identifier for the device publishing the service.
    /// This ID should uniquely identify the device on the network.
    pub device_id: String,

    /// A user-friendly name for the device.
    /// This name might be displayed to users on other devices.
    pub device_name: String,

    /// The version string of the software or service being advertised.
    pub version: String,

    /// The type of service to be broadcast and discovered.
    /// Example: `"_myapp-filetransfer._tcp.local."`.
    pub service_type: String,

    /// The network port on which the local service is listening.
    pub port: u16,

    /// An optional custom instance name for the service.
    /// If not provided, a name might be generated or a default used.
    /// Example: `"My Custom Service Name"`. The full mDNS name will be constructed from this.
    pub instance_name: String,

    /// A collection of additional properties (metadata) to be broadcast with the service.
    /// These are often published as TXT records in mDNS.
    pub properties: Option<HashMap<String, String>>,

    /// Service Time-To-Live (TTL) in seconds.
    /// This is the maximum time a discovered service is considered online without a refresh
    /// by the remote peer. Local cleanup of stale services also uses this.
    pub service_ttl: u64,

    /// mDNS response delay range in milliseconds: `(min_ms, max_ms)`.
    /// Helps prevent network congestion (response storms) when multiple devices
    /// respond to mDNS queries simultaneously. A random delay within this range is chosen.
    pub mdns_response_delay_ms: (u64, u64),

    /// Discovery refresh interval in seconds.
    /// The interval at which the discovery component might re-query or re-announce
    /// to keep service lists fresh. (Note: mDNS has its own refresh mechanisms).
    pub refresh_interval: u64,
}
