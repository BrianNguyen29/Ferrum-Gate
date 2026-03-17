# Module Manifests Exception

## Goal

Understand the exception for module manifest files.

## Exception Rule

- `mod.rs` files may declare multiple `mod` entries.
- No logic, constants, or functions inside `mod.rs`.

## Examples

```rust
// ✅ Correct - mod.rs with module declarations
// src/services/invoices/mod.rs
pub mod create_invoice;
pub mod get_invoice;
pub mod update_invoice;
```

```rust
// ❌ Incorrect - logic in mod.rs
// src/services/invoices/mod.rs
pub mod create_invoice;

pub fn helper_function() {
    // Logic should not be in mod.rs
}
```

```rust
// ❌ Incorrect - constants in mod.rs
// src/services/invoices/mod.rs
pub mod create_invoice;

pub const DEFAULT_AMOUNT: f64 = 0.0; // Constants should not be in mod.rs
```

## Best Practices

- Use `mod.rs` only for module declarations
- Keep `mod.rs` files minimal
- No logic, constants, or functions in `mod.rs`
