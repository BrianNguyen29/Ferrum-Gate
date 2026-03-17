# Language & Style

## Goal

Establish consistent Rust language and style conventions.

## Language & Style Rules

- English-only code, comments, and identifiers.
- Prefer `struct`/`enum` definitions in dedicated files.
- Use explicit error types or result enums for expected failures.

## Examples

```rust
// ✅ Correct - English identifiers
// src/models/invoice.rs
pub struct Invoice {
    pub id: String,
    pub amount: f64,
}
```

```rust
// ❌ Incorrect - non-English identifiers
// src/models/factura.rs
pub struct Factura {
    pub id: String,
    pub monto: f64,
}
```

```rust
// ✅ Correct - struct in dedicated file
// src/models/invoice.rs
pub struct Invoice {
    pub id: String,
    pub amount: f64,
}
```

```rust
// ✅ Correct - explicit error type
// src/models/invoice_error.rs
#[derive(Debug)]
pub enum InvoiceError {
    NotFound,
    InvalidAmount,
    DatabaseError(String),
}
```

## Best Practices

- Use English for all code, comments, and identifiers
- Prefer struct/enum definitions in dedicated files
- Use explicit error types for expected failures
