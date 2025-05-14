use std::collections::HashMap;
use std::sync::Arc;
use std::time::Duration;

use hive_discovery::{
    DiscoveryEvent, DiscoveryImplementation, LocalServiceConfig, create_discovery_service,
};
use log::{debug, info, warn};

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // 初始化日志
    env_logger::Builder::from_env(env_logger::Env::default().default_filter_or("info")).init();

    // 创建服务提供者
    let provider = create_provider_service()?;
    provider.register_service()?; // 提供者注册自己
    info!("提供者服务已注册");
    provider.start_discovery()?; // 提供者启动发现（以便响应查询并维护其注册）
    info!("提供者发现已启动");

    // 稍等片刻，确保服务已经注册
    tokio::time::sleep(Duration::from_secs(1)).await;

    // 创建服务发现者实例
    let discoverer_service = create_discoverer_config_only()?;

    // 发现者也注册自己 (使其成为网络中的一个节点，并允许自动过滤)
    // 如果发现者纯粹是客户端，则此步骤可以省略，但需要手动管理过滤。
    // 为了利用 MdnsDiscoveryService 自动过滤自身实例名的特性，这里进行注册。
    discoverer_service.register_service()?;
    info!("发现者服务已注册 (用于自过滤)");

    // 监听发现事件 (在启动发现之前订阅)
    let mut receiver = discoverer_service.subscribe();
    info!("发现者已订阅事件");

    // 启动服务发现者的发现过程
    discoverer_service.start_discovery()?;
    info!("发现者发现已启动");

    // 处理发现事件
    tokio::spawn(async move {
        while let Ok(event) = receiver.recv().await {
            match event {
                DiscoveryEvent::ServiceFound(service) => {
                    info!(
                        "发现服务: {} ({})",
                        service.instance_name, service.service_type
                    );
                    debug!("服务详情: {:?}", service);
                }
                DiscoveryEvent::ServiceLost(name) => {
                    warn!("服务离线: {}", name);
                }
                DiscoveryEvent::DiscoveryStarted => {
                    info!("服务发现已启动");
                }
                DiscoveryEvent::DiscoveryStopped => {
                    warn!("服务发现已停止");
                }
            }
        }
    });

    // 主程序运行一段时间
    info!("服务发现示例运行中...");
    tokio::time::sleep(Duration::from_secs(30)).await;

    // 关闭服务
    info!("关闭服务发现示例...");
    provider.shutdown()?;
    discoverer_service.shutdown()?; // 修改变量名

    info!("示例程序已结束");
    Ok(())
}

/// 创建服务提供者
fn create_provider_service()
-> Result<Arc<dyn hive_discovery::DiscoveryService>, Box<dyn std::error::Error>> {
    info!("创建服务提供者...");

    // 创建服务属性
    let mut properties = HashMap::new();
    properties.insert("device_id".to_string(), "provider-123456".to_string());
    properties.insert("device_name".to_string(), "示例提供者".to_string());
    properties.insert("version".to_string(), "1.0.0".to_string());
    properties.insert("description".to_string(), "示例服务提供者".to_string());
    properties.insert("capability".to_string(), "file-sharing".to_string());

    // 配置服务
    let config = LocalServiceConfig {
        service_type: "_hive-example._tcp.local.".to_string(),
        port: 8080,
        instance_name: "HiveExampleProvider".to_string(),
        properties: Some(properties),
        service_ttl: 60,
        mdns_response_delay_ms: (20, 120),
        refresh_interval: 30,
    };

    // 创建服务发现实例
    let service = create_discovery_service(DiscoveryImplementation::Mdns, config)?;

    // 注册和启动移至 main
    Ok(service)
}

/// 创建服务发现者 (仅配置)
fn create_discoverer_config_only()
-> Result<Arc<dyn hive_discovery::DiscoveryService>, Box<dyn std::error::Error>> {
    info!("创建服务发现者实例 (仅配置)...");

    let mut properties = HashMap::new();
    properties.insert("device_id".to_string(), "discoverer-654321".to_string());
    properties.insert("device_name".to_string(), "示例发现者".to_string());
    properties.insert("version".to_string(), "1.0.0".to_string());

    // 配置服务发现者
    let config = LocalServiceConfig {
        service_type: "_hive-example._tcp.local.".to_string(),
        port: 8081, // 发现者也需要一个端口进行mDNS通信，即使它不提供用户服务
        instance_name: "HiveExampleDiscoverer".to_string(),
        properties: Some(properties),
        service_ttl: 60,
        mdns_response_delay_ms: (20, 120),
        refresh_interval: 30,
    };

    let service = create_discovery_service(DiscoveryImplementation::Mdns, config)?;

    // 注册和启动移至 main
    info!("服务发现者实例已创建");

    Ok(service)
}
