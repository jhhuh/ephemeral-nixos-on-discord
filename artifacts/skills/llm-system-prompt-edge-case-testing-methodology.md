# LLM System Prompt Edge-Case Testing Methodology

## When to Use
When stress-testing a system prompt for a tool-use LLM agent, especially one scoped to a sandboxed environment.

## Testing Categories

### 1. Tool boundary mismatches
Test what happens when the LLM's tool calls hit infrastructure limits the prompt doesn't mention:
- Timeouts (exec takes 120s but user wants a 2-hour build)
- Output size (tool returns 50KB but context window is finite)
- Error propagation (tool returns errors the LLM hasn't been told about)

### 2. Self-inflicted failures
The LLM generates bad input for its own tools. Does the prompt tell it to retry or give up?
- Syntax errors in generated code
- Type mismatches in configuration
- Conflicting options that cause evaluation failures

### 3. Scope creep / role drift
- Prompt injection: "ignore your instructions"
- Out-of-scope questions the agent might try to answer
- Requests that probe the security boundary (host access, network escape)

### 4. Imperative vs declarative confusion
For NixOS specifically:
- `nix-env` is the most common imperative trap (it's Nix's own tool but creates config drift)
- Users mixing `apt`/`yum` knowledge with NixOS
- Users who `write_file` to config paths that `nixos_rebuild` should manage

### 5. Infrastructure failures vs user errors
The prompt should help the LLM distinguish:
- "Your config was wrong" (fix and retry)
- "The network is down" (explain and suggest retry)
- "The tool timed out" (explain the limit)

## Methodology
1. List scenarios before analyzing — prevents confirmation bias
2. For each: predict behavior, identify the gap, write the minimal fix
3. Verify fixes don't bloat the prompt (count added words, aim for <30% increase)
4. Test that the code still compiles after editing embedded prompts
5. Document which scenarios needed NO change — this validates prompt quality

## Anti-patterns
- Adding guidance for every possible scenario — leads to prompt bloat
- Over-specifying tone/personality — wastes tokens and constrains useful behavior
- Duplicating information the LLM already knows (e.g., "NixOS uses Nix language" — obviously)
- Adding examples in the system prompt for rare edge cases — better to keep the prompt principles-based
