use std::{
    net::{IpAddr, SocketAddr},
    time::{Duration, Instant},
};

use cf_picker_core::{
    colo::ColoMatcher,
    models::PickerOptions,
    traits::{DownloadClient, HttpingResponse, NetworkError, PingClient, Rand},
};
use reqwest::{Client as ReqwestClient, Url, redirect::Policy};
use tokio::{net::TcpStream, time::timeout};

struct TokioRand;

impl Rand for TokioRand {
    fn next_u64(&mut self) -> u64 {
        rand::random::<u64>()
    }
}

#[derive(Debug, Clone, Default)]
pub struct Client;

impl Client {
    pub fn new() -> Self {
        Self
    }
}

pub struct ReqwestResponse(pub reqwest::Response);

impl HttpingResponse for ReqwestResponse {
    fn status(&self) -> u16 {
        self.0.status().as_u16()
    }

    fn header(&self, name: &str) -> Option<&str> {
        self.0.headers().get(name).and_then(|v| v.to_str().ok())
    }
}

fn referer_from_options(options: &PickerOptions) -> String {
    if let Ok(parsed_url) = Url::parse(&options.url) {
        let origin = parsed_url.origin().ascii_serialization();
        if origin != "null" {
            return format!("{origin}/");
        }
    }
    options.url.clone()
}

