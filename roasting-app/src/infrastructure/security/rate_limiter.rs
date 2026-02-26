use dashmap::DashMap;
use std::net::IpAddr;
use std::sync::Arc;
use std::time::{Duration, Instant};

const MAX_REQUESTS_PER_MINUTE: u32 = 5;
const MAX_REQUESTS_PER_HOUR: u32 = 20;
const CLEANUP_INTERVAL_SECS: u64 = 300;

#[derive(Clone)]
struct RequestRecord {
    minute_count: u32,
    hour_count: u32,
    minute_start: Instant,
    hour_start: Instant,
}

impl Default for RequestRecord {
    fn default() -> Self {
        let now = Instant::now();
        Self {
            minute_count: 0,
            hour_count: 0,
            minute_start: now,
            hour_start: now,
        }
    }
}

#[derive(Clone)]
pub struct RateLimiter {
    requests: Arc<DashMap<IpAddr, RequestRecord>>,
    last_cleanup: Arc<std::sync::Mutex<Instant>>,
}

impl RateLimiter {
    pub fn new() -> Self {
        Self {
            requests: Arc::new(DashMap::new()),
            last_cleanup: Arc::new(std::sync::Mutex::new(Instant::now())),
        }
    }

    pub fn check_rate_limit(&self, ip: IpAddr) -> Result<(), RateLimitError> {
        self.maybe_cleanup();

        let now = Instant::now();
        let mut record = self.requests.entry(ip).or_default();

        if now.duration_since(record.minute_start) > Duration::from_secs(60) {
            record.minute_count = 0;
            record.minute_start = now;
        }

        if now.duration_since(record.hour_start) > Duration::from_secs(3600) {
            record.hour_count = 0;
            record.hour_start = now;
        }

        if record.minute_count >= MAX_REQUESTS_PER_MINUTE {
            let wait_secs = 60 - now.duration_since(record.minute_start).as_secs();
            return Err(RateLimitError::TooManyRequestsPerMinute(wait_secs));
        }

        if record.hour_count >= MAX_REQUESTS_PER_HOUR {
            let wait_secs = 3600 - now.duration_since(record.hour_start).as_secs();
            return Err(RateLimitError::TooManyRequestsPerHour(wait_secs));
        }

        record.minute_count += 1;
        record.hour_count += 1;

        Ok(())
    }

    fn maybe_cleanup(&self) {
        let mut last_cleanup = self.last_cleanup.lock().unwrap();
        if last_cleanup.elapsed() > Duration::from_secs(CLEANUP_INTERVAL_SECS) {
            let cutoff = Instant::now() - Duration::from_secs(3600);
            self.requests.retain(|_, v| v.hour_start > cutoff);
            *last_cleanup = Instant::now();
        }
    }
}

impl Default for RateLimiter {
    fn default() -> Self {
        Self::new()
    }
}

#[derive(Debug, Clone)]
pub enum RateLimitError {
    TooManyRequestsPerMinute(u64),
    TooManyRequestsPerHour(u64),
}

impl RateLimitError {
    pub fn message_id(&self) -> String {
        match self {
            Self::TooManyRequestsPerMinute(secs) => {
                format!("Terlalu banyak request! Tunggu {} detik lagi.", secs)
            }
            Self::TooManyRequestsPerHour(secs) => {
                format!(
                    "Kamu sudah mencapai batas per jam. Tunggu {} menit lagi.",
                    secs / 60
                )
            }
        }
    }
}
