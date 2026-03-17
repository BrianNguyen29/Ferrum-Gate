# Prompt Separation (STRICT)

## Goal

Separate LLM/AI prompts from Rust code.

## Rule

- **No inline LLM or AI prompts** inside `.rs` files.
- Put each prompt in its own file under your prompts directory (e.g.
  `prompts/<area>/Xxx.prompt`; in Tauri projects typically
  `src-tauri/prompts/<area>/Xxx.prompt`).
- Load with `include_str!()` in Rust code.

## Examples

```rust
// ❌ Incorrect - inline prompt
// src/services/ai/generate_summary.rs
pub async fn generate_summary(text: &str) -> Result<String, AIServiceError> {
    let prompt = "Summarize the following text: {text}";
    // Prompt should be in separate file
}
```

```rust
// ✅ Correct - prompt in separate file
// prompts/ai/generate_summary.prompt
Summarize the following text: {text}

// src/services/ai/generate_summary.rs
const GENERATE_SUMMARY_PROMPT: &str = include_str!("../../../prompts/ai/generate_summary.prompt");

pub async fn generate_summary(text: &str) -> Result<String, AIServiceError> {
    let prompt = GENERATE_SUMMARY_PROMPT.replace("{text}", text);
    // Use prompt
}
```

## Best Practices

- Never put prompts inline in Rust code
- One prompt file per prompt
- Use `include_str!()` to load prompt files
