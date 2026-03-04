use std::collections::HashMap;
use std::time::{Duration, Instant};

use tokio::sync::Mutex;

pub struct RateLimiter {
    /// user_id -> list of VM creation timestamps
    history: Mutex<HashMap<u64, Vec<Instant>>>,
    /// Max concurrent VMs per user
    max_per_user: usize,
    /// Cooldown between VM creations
    cooldown: Duration,
}

impl RateLimiter {
    pub fn new(max_per_user: usize, cooldown: Duration) -> Self {
        Self {
            history: Mutex::new(HashMap::new()),
            max_per_user,
            cooldown,
        }
    }

    /// Check if a user can create a VM. Returns Ok(()) or Err with reason.
    pub async fn check(&self, user_id: u64, active_count: usize) -> Result<(), String> {
        let mut history = self.history.lock().await;
        let entries = history.entry(user_id).or_default();

        // Clean old entries (older than 1 hour)
        let cutoff = Instant::now() - Duration::from_secs(3600);
        entries.retain(|t| *t > cutoff);

        // Check concurrent limit
        if active_count >= self.max_per_user {
            return Err(format!(
                "You already have {} active sandbox(es). Max is {}.",
                active_count, self.max_per_user
            ));
        }

        // Check cooldown
        if let Some(last) = entries.last() {
            let elapsed = last.elapsed();
            if elapsed < self.cooldown {
                let remaining = self.cooldown - elapsed;
                return Err(format!(
                    "Please wait {}s before creating another sandbox.",
                    remaining.as_secs()
                ));
            }
        }

        Ok(())
    }

    /// Record that a user created a VM.
    pub async fn record(&self, user_id: u64) {
        let mut history = self.history.lock().await;
        history.entry(user_id).or_default().push(Instant::now());
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_allows_first_creation() {
        let rl = RateLimiter::new(2, Duration::from_secs(30));
        assert!(rl.check(12345, 0).await.is_ok());
    }

    #[tokio::test]
    async fn test_blocks_over_limit() {
        let rl = RateLimiter::new(2, Duration::from_secs(0));
        assert!(rl.check(12345, 2).await.is_err());
    }

    #[tokio::test]
    async fn test_cooldown() {
        let rl = RateLimiter::new(5, Duration::from_secs(60));
        rl.record(12345).await;
        let result = rl.check(12345, 0).await;
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("wait"));
    }
}
