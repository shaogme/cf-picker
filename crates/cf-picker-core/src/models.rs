use core::{net::IpAddr, time::Duration};

use alloc::{
    format,
    string::{String, ToString},
    vec,
    vec::Vec,
};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PingData {
    pub ip: IpAddr,
    pub sent: usize,
    pub received: usize,
    pub delay: Duration,
    pub colo: String,
}

impl PingData {
    pub fn loss_rate(&self) -> f32 {
        if self.sent == 0 {
            return 1.0;
        }
        (self.sent - self.received) as f32 / self.sent as f32
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CloudflareIpData {
    pub ping_data: PingData,
    pub download_speed: f64,
}

impl CloudflareIpData {
    pub fn new(ping_data: PingData) -> Self {
        Self {
            ping_data,
            download_speed: 0.0,
        }
    }

    pub fn to_record(&self) -> Vec<String> {
        let loss_rate = self.ping_data.loss_rate();
        let delay_ms = self.ping_data.delay.as_secs_f64() * 1000.0;
        let speed_mb = self.download_speed / 1024.0 / 1024.0;
        let colo = if self.ping_data.colo.is_empty() {
            "N/A".to_string()
        } else {
            self.ping_data.colo.clone()
        };

        vec![
            self.ping_data.ip.to_string(),
            self.ping_data.sent.to_string(),
            self.ping_data.received.to_string(),
            format!("{:.2}", loss_rate),
            format!("{:.2}", delay_ms),
            format!("{:.2}", speed_mb),
            colo,
        ]
    }
}

#[derive(Debug, Clone)]
pub struct PickerOptions {
    pub routines: usize,
    pub ping_times: usize,
    pub test_count: usize,
    pub download_time: Duration,
    pub tcp_port: u16,
    pub url: String,
    pub httping: bool,
    pub httping_status_code: u16,
    pub tcping_timeout: Duration,
    pub httping_timeout: Duration,
    pub cf_colo: String,
    pub max_delay: Duration,
    pub min_delay: Duration,
    pub max_loss_rate: f32,
    pub min_speed: f64,
    pub ip_file: String,
    pub ip_text: String,
    pub disable_download: bool,
    pub test_all: bool,
    pub debug: bool,
}

impl Default for PickerOptions {
    fn default() -> Self {
        Self {
            routines: 200,
            ping_times: 4,
            test_count: 10,
            download_time: Duration::from_secs(10),
            tcp_port: 443,
            url: "https://speed.cloudflare.com/__down?bytes=500000000".to_string(),
            httping: false,
            httping_status_code: 0,
            tcping_timeout: Duration::from_secs(1),
            httping_timeout: Duration::from_secs(2),
            cf_colo: String::new(),
            max_delay: Duration::from_millis(9999),
            min_delay: Duration::from_millis(0),
            max_loss_rate: 1.0,
            min_speed: 0.0,
            ip_file: "ip.txt".to_string(),
            ip_text: String::new(),
            disable_download: false,
            test_all: false,
            debug: false,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_loss_rate_calculation() {
        let ping_data = PingData {
            ip: "1.1.1.1".parse().unwrap(),
            sent: 4,
            received: 3,
            delay: Duration::from_millis(20),
            colo: "HKG".to_string(),
        };
        assert!((ping_data.loss_rate() - 0.25).abs() < f32::EPSILON);
    }
}
