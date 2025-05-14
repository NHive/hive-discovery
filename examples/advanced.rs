use std::collections::HashMap;
use std::sync::Arc;
use std::time::Duration;

use hive_discovery::{
    DiscoveryEvent, DiscoveryImplementation, LocalServiceConfig, create_discovery_service,
};
use log::{error, info};
use tokio::signal;
use tokio::sync::mpsc;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // 初始化日志
    env_logger::Builder::from_env(env_logger::Env::default().default_filter_or("info")).init();

    // 创建控制通道
    let (tx, mut rx) = mpsc::channel::<String>(32);

    // 创建服务
    let config = LocalServiceConfig {
        device_id: "advanced-device".to_string(),
        device_name: "示例设备".to_string(),
        version: "1.0.0".to_string(),
        service_type: "_hive-advanced._tcp.local.".to_string(),
        port: 9000,
        instance_name: "HiveAdvancedExample".to_string(),
        properties: Some({
            let mut props = HashMap::new();
            props.insert("mode".to_string(), "advanced".to_string());
            props.insert("features".to_string(), "refresh,filter".to_string());
            props
        }),
        service_ttl: 30,
        mdns_response_delay_ms: (10, 100),
        refresh_interval: 15,
    };

    let service = create_discovery_service(DiscoveryImplementation::Mdns, config)?;

    // 启动服务
    service.register_service()?;
    service.start_discovery()?;

    // 服务发现处理
    let service_clone = Arc::clone(&service);
    let tx_clone = tx.clone();
    tokio::spawn(async move {
        let mut receiver = service_clone.subscribe();
        let mut discovered_services = HashMap::new();

        while let Ok(event) = receiver.recv().await {
            match event {
                DiscoveryEvent::ServiceFound(details) => {
                    let name = details.instance_name.clone();
                    discovered_services.insert(name.clone(), details);
                    info!("服务已发现/更新: {}", name);
                }
                DiscoveryEvent::ServiceLost(name) => {
                    discovered_services.remove(&name);
                    info!("服务已失去: {}", name);
                }
                _ => {}
            }

            // 发送服务数量统计
            let _ = tx_clone
                .send(format!("当前发现的服务数: {}", discovered_services.len()))
                .await;
        }
    });

    // 命令处理循环
    tokio::spawn(async move {
        let mut interval = tokio::time::interval(Duration::from_secs(5));

        loop {
            tokio::select! {
                _ = interval.tick() => {
                    // 定期刷新服务
                    if let Err(e) = service.refresh_services() {
                        error!("刷新服务失败: {}", e);
                    } else {
                        info!("手动刷新服务列表");
                    }
                }
                Some(msg) = rx.recv() => {
                    info!("状态: {}", msg);
                }
                _ = signal::ctrl_c() => {
                    info!("接收到中断信号，开始关闭...");
                    if let Err(e) = service.shutdown() {
                        error!("关闭失败: {}", e);
                    }
                    break;
                }
            }
        }
    });

    // 等待中断信号
    signal::ctrl_c().await?;
    info!("程序即将退出");

    Ok(())
}
