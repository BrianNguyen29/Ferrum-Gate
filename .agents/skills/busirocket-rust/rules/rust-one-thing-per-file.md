# One Thing Per File (STRICT)

## Goal

Enforce strict one-thing-per-file discipline in Rust modules.

## Rule

- **Exactly one exported symbol per file** for your own Rust modules:
  - One public function **or** one public type **or** one public trait per file.
- No helpers inside the same file. Extract helpers to `utils/`-style modules.
- Files should do one thing: data model, boundary call, or pure logic.

## Examples

```rust
// ✅ Correct - one function per file
// src/services/invoices/create_invoice.rs
pub async fn create_invoice(input: CreateInvoiceInput) -> Result<Invoice, InvoiceError> {
    // ...
}
```

```rust
// ✅ Correct - one type per file
// src/models/invoice.rs
pub struct Invoice {
    pub id: String,
    pub amount: f64,
}
```

```rust
// ❌ Incorrect - multiple exports
// src/services/invoices/invoice_service.rs
pub fn create_invoice() { /* ... */ }
pub fn get_invoice() { /* ... */ } // Not allowed!
```

```rust
// ❌ Incorrect - helper in same file
// src/services/invoices/create_invoice.rs
pub async fn create_invoice(input: CreateInvoiceInput) -> Result<Invoice, InvoiceError> {
    // ...
}

fn validate_input(input: &CreateInvoiceInput) -> bool {
    // Helper should be in utils/
}
```

## Best Practices

- One export per file
- Extract helpers to `utils/`
- Keep files focused on one responsibility
