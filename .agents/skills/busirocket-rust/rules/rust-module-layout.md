# Module Layout (STRICT)

## Goal

Establish strict module layout conventions.

## Module Layout (STRICT)

- `src/` is for Rust code only.
- `src/services/`: external boundaries (IO, DB, network).
- `src/utils/`: pure logic (no IO).
- `src/models/`: domain types (one type per file).
- No "misc" modules like `helpers.rs` or `common.rs`.

In Tauri projects, this layout lives under `src-tauri/src/` (see
`busirocket-tauri` skill).

## Examples

```rust
// ✅ Correct - service in services/
// src/services/invoices/create_invoice.rs
pub async fn create_invoice(input: CreateInvoiceInput) -> Result<Invoice, InvoiceError> {
    // External boundary (DB access)
}
```

```rust
// ✅ Correct - pure logic in utils/
// src/utils/invoices/format_amount.rs
pub fn format_amount(amount: f64) -> String {
    format!("${:.2}", amount)
}
```

```rust
// ✅ Correct - type in models/
// src/models/invoice.rs
pub struct Invoice {
    pub id: String,
    pub amount: f64,
}
```

```rust
// ❌ Incorrect - misc module
// src/helpers.rs
pub fn format_amount() { /* ... */ }
pub fn validate_input() { /* ... */ }
pub fn parse_date() { /* ... */ }
```

## Best Practices

- Keep services, utils, and models separate
- No misc modules
- One type per file in models/
