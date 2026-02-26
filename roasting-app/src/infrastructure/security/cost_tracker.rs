use chrono::{DateTime, Utc};
use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::Mutex;

const DAILY_REQUEST_LIMIT: u32 = 100;
const ESTIMATED_COST_PER_REQUEST_CENTS: u32 = 5;
const DAILY_COST_LIMIT_CENTS: u32 = 500;

pub struct CostTracker {
    daily_requests: AtomicU32,
    daily_cost_cents: AtomicU32,
    last_reset: Mutex<DateTime<Utc>>,
}

impl CostTracker {
    pub fn new() -> Self {
        Self {
            daily_requests: AtomicU32::new(0),
            daily_cost_cents: AtomicU32::new(0),
            last_reset: Mutex::new(Utc::now()),
        }
    }

    pub fn check_and_increment(&self) -> Result<(), CostLimitError> {
        self.maybe_reset_daily();

        let current_requests = self.daily_requests.load(Ordering::SeqCst);
        let current_cost = self.daily_cost_cents.load(Ordering::SeqCst);

        if current_requests >= DAILY_REQUEST_LIMIT {
            return Err(CostLimitError::DailyRequestLimitReached);
        }

        if current_cost + ESTIMATED_COST_PER_REQUEST_CENTS > DAILY_COST_LIMIT_CENTS {
            return Err(CostLimitError::DailyCostLimitReached);
        }

        self.daily_requests.fetch_add(1, Ordering::SeqCst);
        self.daily_cost_cents
            .fetch_add(ESTIMATED_COST_PER_REQUEST_CENTS, Ordering::SeqCst);

        Ok(())
    }

    pub fn get_remaining_requests(&self) -> u32 {
        DAILY_REQUEST_LIMIT.saturating_sub(self.daily_requests.load(Ordering::SeqCst))
    }

    fn maybe_reset_daily(&self) {
        let now = Utc::now();
        let mut last_reset = self.last_reset.lock().unwrap();

        if now.date_naive() != last_reset.date_naive() {
            self.daily_requests.store(0, Ordering::SeqCst);
            self.daily_cost_cents.store(0, Ordering::SeqCst);
            *last_reset = now;
            tracing::info!("Daily cost tracker reset");
        }
    }
}

impl Default for CostTracker {
    fn default() -> Self {
        Self::new()
    }
}

#[derive(Debug, Clone)]
pub enum CostLimitError {
    DailyRequestLimitReached,
    DailyCostLimitReached,
}

impl CostLimitError {
    pub fn message_id(&self) -> &'static str {
        match self {
            Self::DailyRequestLimitReached => {
                "Batas harian tercapai. Coba lagi besok ya!"
            }
            Self::DailyCostLimitReached => {
                "Server kehabisan budget hari ini. Coba lagi besok!"
            }
        }
    }
}
