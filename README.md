# Hive Discovery

[English](./README_EN.md) | 中文

Hive Discovery是一个用Rust编写的轻量级服务发现库，提供跨平台的网络服务发现功能，暂支持mDNS/DNS-SD协议。

## 特性

- 使用mDNS/DNS-SD进行零配置网络服务发现
- 支持服务注册和发现
- 异步事件通知系统
- 自动过滤功能，避免自我发现
- 服务生命周期管理和超时控制
- 跨平台支持（Linux、macOS、Windows）


### 基本使用示例

```rust
use std::collections::HashMap;
use hive_discovery::{
    DiscoveryEvent, DiscoveryImplementation, LocalServiceConfig, create_discovery_service
};

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // 创建服务发现实例
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

    // 订阅服务发现事件
    let mut receiver = discovery.subscribe();
    
    // 注册本地服务
    discovery.register_service()?;
    
    // 开始服务发现
    discovery.start_discovery()?;
    
    // 处理发现事件
    tokio::spawn(async move {
        while let Ok(event) = receiver.recv().await {
            match event {
                DiscoveryEvent::ServiceFound(service) => {
                    println!("发现服务: {} ({})", service.instance_name, service.service_type);
                }
                DiscoveryEvent::ServiceLost(name) => {
                    println!("服务离线: {}", name);
                }
                _ => {}
            }
        }
    });
    
    // 应用程序逻辑...
    
    // 关闭服务发现
    discovery.shutdown()?;
    
    Ok(())
}
```

查看 [examples](./examples) 目录获取更多示例。

## 详细使用说明

### 服务配置

`LocalServiceConfig` 结构体提供了丰富的配置选项：

- `service_type`: 服务类型，例如 "_my-service._tcp.local."
- `port`: 服务监听端口
- `instance_name`: 服务实例名称
- `properties`: 服务属性（键值对形式）
  - 其他自定义属性...
- `service_ttl`: 服务生存时间（秒）
- `mdns_response_delay_ms`: mDNS响应延迟范围，用于网络拥塞控制
- `refresh_interval`: 发现刷新间隔

### 服务发现事件

监听 `DiscoveryEvent` 事件来接收服务发现通知：

- `ServiceFound`: 发现新服务或服务信息更新
- `ServiceLost`: 服务不再可用
- `DiscoveryStarted`: 服务发现过程已启动
- `DiscoveryStopped`: 服务发现过程已停止

### 过滤功能

使用 `add_filter` 和 `remove_filter` 方法可以过滤不需要的服务：

```rust
// 添加过滤器忽略特定服务实例
discovery.add_filter("ServiceToIgnore".to_string());

// 移除过滤器
discovery.remove_filter("ServiceToIgnore");
```

