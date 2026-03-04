pub mod commands;
pub mod handler;

use std::sync::Arc;

use crate::session::SessionTracker;
use crate::vm::VmManager;

pub struct BotData {
    pub vm_manager: Arc<VmManager>,
    pub sessions: Arc<SessionTracker>,
    pub llm_backend_factory: Arc<dyn LlmBackendFactory>,
}

/// Factory for creating LLM backends (one per session).
pub trait LlmBackendFactory: Send + Sync {
    fn create(&self) -> Box<dyn crate::llm::LlmBackend>;
}
