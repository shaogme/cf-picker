use core::{fmt, future::Future, net::IpAddr, time::Duration};

use alloc::string::String;

use crate::models::PickerOptions;

pub trait Rand {
    fn next_u64(&mut self) -> u64;

    fn random_range_usize(&mut self, min: usize, max: usize) -> usize {
        if min >= max {
            return min;
        }
        let range = max as u128 - min as u128 + 1;
        (min as u128 + (self.next_u64() as u128 % range)) as usize
    }

    fn random_range_u8(&mut self, min: u8, max: u8) -> u8 {
        if min >= max {
            return min;
        }
        let range = max as u64 - min as u64 + 1;
        min + (self.next_u64() % range) as u8
    }
}

#[derive(Debug, Clone)]
pub enum NetworkError {
    Timeout,
    ConnectionFailed(String),
    HttpStatus(u16),
    Other(String),
}

impl fmt::Display for NetworkError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Timeout => write!(f, "请求超时"),
            Self::ConnectionFailed(msg) => write!(f, "连接失败: {msg}"),
            Self::HttpStatus(code) => write!(f, "HTTP 状态码错误: {code}"),
            Self::Other(msg) => write!(f, "{msg}"),
        }
    }
}

impl core::error::Error for NetworkError {}

pub trait HttpingResponse {
    fn status(&self) -> u16;
    fn header(&self, name: &str) -> Option<&str>;
}

pub trait PingClient: Send + Sync {
    type Response: HttpingResponse;

    fn tcping(
        &self,
        ip: IpAddr,
        port: u16,
        timeout: Duration,
    ) -> impl Future<Output = Result<Duration, NetworkError>> + Send;

    fn httping(
        &self,
        ip: IpAddr,
        options: &PickerOptions,
        timeout_dur: Duration,
    ) -> impl Future<Output = Result<(Self::Response, Duration), NetworkError>> + Send;
}

pub trait DownloadClient: Send + Sync {
    fn download_ip(
        &self,
        ip: IpAddr,
        options: &PickerOptions,
    ) -> impl Future<Output = Result<(f64, String), NetworkError>> + Send;
}

pub trait LineReader {
    type Error: core::error::Error + Send + Sync + 'static;

    fn read_line(&mut self, buf: &mut String) -> Result<usize, Self::Error>;
}

#[cfg(test)]
mod tests {
    use super::*;

    struct DummyRand;
    impl Rand for DummyRand {
        fn next_u64(&mut self) -> u64 {
            u64::MAX
        }
    }

    #[test]
    fn test_random_range_max_bounds() {
        let mut rng = DummyRand;
        assert_eq!(rng.random_range_u8(0, 255), 255);
        assert_eq!(rng.random_range_usize(0, usize::MAX), usize::MAX);
    }
}
