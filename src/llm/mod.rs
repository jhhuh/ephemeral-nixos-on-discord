pub mod agent;
pub mod anthropic;
pub mod ollama;
pub mod openai;
pub mod tools;
pub mod traits;

pub use traits::{LlmBackend, LlmResponse, Message, ToolCall, ToolDef};
