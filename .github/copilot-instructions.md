# Copilot Instructions

## Project Overview

This repository is a Rust library that exposes a unified chat API over multiple LLM providers.

- Public API surface: `src/lib.rs`
- Main client entry point: `src/client.rs`
- Unified request/response types: `src/types.rs`
- Provider configuration builders: `src/config.rs`
- OpenAI adapter: `src/openai/mod.rs`
- Anthropic adapter: `src/anthropic/mod.rs`
- Shared SSE parsing: `src/sse.rs`
- Integration coverage and usage examples: `tests/integration_test.rs`

## Working Style

- Keep changes minimal and localized.
- Preserve the provider-agnostic API unless the task explicitly requires a public API change.
- Prefer fixing behavior inside `src/openai` or `src/anthropic` before changing unified types in `src/types.rs`.
- Match the existing builder-style API used by `ChatRequest`, `LlmConfig`, and `LlmProviderConfig`.
- Do not add new dependencies unless the current standard library and existing crates are insufficient.

## Build And Validation

Use these commands as the default validation flow:

```bash
cargo clippy --all-targets --all-features -- -D warnings
cargo test --locked
```

Additional notes:

- `cargo test --locked` is used by CI publishing workflow.
- Integration tests in `tests/integration_test.rs` load secrets from `.env` via `dotenvy` and may call live provider APIs.
- If a task does not require live API verification, prefer targeted unit-level validation or compile/lint checks.

## Architecture Notes

- `Client` dispatches requests by provider enum and owns the shared `reqwest::Client`.
- Unified domain models live in `src/types.rs`; provider modules are responsible for converting to and from vendor-specific payloads.
- Streaming support flows through `src/sse.rs` and provider-specific stream event conversion.
- Tool calls and tool results are represented in the unified content model and then mapped into provider-specific request formats.

## Editing Guidance

- When changing unified message/content behavior, review both provider adapters and the integration tests.
- When changing streaming behavior, inspect both provider stream conversion logic and `StreamEvent` handling.
- When changing config behavior, keep the fluent builder API intact.
- Avoid broad refactors unless the user explicitly asks for them.

## Release Workflow

- Publishing is handled by `.github/workflows/publish.yml`.
- A push tag matching `v*` triggers publish.
- The workflow verifies that the tag version matches the crate version in `Cargo.toml` before running `cargo publish --locked`.
- Publishing requires the GitHub Actions secret `CRATES_TOKEN`.

## Useful Starting Points

- For API surface changes: start in `src/lib.rs`, `src/types.rs`, and `src/client.rs`.
- For OpenAI-specific issues: start in `src/openai/mod.rs` and `src/openai/types.rs`.
- For Anthropic-specific issues: start in `src/anthropic/mod.rs` and `src/anthropic/types.rs`.
- For examples of realistic usage, tool-calling, and streaming expectations: see `tests/integration_test.rs`.