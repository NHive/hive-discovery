// file_path: src/services/discovery/mod.rs
pub mod error;
pub mod mdns;
pub mod types;

pub use error::{HiveDiscoError, Result};
pub use types::{DiscoveryEvent, DiscoveryServiceDetails, DiscoveryServiceStatus, LocalServiceConfig};

use std::sync::Arc;
use tokio::sync::broadcast;

/// Service Discovery Trait.
///
/// Defines the core functional interface that service discovery components must implement.
pub trait DiscoveryService: Sync + Send {
    /// Registers the local service on the network.
    ///
    /// This makes the current device discoverable as a service provider.
    fn register_service(&self) -> Result<()>;
    
    /// Starts network service discovery.
    ///
    /// Begins listening for service broadcasts on the network.
    fn start_discovery(&self) -> Result<()>;
    
    /// Subscribes to service discovery events.
    ///
    /// Returns a `broadcast::Receiver` to receive various service discovery events.
    fn subscribe(&self) -> broadcast::Receiver<DiscoveryEvent>;
    
    /// Adds an instance name filter.
    ///
    /// This is used to ignore events from specific service instances.
    fn add_filter(&self, instance_name: String);
    
    /// Removes an instance name filter.
    fn remove_filter(&self, instance_name: &str);
    
    /// Stops network service discovery.
    ///
    /// Stops listening for service broadcasts on the network.
    fn stop_discovery(&self) -> Result<()>;
    
    /// Refreshes discovered services.
    ///
    /// Re-sends `ServiceFound` events for all currently known services.
    /// This can be useful for new subscribers to get the current state.
    fn refresh_services(&self) -> Result<()>;
    
    /// Shuts down the service discovery component.
    ///
    /// Completely stops all service discovery related functions and releases resources.
    fn shutdown(&self) -> Result<()>;
    
    /// Gets the current operational status of the service discovery component.
    fn status(&self) -> DiscoveryServiceStatus;
}

/// Specifies the underlying implementation for service discovery.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum DiscoveryImplementation {
    /// mDNS/DNS-SD based service discovery.
    Mdns,
    /// UDP multicast based service discovery (placeholder).
    Multicast,
}

/// Creates a service discovery component instance.
/// 
/// # Arguments
/// * `implementation` - Specifies which service discovery implementation to use.
/// * `config` - Configuration parameters for the local service.
/// 
/// # Returns
/// A `Result` containing an `Arc` to a component that implements the `DiscoveryService` trait,
/// or a `HiveDiscoError` if instantiation fails.
pub fn create_discovery_service(
    implementation: DiscoveryImplementation,
    config: LocalServiceConfig,
) -> Result<Arc<dyn DiscoveryService>> {
    match implementation {
        DiscoveryImplementation::Mdns => {
            let service = mdns::MdnsDiscoveryService::new(config)?;
            Ok(Arc::new(service))
        }
        DiscoveryImplementation::Multicast => {
            Err(error::HiveDiscoError::ConfigError(
                "Multicast implementation is not yet complete".to_string(),
            ))
        }
    }
}
