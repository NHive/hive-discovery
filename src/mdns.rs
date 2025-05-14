use std::collections::{BTreeMap, BTreeSet, HashMap, HashSet};
use std::net::SocketAddr;
use std::sync::{Arc, RwLock};
use std::thread;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use log::{debug, error, info, warn};
use mdns_sd::{ServiceDaemon, ServiceEvent, ServiceInfo};
use tokio::sync::broadcast;

use crate::error::{HiveDiscoError, Result};
use crate::types::{DiscoveryEvent, DiscoveryServiceDetails, DiscoveryServiceStatus};
use crate::{DiscoveryService, LocalServiceConfig};

/// Internal state for the mDNS service.
struct MdnsServiceState {
    /// Current operational status of the discovery service.
    status: DiscoveryServiceStatus,
    /// Set of instance names to filter out from discovery events.
    filter: HashSet<String>,
    /// Hashes of discovered services to detect changes.
    service_hashes: HashMap<String, u64>,
}

/// An mDNS/DNS-SD protocol-based service discovery implementation.
pub struct MdnsDiscoveryService {
    mdns: Arc<ServiceDaemon>,
    config: LocalServiceConfig,
    sender: broadcast::Sender<DiscoveryEvent>,
    discovered_services: Arc<RwLock<HashMap<String, DiscoveryServiceDetails>>>,
    state: Arc<RwLock<MdnsServiceState>>,
    own_instance_name: Arc<RwLock<Option<String>>>,
    discovery_started_sent: Arc<RwLock<bool>>,
}

impl MdnsDiscoveryService {
    /// Creates a new `MdnsDiscoveryService` instance.
    ///
    /// # Arguments
    /// * `config` - Configuration for the local service to be registered and for discovery parameters.
    ///
    /// # Errors
    /// Returns `HiveDiscoError::ConfigError` if the configuration is invalid.
    /// Returns `HiveDiscoError::MdnsError` if the mDNS daemon fails to initialize.
    pub fn new(config: LocalServiceConfig) -> Result<Self> {
        // Validate configuration
        if config.service_type.is_empty() {
            return Err(HiveDiscoError::ConfigError("Service type cannot be empty".into()));
        }

        if config.instance_name.is_empty() {
            return Err(HiveDiscoError::ConfigError("Instance name cannot be empty".into()));
        }

        // Create mDNS service daemon
        let mdns = ServiceDaemon::new().map_err(|e| {
            error!("Failed to create mDNS service daemon: {}", e);
            HiveDiscoError::MdnsError(e)
        })?;

        // Create event channel with a buffer size of 100
        let (sender, _) = broadcast::channel(100);

        let service = MdnsDiscoveryService {
            mdns: Arc::new(mdns),
            config,
            sender,
            discovered_services: Arc::new(RwLock::new(HashMap::new())),
            state: Arc::new(RwLock::new(MdnsServiceState {
                status: DiscoveryServiceStatus::Stopped,
                filter: HashSet::new(),
                service_hashes: HashMap::new(),
            })),
            own_instance_name: Arc::new(RwLock::new(None)),
            discovery_started_sent: Arc::new(RwLock::new(false)),
        };

        Ok(service)
    }

    /// Starts a background task to clean up expired services.
    /// This task periodically checks `discovered_services` and removes services
    /// whose `last_seen` timestamp exceeds the configured `service_ttl`.
    fn start_cleanup_task(&self) -> Result<()> {
        let services = Arc::clone(&self.discovered_services);
        let sender = self.sender.clone();
        let ttl = self.config.service_ttl;
        let state = Arc::clone(&self.state);

        thread::spawn(move || {
            debug!("Starting service cleanup task");

            // Run cleanup loop until service status is no longer Running
            loop {
                {
                    // Check service status
                    let state_guard = state.read().unwrap();
                    if state_guard.status != DiscoveryServiceStatus::Running {
                        break;
                    }
                }

                // Check every second
                thread::sleep(Duration::from_secs(1));

                let mut services_to_remove = Vec::new();
                let now = SystemTime::now()
                    .duration_since(UNIX_EPOCH)
                    .unwrap_or_default()
                    .as_secs();

                // Mark services for removal
                {
                    let services_map = services.read().unwrap();

                    for (id, service) in services_map.iter() {
                        if let Some(last_seen) = service.last_seen {
                            if now - last_seen > ttl {
                                debug!("Service {} has expired", id);
                                services_to_remove.push(id.clone());
                            }
                        }
                    }
                }

                // If there are expired services, remove them
                if !services_to_remove.is_empty() {
                    let mut services_map = services.write().unwrap();

                    for id in &services_to_remove {
                        services_map.remove(id);
                    }

                    // Send service removal event
                    for id in services_to_remove {
                        if let Err(e) = sender.send(DiscoveryEvent::ServiceLost(id.clone())) {
                            warn!("Failed to send service removal event: {}, Service ID: {}", e, id);
                        }
                    }
                }
            }

            debug!("Service cleanup task stopped");
        });

        Ok(())
    }

