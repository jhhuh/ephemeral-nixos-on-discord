# LLM Backends

The bot supports three LLM backends, selected via the `LLM_BACKEND` environment variable.

## Anthropic (default)

```bash
export LLM_BACKEND=anthropic
export LLM_API_KEY=sk-ant-...
```

Uses Claude via the [Anthropic Messages API](https://docs.anthropic.com/en/api/messages). Default model: `claude-sonnet-4-6`.

## OpenAI

```bash
export LLM_BACKEND=openai
export LLM_API_KEY=sk-...
```

Uses the [OpenAI Chat Completions API](https://platform.openai.com/docs/api-reference/chat). Default model: `gpt-4o`.

### OpenAI-Compatible APIs

The OpenAI backend supports custom base URLs for any OpenAI-compatible API:

```bash
export LLM_BACKEND=openai
export LLM_API_KEY=...
export OPENAI_API_BASE=https://api.together.xyz/v1     # Together AI
export OPENAI_API_BASE=https://your-azure-endpoint/     # Azure OpenAI
export OPENAI_API_BASE=http://localhost:8000/v1          # vLLM / local
```

## Ollama (local)

```bash
export LLM_BACKEND=ollama
# No API key needed
```

Uses a local [Ollama](https://ollama.ai) instance. No API key required.

| Variable | Default | Description |
|----------|---------|-------------|
| `OLLAMA_BASE_URL` | `http://localhost:11434` | Ollama server URL |
| `OLLAMA_MODEL` | `llama3.1` | Model to use |

!!! note "Tool use support"
    Not all Ollama models support tool use (function calling). Use models that support it, such as `llama3.1`, `mistral`, or `qwen2.5`.

## Adding a Custom Backend

Implement the `LlmBackend` trait:

```rust
#[async_trait::async_trait]
pub trait LlmBackend: Send + Sync {
    async fn chat(
        &self,
        messages: &[Message],
        tools: &[ToolDef],
    ) -> Result<LlmResponse, Box<dyn std::error::Error + Send + Sync>>;
}
```

Then add a factory in `src/main.rs` and a match arm for `LLM_BACKEND`.
