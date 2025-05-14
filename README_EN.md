# Hive Discovery

Hive Discovery is a lightweight service discovery library written in Rust, providing cross-platform network service discovery functionality with support for mDNS/DNS-SD protocols.

## Features

- Zero-configuration network service discovery using mDNS/DNS-SD
- Support for service registration and discovery
- Asynchronous event notification system
- Automatic filtering to avoid self-discovery
- Service lifecycle management and timeout control
- Cross-platform support (Linux, macOS, Windows)

### Basic Usage Example

```rust
use std::collections::HashMap;
use hive_discovery::{
    DiscoveryEvent, DiscoveryImplementation, LocalServiceConfig, create_discovery_service
};

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Create a discovery service instance
    let mut properties = HashMap::new();
    properties.insert("device_id".to_string(), "my-device-123".to_string());
    properties.insert("device_name".to_string(), "My Device".to_string());
    properties.insert("version".to_string(), "1.0.0".to_string());
    
    let config = LocalServiceConfig {
        service_type: "_my-service._tcp.local.".to_string(),
        port: 8080,
        instance_name: "MyServiceInstance".to_string(),
        properties: Some(properties),
        service_ttl: 60,
        mdns_response_delay_ms: (20, 120),
        refresh_interval: 30,
    };
    
    let discovery = create_discovery_service(DiscoveryImplementation::Mdns, config)?;

    // Subscribe to service discovery events
    let mut receiver = discovery.subscribe();
    
    // Register local service
    discovery.register_service()?;
    
    // Start service discovery
    discovery.start_discovery()?;
    
    // Handle discovery events
    tokio::spawn(async move {
        while let Ok(event) = receiver.recv().await {
            match event {
                DiscoveryEvent::ServiceFound(service) => {
                    println!("Service found: {} ({})", service.instance_name, service.service_type);
                }
                DiscoveryEvent::ServiceLost(name) => {
                    println!("Service lost: {}", name);
                }
                _ => {}
            }
        }
    });
    
    // Application logic...
    
    // Shutdown service discovery
    discovery.shutdown()?;
    
    Ok(())
}
```

Check the [examples](./examples) directory for more examples.

## Detailed Usage Instructions

### Service Configuration

The `LocalServiceConfig` struct provides rich configuration options:

- `service_type`: Service type, e.g. "_my-service._tcp.local."
- `port`: Port the service listens on
- `instance_name`: Service instance name
- `properties`: Service properties (key-value pairs, must include `device_id`)
  - `device_id`: Unique identifier for the device (required)
  - `device_name`: Friendly name for the device (recommended)
  - `version`: Software version (recommended)
  - Other custom properties...
- `service_ttl`: Service Time To Live (seconds)
- `mdns_response_delay_ms`: mDNS response delay range, for network congestion control
- `refresh_interval`: Discovery refresh interval

### Service Discovery Events

Listen to `DiscoveryEvent` events to receive service discovery notifications:

- `ServiceFound`: New service discovered or service information updated
- `ServiceLost`: Service is no longer available
- `DiscoveryStarted`: Service discovery process has started
- `DiscoveryStopped`: Service discovery process has stopped

### Filtering Functionality

Use the `add_filter` and `remove_filter` methods to filter unwanted services:

```rust
// Add filter to ignore specific service instance
discovery.add_filter("ServiceToIgnore".to_string());

// Remove filter
discovery.remove_filter("ServiceToIgnore");
```