    /// Calculates a hash for `DiscoveryServiceDetails` to detect changes.
    fn calculate_service_hash(service: &DiscoveryServiceDetails) -> u64 {
        use std::collections::hash_map::DefaultHasher;
        use std::hash::{Hash, Hasher};

        let mut hasher = DefaultHasher::new();
        service.service_type.hash(&mut hasher);
        service.instance_name.hash(&mut hasher);
        service.domain_name.hash(&mut hasher);
        service.host_name.hash(&mut hasher);
        service.addresses.hash(&mut hasher);
        service.socket_addresses.hash(&mut hasher);
        service.port.hash(&mut hasher);
        service.properties.hash(&mut hasher);
        hasher.finish()
    }

    /// Handles incoming mDNS service events from the `ServiceDaemon`.
    fn handle_service_event(
        event: ServiceEvent,
        services: &Arc<RwLock<HashMap<String, DiscoveryServiceDetails>>>,
        sender: &broadcast::Sender<DiscoveryEvent>,
        state: &Arc<RwLock<MdnsServiceState>>,
        service_type: &str,
    ) {
        match event {
            ServiceEvent::ServiceFound(type_domain, fullname) => {
                // Check if the service type matches the one we are browsing for.
                if type_domain != service_type {
                    debug!("Ignoring mismatched service type: {} (expected: {})", type_domain, service_type);
                    return;
                }
                debug!("Service found (unresolved): {} ({})", fullname, type_domain);
            }

            ServiceEvent::ServiceResolved(info) => {
                // Check if the resolved service type matches.
                if info.get_type() != service_type {
                    debug!("Ignoring mismatched resolved service type: {} (expected: {})", info.get_type(), service_type);
                    return;
                }

                let id = info.get_fullname().to_string();

                // Check if this service instance name is in the filter list.
                let should_filter = {
                    let state_guard = state.read().unwrap();
                    state_guard.filter.contains(&id)
                };

                if should_filter {
                    debug!("Service {} is filtered, ignoring.", id);
                    return;
                }

                // Get current timestamp.
                let now = SystemTime::now()
                    .duration_since(UNIX_EPOCH)
                    .unwrap_or_default()
                    .as_secs();

                // Extract properties from TXT records.
                let properties = info
                    .get_properties()
                    .iter()
                    .map(|prop| (prop.key().to_string(), prop.val_str().to_string()))
                    .collect::<BTreeMap<_, _>>(); // Collect into a BTreeMap for consistent ordering.

                let service_details = DiscoveryServiceDetails {
                    service_type: info.get_type().to_string(),
                    instance_name: id.clone(),
                    domain_name: ".local.".to_string(), // Typically .local. for mDNS
                    host_name: Some(info.get_hostname().to_string()),
                    addresses: info.get_addresses().iter().cloned().collect::<BTreeSet<_>>(),
                    socket_addresses: info
                        .get_addresses()
                        .iter()
                        .map(|&addr| SocketAddr::new(addr, info.get_port()))
                        .collect::<BTreeSet<_>>(),
                    port: info.get_port(),
                    properties,
                    last_seen: Some(now),
                };

                // Calculate the hash of the current service details.
                let current_hash = Self::calculate_service_hash(&service_details);

                // Check if the service details have changed compared to the last known state.
                let has_changed = {
                    let mut state_guard = state.write().unwrap();
                    let previous_hash = state_guard.service_hashes.get(&id).cloned();

                    // Update the hash for this service instance.
                    state_guard.service_hashes.insert(id.clone(), current_hash);

                    // If it's a new service or the hash is different, consider it changed.
                    previous_hash.map_or(true, |hash| hash != current_hash)
                };

                // Update the map of discovered services.
                {
                    let mut services_map = services.write().unwrap();
                    services_map.insert(id.clone(), service_details.clone());
                }

                // Only send a ServiceFound event if the service is new or has changed.
                if has_changed {
                    debug!("Service {} updated or newly discovered, sending ServiceFound event.", id);
                    if let Err(e) = sender.send(DiscoveryEvent::ServiceFound(service_details)) {
                        warn!("Failed to send ServiceFound event: {}", e);
                    }
                } else {
                    debug!("Service {} unchanged, skipping event send.", id);
                }
            }

            ServiceEvent::ServiceRemoved(type_domain, fullname) => {
                // Check if the service type matches.
                if type_domain != service_type {
                    debug!("Ignoring mismatched removed service type: {} (expected: {})", type_domain, service_type);
                    return;
                }

                debug!("Service removed: {} ({})", fullname, type_domain);

                // Remove from discovered services map.
                {
                    let mut services_map = services.write().unwrap();
                    services_map.remove(&fullname);
                }

                // Remove service from hash records.
                {
                    let mut state_guard = state.write().unwrap();
                    state_guard.service_hashes.remove(&fullname);
                }

                // Send service removal event.
                if let Err(e) = sender.send(DiscoveryEvent::ServiceLost(fullname)) {
                    warn!("Failed to send ServiceLost event: {}", e);
                }
            }

            _ => {} // Ignore other event types (e.g., NetworkInterfaceEnumerated).
        }
    }
}

