# self-llm

A unified Rust chat API for OpenAI-compatible and Anthropic-compatible providers.

- 中文文档: [README.zh-CN.md](README.zh-CN.md)
- English documentation: [README.en.md](README.en.md)

## Quick Links

- Public API: `src/lib.rs`
- Main client: `src/client.rs`
- Unified types: `src/types.rs`
- OpenAI adapter: `src/openai/mod.rs`
- Anthropic adapter: `src/anthropic/mod.rs`
- Integration examples: `tests/integration_test.rs`

## Validation

```bash
cargo clippy --all-targets --all-features -- -D warnings
cargo test --locked
```