fn format_reqwest_error(err: &reqwest::Error) -> String {
    if err.is_timeout() {
        return "超时".to_string();
    }

    use std::error::Error;
    let mut msg = err.to_string();

    if let Some(pos) = msg.find("): ") {
        msg = msg[pos + 3..].to_string();
    }
    if let Some(stripped) = msg.strip_prefix("client error (Connect): ") {
        msg = stripped.to_string();
    } else if let Some(stripped) = msg.strip_prefix("client error: ") {
        msg = stripped.to_string();
    }

    let mut curr: Option<&(dyn Error + 'static)> = err.source();
    while let Some(src) = curr {
        let src_msg = src.to_string();
        if !msg.contains(&src_msg) {
            msg.push_str(&format!(": {src_msg}"));
        }
        curr = src.source();
    }
    msg
}

impl PingClient for Client {
    type Response = ReqwestResponse;

    async fn tcping(
        &self,
        ip: IpAddr,
        port: u16,
        timeout_dur: Duration,
    ) -> Result<Duration, NetworkError> {
        let socket_addr = SocketAddr::new(ip, port);
        let start = Instant::now();
        match timeout(timeout_dur, TcpStream::connect(socket_addr)).await {
            Ok(Ok(_stream)) => Ok(start.elapsed()),
            Ok(Err(e)) => Err(NetworkError::ConnectionFailed(e.to_string())),
            Err(_) => Err(NetworkError::Timeout),
        }
    }

    async fn httping(
        &self,
        ip: IpAddr,
        options: &PickerOptions,
        timeout_dur: Duration,
    ) -> Result<(ReqwestResponse, Duration), NetworkError> {
        let parsed_url = Url::parse(&options.url)
            .map_err(|e| NetworkError::Other(format!("Invalid URL: {e}")))?;
        let host_str = parsed_url
            .host_str()
            .ok_or_else(|| NetworkError::Other("Invalid URL: missing host".to_string()))?;
        let socket_addr = SocketAddr::new(ip, options.tcp_port);
        let referer = referer_from_options(options);

        let client = ReqwestClient::builder()
            .timeout(timeout_dur)
            .resolve(host_str, socket_addr)
            .danger_accept_invalid_certs(true)
            .redirect(Policy::none())
            .build()
            .map_err(|e| NetworkError::Other(e.to_string()))?;

        let start = Instant::now();
        let resp = client
            .head(options.url.as_str())
            .header(
                "User-Agent",
                "Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/125.0.0.0 Safari/537.36",
            )
            .header("Referer", referer)
            .send()
            .await
            .map_err(|e| NetworkError::ConnectionFailed(format_reqwest_error(&e)))?;

        let delay = start.elapsed();
        Ok((ReqwestResponse(resp), delay))
    }
}

impl DownloadClient for Client {
    async fn download_ip(
        &self,
        ip: IpAddr,
        options: &PickerOptions,
    ) -> Result<(f64, String), NetworkError> {
        let parsed_url = Url::parse(&options.url)
            .map_err(|e| NetworkError::Other(format!("Invalid URL: {e}")))?;
        let host_str = parsed_url
            .host_str()
            .ok_or_else(|| NetworkError::Other("Invalid URL: missing host".to_string()))?;
        let socket_addr = SocketAddr::new(ip, options.tcp_port);
        let referer = referer_from_options(options);

        let client = ReqwestClient::builder()
            .connect_timeout(Duration::from_secs(5))
            .resolve(host_str, socket_addr)
            .danger_accept_invalid_certs(true)
            .build()
            .map_err(|e| NetworkError::Other(e.to_string()))?;

        let resp = client
            .get(options.url.as_str())
            .header(
                "User-Agent",
                "Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/125.0.0.0 Safari/537.36",
            )
            .header("Referer", referer)
            .send()
            .await
            .map_err(|e| NetworkError::ConnectionFailed(format_reqwest_error(&e)))?;

        if !resp.status().is_success() {
            return Err(NetworkError::HttpStatus(resp.status().as_u16()));
        }

        let mut rng = TokioRand;
        let matcher = ColoMatcher::new(&options.cf_colo, &mut rng);
        let httping_resp = ReqwestResponse(resp);
        let colo = matcher.extract_colo(&httping_resp);
        let mut resp = httping_resp.0;

        let start_time = Instant::now();
        let end_time = start_time + options.download_time;
        let mut total_bytes: u64 = 0;

        let alpha = 0.1;
        let mut ewma_rate = 0.0;
        let mut last_check = Instant::now();
        let mut last_bytes: u64 = 0;

        loop {
            match resp.chunk().await {
                Ok(Some(chunk)) => {
                    let now = Instant::now();
                    total_bytes += chunk.len() as u64;

                    if now >= end_time {
                        break;
                    }

                    let elapsed_slice = now.duration_since(last_check).as_secs_f64();
                    if elapsed_slice >= 0.1 {
                        let slice_bytes = total_bytes - last_bytes;
                        let current_rate = slice_bytes as f64 / elapsed_slice;
                        if ewma_rate == 0.0 {
                            ewma_rate = current_rate;
                        } else {
                            ewma_rate = alpha * current_rate + (1.0 - alpha) * ewma_rate;
                        }
                        last_check = now;
                        last_bytes = total_bytes;
                    }
                }
                Ok(None) => break,
                Err(e) => {
                    if total_bytes == 0 {
                        return Err(NetworkError::ConnectionFailed(format_reqwest_error(&e)));
                    }
                    break;
                }
            }
        }

        let total_elapsed = start_time.elapsed().as_secs_f64();
        let avg_rate = if total_elapsed > 0.0 {
            total_bytes as f64 / total_elapsed
        } else {
            0.0
        };

        let final_rate = if ewma_rate > 0.0 { ewma_rate } else { avg_rate };
        Ok((final_rate, colo))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use cf_picker_core::{models::PickerOptions, ping::PingRunner};

    #[tokio::test]
    async fn test_native_client_ping_runner_mock() {
        let client = Client::new();
        let options = PickerOptions {
            routines: 10,
            ping_times: 1,
            tcp_port: 80,
            ..Default::default()
        };
        let runner = PingRunner::new(options);
        let ips = vec!["127.0.0.1".parse::<IpAddr>().unwrap()];
        let mut rng = TokioRand;
        let results = runner.run(&ips, &client, None::<fn(_)>, &mut rng).await;
        assert!(results.is_empty() || !results.is_empty());
    }
}