impl DiscoveryService for MdnsDiscoveryService {
    fn register_service(&self) -> Result<()> {
        let instance_name_from_config = self.config.instance_name.clone();
        let service_type_from_config = self.config.service_type.clone();

        // Instance name should not end with a dot for ServiceInfo.
        let final_instance_name = instance_name_from_config.trim_end_matches('.').to_string();

        // Service type should be passed to mdns-sd as configured by the user,
        // assuming the user has configured it correctly (e.g., "_my-service._tcp.local.").
        // The mdns-sd library expects the type string to include the domain part (e.g., .local.).
        let final_service_type = service_type_from_config;

        info!("Registering service: instance='{}', type='{}'", final_instance_name, final_service_type);

        // Store own instance name and add to filter to avoid self-discovery events if not desired.
        {
            let mut own_name = self.own_instance_name.write().unwrap();
            *own_name = Some(final_instance_name.clone());

            let mut state = self.state.write().unwrap();
            state.filter.insert(final_instance_name.clone());
        }

        // Set service properties.
        let mut properties = self.config.properties.clone().unwrap_or_default();

        // Add default properties.
        properties.insert("device_id".to_string(), self.config.device_id.clone());
        properties.insert("device_name".to_string(), self.config.device_name.clone());
        properties.insert("version".to_string(), self.config.version.clone());

        // Create ServiceInfo object.
        let service_info = ServiceInfo::new(
            &final_service_type,    // Use the user-provided service type.
            &final_instance_name, // Use the trimmed instance name.
            "localhost.local.", // Hostname is usually auto-detected or can be specific.
            (), // Empty IP address list; mdns-sd will use auto-detection.
            self.config.port,
            properties,
        )
        .map_err(|e| {
            error!("Failed to create service info: {}", e);
            HiveDiscoError::MdnsError(e)
        })?
        .enable_addr_auto(); // Enable automatic address detection.

        // Register service with the mDNS daemon.
        self.mdns.register(service_info).map_err(|e| {
            error!("Failed to register mDNS service: {}", e);
            HiveDiscoError::MdnsError(e)
        })?;

        info!("Service registered successfully: {}", final_instance_name);
        Ok(())
    }

