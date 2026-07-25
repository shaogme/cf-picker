use cf_picker_core::models::PickerOptions;
use cf_picker_core::traits::DownloadClient;
use cf_picker_tokio::client::Client;
use std::time::Duration;

#[tokio::test]
async fn test_worker_speedtest_node() {
    let client = Client::new();
    let options = PickerOptions {
        url: "https://speed.cloudflare.com/__down?bytes=10485760".to_string(), // 10MB 测试
        download_time: Duration::from_secs(2),
        tcp_port: 443,
        cf_colo: "LAX,HKG,SJC".to_string(),
        ..Default::default()
    };

    println!("\n=== 1. 直接测试 Worker 节点域名 ===");
    // 通过 DNS 解析获得的 Worker 对应 IP
    let domain_ip = "172.67.180.123".parse().unwrap();
    match client.download_ip(domain_ip, &options).await {
        Ok((speed, colo)) => {
            let speed_mb = speed / (1024.0 * 1024.0);
            println!(
                "✅ 直接测试成功! 测速结果: {:.2} MB/s, 识别节点 Colo: {}",
                speed_mb, colo
            );
            assert!(speed > 0.0);
        }
        Err(e) => panic!("❌ 直接测试失败: {:?}", e),
    }

    println!("\n=== 2. 使用指定 Cloudflare IP 模拟测试 ===");
    let cf_ips = vec!["104.16.1.1", "104.17.2.95", "104.20.4.92"];

    for ip_str in cf_ips {
        let ip = ip_str.parse().unwrap();
        print!("测试指定 IP: {:<15} ... ", ip_str);
        match client.download_ip(ip, &options).await {
            Ok((speed, colo)) => {
                let speed_mb = speed / (1024.0 * 1024.0);
                println!(
                    "成功! 速度: {:6.2} MB/s, 机房 Colo: {}",
                    speed_mb,
                    if colo.is_empty() { "未知" } else { &colo }
                );
                assert!(speed > 0.0);
            }
            Err(e) => panic!("失败: {:?}", e),
        }
    }
}
