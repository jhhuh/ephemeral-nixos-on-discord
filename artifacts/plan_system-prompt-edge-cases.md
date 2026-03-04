# System Prompt Edge Case Stress Test

## Task
Analyze 12 edge case scenarios against the NixOS Sandbox system prompt.
For each: what happens, what's the problem, what's the fix.
Then produce a surgically improved prompt.

## Scenarios Analyzed
1. Conflicting nixos_rebuild calls (nginx + apache on port 80)
2. nixos_rebuild syntax/type errors
3. Long-running commands (kernel compile, 120s exec timeout)
4. Rapidly repeated messages (race conditions)
5. Non-English user
6. Prompt injection
7. VM state persistence across messages
8. Nix build failures (network/cache issues)
9. User wants flakes (Nix, not NixOS config)
10. Mixed imperative + declarative (config drift)
11. Extremely verbose output (50KB+)
12. User asks about host / security boundary