    fn start_discovery(&self) -> Result<()> {
        // Check current status.
        {
            let mut state_guard = self.state.write().unwrap();

            match state_guard.status {
                DiscoveryServiceStatus::Running => {
                    // If already running, ensure DiscoveryStarted event is sent to new subscribers.
                    if self.sender.receiver_count() > 0 {
                        let mut started_sent = self.discovery_started_sent.write().unwrap();
                        if !*started_sent {
                            if let Err(e) = self.sender.send(DiscoveryEvent::DiscoveryStarted) {
                                warn!("Failed to resend DiscoveryStarted event: {}", e);
                            } else {
                                *started_sent = true;
                            }
                        }
                    }
                    return Ok(());
                }
                DiscoveryServiceStatus::Stopping => {
                    return Err(HiveDiscoError::StateError(
                        "Service is currently stopping, cannot start discovery.".into(),
                    ));
                }
                DiscoveryServiceStatus::Stopped => {
                    state_guard.status = DiscoveryServiceStatus::Running;
                    // Reset discovery_started_sent flag for the new session.
                    let mut started_sent = self.discovery_started_sent.write().unwrap();
                    *started_sent = false;
                }
            }
        }

        let service_type = self.config.service_type.clone();
        info!("Starting service discovery for type: {}", service_type);

        // Start background cleanup task for expired services.
        self.start_cleanup_task()?;

        // Create mDNS browser for the specified service type.
        let receiver = self.mdns.browse(&service_type).map_err(|e| {
            error!("Failed to create mDNS browser: {}", e);
            HiveDiscoError::MdnsError(e)
        })?;

        let services = Arc::clone(&self.discovered_services);
        let sender = self.sender.clone();
        let state = Arc::clone(&self.state);
        let discovery_started_sent = Arc::clone(&self.discovery_started_sent);

        // Send DiscoveryStarted event if there are subscribers and it hasn't been sent for this session.
        if self.sender.receiver_count() > 0 {
            let mut started_sent_guard = discovery_started_sent.write().unwrap();
            if !*started_sent_guard {
                if let Err(e) = sender.send(DiscoveryEvent::DiscoveryStarted) {
                    warn!("Failed to send DiscoveryStarted event: {}", e);
                } else {
                    *started_sent_guard = true;
                }
            }
        }

        // Create a thread to handle service discovery events from the mDNS browser.
        thread::spawn(move || {
            debug!("Service discovery event processing thread started for type: {}", service_type);

            while let Ok(event) = receiver.recv() {
                // Check service status; stop processing if not Running.
                {
                    let state_guard = state.read().unwrap();
                    if state_guard.status != DiscoveryServiceStatus::Running {
                        break;
                    }
                }
                // Pass service type to the event handler for filtering.
                MdnsDiscoveryService::handle_service_event(event, &services, &sender, &state, &service_type);
            }

            debug!("Service discovery event processing thread finished for type: {}", service_type);

            // Send DiscoveryStopped event when the loop terminates.
            if let Err(e) = sender.send(DiscoveryEvent::DiscoveryStopped) {
                warn!("Failed to send DiscoveryStopped event: {}", e);
            }

            // Update status if it was in Stopping state.
            let mut state_guard = state.write().unwrap();
            if state_guard.status == DiscoveryServiceStatus::Stopping {
                state_guard.status = DiscoveryServiceStatus::Stopped;
            }
        });

        Ok(())
    }

    fn subscribe(&self) -> broadcast::Receiver<DiscoveryEvent> {
        let receiver = self.sender.subscribe();
        // If the service is already running and the DiscoveryStarted event hasn't been sent
        // to this new subscriber (or if it's the first subscriber), try sending it.
        let state_guard = self.state.read().unwrap();
        if state_guard.status == DiscoveryServiceStatus::Running {
            let mut started_sent = self.discovery_started_sent.write().unwrap();
            // Send if not sent yet, or if this is the very first subscriber.
            if !*started_sent || self.sender.receiver_count() == 1 { 
                if self.sender.send(DiscoveryEvent::DiscoveryStarted).is_ok() {
                    *started_sent = true;
                }
            }
        }
        receiver
    }

    fn add_filter(&self, instance_name: String) {
        let trimmed_name = instance_name.trim_end_matches('.').to_string();
        debug!("Adding filter for instance name: {}", trimmed_name);
        let mut state = self.state.write().unwrap();
        state.filter.insert(trimmed_name);
    }

