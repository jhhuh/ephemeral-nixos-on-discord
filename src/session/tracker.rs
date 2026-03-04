use std::collections::HashMap;
use std::sync::Arc;
use std::time::{Duration, Instant};

use tokio::sync::{Mutex, MutexGuard};

use crate::llm::agent::Agent;
use crate::qga::QgaClient;

pub struct Session {
    pub vm_id: String,
    pub thread_id: u64,
    pub user_id: u64,
    pub agent: Agent,
    pub qga: QgaClient,
    pub created_at: Instant,
    pub last_activity: Instant,
}

pub struct SessionTracker {
    sessions: Arc<Mutex<HashMap<u64, Session>>>,
    idle_timeout: Duration,
}

impl SessionTracker {
    pub fn new(idle_timeout: Duration) -> Self {
        Self {
            sessions: Arc::new(Mutex::new(HashMap::new())),
            idle_timeout,
        }
    }

    pub async fn add(&self, thread_id: u64, session: Session) {
        self.sessions.lock().await.insert(thread_id, session);
    }

    pub async fn get_mut<F, R>(&self, thread_id: u64, f: F) -> Option<R>
    where
        F: FnOnce(&mut Session) -> R,
    {
        let mut sessions = self.sessions.lock().await;
        let session = sessions.get_mut(&thread_id)?;
        session.last_activity = Instant::now();
        Some(f(session))
    }

    pub async fn remove(&self, thread_id: u64) -> Option<Session> {
        self.sessions.lock().await.remove(&thread_id)
    }

    pub async fn find_by_vm(&self, vm_id: &str) -> Option<u64> {
        let sessions = self.sessions.lock().await;
        sessions
            .values()
            .find(|s| s.vm_id == vm_id)
            .map(|s| s.thread_id)
    }

    pub async fn expired_sessions(&self) -> Vec<u64> {
        let sessions = self.sessions.lock().await;
        let now = Instant::now();
        sessions
            .iter()
            .filter(|(_, s)| now.duration_since(s.last_activity) > self.idle_timeout)
            .map(|(&tid, _)| tid)
            .collect()
    }

    pub async fn count(&self) -> usize {
        self.sessions.lock().await.len()
    }

    pub async fn count_by_user(&self, user_id: u64) -> usize {
        self.sessions.lock().await.values().filter(|s| s.user_id == user_id).count()
    }

    pub async fn sessions_mut(&self) -> MutexGuard<'_, HashMap<u64, Session>> {
        self.sessions.lock().await
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_expired_sessions() {
        let tracker = SessionTracker::new(Duration::from_millis(50));

        // No expired sessions when empty
        assert!(tracker.expired_sessions().await.is_empty());
        assert_eq!(tracker.count().await, 0);
    }
}
