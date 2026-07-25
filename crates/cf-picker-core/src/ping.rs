use core::{
    cmp::Ordering,
    net::IpAddr,
    sync::atomic::{AtomicUsize, Ordering as AtomicOrdering},
    time::Duration,
};

use alloc::{string::String, sync::Arc, vec::Vec};
use futures_util::{StreamExt, future::ready, stream};

use crate::{
    colo::ColoMatcher,
    models::{CloudflareIpData, PickerOptions, PingData},
    traits::{HttpingResponse, NetworkError, PingClient, Rand},
};

#[derive(Debug, Clone)]
pub enum PingEvent {
    Progress {
        total: usize,
        completed: usize,
        available: usize,
    },
    Error {
        ip: Option<IpAddr>,
        error: NetworkError,
    },
    Finished(Vec<CloudflareIpData>),
}

pub struct PingRunner {
    options: PickerOptions,
}

impl PingRunner {
    pub fn new(options: PickerOptions) -> Self {
        Self { options }
    }

    pub async fn run<C, F, G>(
        &self,
        ips: &[IpAddr],
        client: &C,
        on_event: Option<F>,
        rng: &mut G,
    ) -> Vec<CloudflareIpData>
    where
        C: PingClient + ?Sized,
        F: Fn(PingEvent) + Send + Sync,
        G: Rand + ?Sized,
    {
        if ips.is_empty() {
            return Vec::new();
        }

        let total = ips.len();
        let colo_matcher = ColoMatcher::new(&self.options.cf_colo, rng);
        let completed = Arc::new(AtomicUsize::new(0));
        let available = Arc::new(AtomicUsize::new(0));
        let on_event = on_event.map(Arc::new);

        let stream_futures = ips.iter().map(|&ip| {
            let options = &self.options;
            let matcher = &colo_matcher;
            let completed = Arc::clone(&completed);
            let available = Arc::clone(&available);
            let on_event = on_event.clone();

            async move {
                let (recv, total_delay, colo) = if options.httping {
                    httping_ip(ip, options, matcher, client, on_event.as_deref()).await
                } else {
                    tcping_ip(ip, options, client, on_event.as_deref()).await
                };

                let is_avail = recv > 0;
                let current_comp = completed.fetch_add(1, AtomicOrdering::SeqCst) + 1;
                let current_avail = if is_avail {
                    available.fetch_add(1, AtomicOrdering::SeqCst) + 1
                } else {
                    available.load(AtomicOrdering::SeqCst)
                };

                if let Some(ref cb) = on_event {
                    cb(PingEvent::Progress {
                        total,
                        completed: current_comp,
                        available: current_avail,
                    });
                }

                if is_avail {
                    let avg_delay = total_delay / (recv as u32);
                    Some(CloudflareIpData::new(PingData {
                        ip,
                        sent: options.ping_times,
                        received: recv,
                        delay: avg_delay,
                        colo,
                    }))
                } else {
                    None
                }
            }
        });

        let mut results: Vec<CloudflareIpData> = stream::iter(stream_futures)
            .buffer_unordered(self.options.routines)
            .filter_map(ready)
            .collect()
            .await;

        results = filter_delay(results, self.options.min_delay, self.options.max_delay);
        results = filter_loss_rate(results, self.options.max_loss_rate);

        results.sort_by(|a, b| {
            let a_loss = a.ping_data.loss_rate();
            let b_loss = b.ping_data.loss_rate();
            a_loss
                .partial_cmp(&b_loss)
                .unwrap_or(Ordering::Equal)
                .then_with(|| a.ping_data.delay.cmp(&b.ping_data.delay))
        });

        if let Some(ref cb) = on_event {
            cb(PingEvent::Finished(results.clone()));
        }

        results
    }
}

fn filter_delay(
    data: Vec<CloudflareIpData>,
    min_delay: Duration,
    max_delay: Duration,
) -> Vec<CloudflareIpData> {
    data.into_iter()
        .filter(|item| item.ping_data.delay >= min_delay && item.ping_data.delay <= max_delay)
        .collect()
}

fn filter_loss_rate(data: Vec<CloudflareIpData>, max_loss_rate: f32) -> Vec<CloudflareIpData> {
    data.into_iter()
        .filter(|item| item.ping_data.loss_rate() <= max_loss_rate)
        .collect()
}

async fn tcping_ip<C: PingClient + ?Sized, F: Fn(PingEvent) + Send + Sync>(
    ip: IpAddr,
    options: &PickerOptions,
    client: &C,
    on_event: Option<&F>,
) -> (usize, Duration, String) {
    let mut recv = 0;
    let mut total_delay = Duration::ZERO;

    for _ in 0..options.ping_times {
        match client
            .tcping(ip, options.tcp_port, options.tcping_timeout)
            .await
        {
            Ok(delay) => {
                recv += 1;
                total_delay += delay;
            }
            Err(err) => {
                if matches!(err, NetworkError::Other(_))
                    && let Some(cb) = on_event {
                        cb(PingEvent::Error {
                            ip: Some(ip),
                            error: err,
                        });
                    }
            }
        }
    }

    (recv, total_delay, String::new())
}

async fn httping_ip<C: PingClient + ?Sized, F: Fn(PingEvent) + Send + Sync>(
    ip: IpAddr,
    options: &PickerOptions,
    matcher: &ColoMatcher,
    client: &C,
    on_event: Option<&F>,
) -> (usize, Duration, String) {
    let (resp, _) = match client.httping(ip, options, options.httping_timeout).await {
        Ok(res) => res,
        Err(err) => {
            if matches!(err, NetworkError::Other(_) | NetworkError::HttpStatus(_))
                && let Some(cb) = on_event {
                    cb(PingEvent::Error {
                        ip: Some(ip),
                        error: err,
                    });
                }
            return (0, Duration::ZERO, String::new());
        }
    };

    let status = resp.status();
    let is_valid_status = if options.httping_status_code == 0
        || options.httping_status_code < 100
        || options.httping_status_code > 599
    {
        status == 200 || status == 301 || status == 302
    } else {
        status == options.httping_status_code
    };

    if !is_valid_status {
        if let Some(cb) = on_event {
            cb(PingEvent::Error {
                ip: Some(ip),
                error: NetworkError::HttpStatus(status),
            });
        }
        return (0, Duration::ZERO, String::new());
    }

    let colo = matcher.extract_colo(&resp);
    if !options.cf_colo.is_empty() && !matcher.is_match(&colo) {
        return (0, Duration::ZERO, String::new());
    }

    let mut recv = 0;
    let mut total_delay = Duration::ZERO;

    for _ in 0..options.ping_times {
        match client.httping(ip, options, options.httping_timeout).await {
            Ok((_, delay)) => {
                recv += 1;
                total_delay += delay;
            }
            Err(err) => {
                if matches!(err, NetworkError::Other(_) | NetworkError::HttpStatus(_))
                    && let Some(cb) = on_event {
                        cb(PingEvent::Error {
                            ip: Some(ip),
                            error: err,
                        });
                    }
            }
        }
    }

    (recv, total_delay, colo)
}