    fn remove_filter(&self, instance_name: &str) {
        let trimmed_instance_name = instance_name.trim_end_matches('.');
        // Ensure not to remove its own instance name from the filter if it was added.
        let own_name_guard = self.own_instance_name.read().unwrap();
        if let Some(own_name) = own_name_guard.as_ref() {
            if own_name == trimmed_instance_name {
                debug!("Attempted to remove own instance name filter ('{}'), which is protected. Ignored.", trimmed_instance_name);
                return;
            }
        }

        debug!("Removing filter for instance name: {}", trimmed_instance_name);
        let mut state = self.state.write().unwrap();
        state.filter.remove(trimmed_instance_name);
    }

    fn stop_discovery(&self) -> Result<()> {
        // Check current status.
        {
            let mut state = self.state.write().unwrap();

            match state.status {
                DiscoveryServiceStatus::Stopped | DiscoveryServiceStatus::Stopping => {
                    // Already stopped or stopping, nothing to do.
                    return Ok(());
                }
                DiscoveryServiceStatus::Running => {
                    state.status = DiscoveryServiceStatus::Stopping;
                    // Reset discovery_started_sent flag, so it can be resent on next start.
                    let mut started_sent = self.discovery_started_sent.write().unwrap();
                    *started_sent = false;
                }
            }
        }

        let service_type = self.config.service_type.clone();
        info!("Stopping service discovery for type: {}", service_type);

        // Stop browsing for services. This will cause the event processing loop in `start_discovery` to terminate.
        self.mdns.stop_browse(&service_type).map_err(|e| {
            error!("Failed to stop mDNS browser for type '{}': {}", service_type, e);
            HiveDiscoError::MdnsError(e)
        })?;

        Ok(())
    }

    fn refresh_services(&self) -> Result<()> {
        debug!("Refreshing discovered services");

        // Get a snapshot of all currently discovered services.
        let services_snapshot = {
            let services_map = self.discovered_services.read().unwrap();
            services_map.values().cloned().collect::<Vec<_>>()
        };

        // Clear hash records to ensure that ServiceFound events are re-sent for all services,
        // even if they haven't changed, as this is a manual refresh.
        {
            let mut state = self.state.write().unwrap();
            state.service_hashes.clear();
        }

        // Send ServiceFound event for each service in the snapshot.
        for service in services_snapshot {
            if let Err(e) = self.sender.send(DiscoveryEvent::ServiceFound(service.clone())) {
                warn!("Failed to send refreshed ServiceFound event for '{}': {}", service.instance_name, e);
            }
        }

        Ok(())
    }

    fn shutdown(&self) -> Result<()> {
        info!("Shutting down mDNS service discovery component");

        // First, attempt to stop active discovery.
        if let Err(e) = self.stop_discovery() {
            warn!("Error occurred while stopping service discovery during shutdown: {}", e);
            // Continue with shutdown regardless of this error.
        }

        // Update state to Stopped.
        {
            let mut state = self.state.write().unwrap();
            state.status = DiscoveryServiceStatus::Stopped;
        }

        // Shutdown the mDNS daemon.
        let status_receiver = self.mdns.shutdown().map_err(|e| {
            error!("Failed to initiate mDNS daemon shutdown: {}", e);
            HiveDiscoError::ShutdownError(format!("Failed to initiate mDNS daemon shutdown: {}", e))
        })?;

        // Wait for daemon shutdown confirmation.
        // It's important to wait for the daemon to confirm shutdown to ensure resources are released.
        match status_receiver.recv_timeout(Duration::from_secs(5)) { // Add a timeout
            Ok(_) => {
                info!("mDNS daemon shut down successfully.");
                Ok(())
            }
            Err(e) => {
                let error_msg = format!("Waiting for mDNS daemon shutdown failed: {}", e);
                error!("{}", error_msg);
                Err(HiveDiscoError::ShutdownError(error_msg))
            }
        }
    }

    fn status(&self) -> DiscoveryServiceStatus {
        let state = self.state.read().unwrap();
        state.status
    }
}

impl Drop for MdnsDiscoveryService {
    fn drop(&mut self) {
        info!("MdnsDiscoveryService instance is being dropped. Performing automatic shutdown.");
        // Attempt to gracefully shut down the service.
        // Errors during drop are logged but not propagated further, as drop cannot return a Result.
        if let Err(e) = self.shutdown() {
            error!("Error during automatic shutdown in drop: {}", e);
        }
    }
}
