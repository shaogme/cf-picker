use core::{
    cmp::{Ordering, min},
    net::IpAddr,
};

use alloc::{sync::Arc, vec::Vec};

use crate::{
    models::{CloudflareIpData, PickerOptions},
    traits::{DownloadClient, NetworkError},
};

#[derive(Debug, Clone)]
pub enum DownloadEvent {
    Progress { current: usize, total: usize },
    ItemSuccess(CloudflareIpData),
    Error { ip: IpAddr, error: NetworkError },
    Finished(Vec<CloudflareIpData>),
}

pub struct DownloadRunner {
    options: PickerOptions,
}

impl DownloadRunner {
    pub fn new(options: PickerOptions) -> Self {
        Self { options }
    }

    pub async fn run<C, F>(
        &self,
        mut ip_set: Vec<CloudflareIpData>,
        client: &C,
        on_event: Option<F>,
    ) -> Vec<CloudflareIpData>
    where
        C: DownloadClient + ?Sized,
        F: Fn(DownloadEvent) + Send + Sync,
    {
        if self.options.disable_download || ip_set.is_empty() {
            return ip_set;
        }

        let mut test_num = self.options.test_count;
        if ip_set.len() < self.options.test_count || self.options.min_speed > 0.0 {
            test_num = ip_set.len();
        }
        let target_count = min(self.options.test_count, test_num);

        let on_event = on_event.map(Arc::new);
        let mut results = Vec::new();

        for item in ip_set.iter_mut().take(test_num) {
            let success = match client.download_ip(item.ping_data.ip, &self.options).await {
                Ok((speed, colo)) => {
                    item.download_speed = speed;
                    if item.ping_data.colo.is_empty() {
                        item.ping_data.colo = colo;
                    }
                    true
                }
                Err(err) => {
                    if let Some(ref cb) = on_event {
                        cb(DownloadEvent::Error {
                            ip: item.ping_data.ip,
                            error: err,
                        });
                    }
                    false
                }
            };

            if success && item.download_speed >= self.options.min_speed {
                results.push(item.clone());
                if let Some(ref cb) = on_event {
                    cb(DownloadEvent::ItemSuccess(item.clone()));
                    cb(DownloadEvent::Progress {
                        current: results.len(),
                        total: target_count,
                    });
                }
                if results.len() == target_count {
                    break;
                }
            }
        }

        if self.options.min_speed == 0.0 && results.is_empty() {
            results = ip_set;
        }

        results.sort_by(|a, b| {
            b.download_speed
                .partial_cmp(&a.download_speed)
                .unwrap_or(Ordering::Equal)
        });

        if let Some(ref cb) = on_event {
            cb(DownloadEvent::Finished(results.clone()));
        }

        results
    }
}
