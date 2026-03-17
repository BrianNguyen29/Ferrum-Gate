# SQL Separation (STRICT)

## Goal

Separate SQL queries from Rust code.

## Rule

- **No inline SQL strings** inside `.rs` files.
- Put each query in its own file under your SQL directory (e.g.
  `sql/<area>/Xxx.sql`; in Tauri projects typically
  `src-tauri/sql/<area>/Xxx.sql`).
- Load with `include_str!()` in Rust code.

## Examples

```rust
// ❌ Incorrect - inline SQL
// src/services/invoices/create_invoice.rs
pub async fn create_invoice(input: CreateInvoiceInput) -> Result<Invoice, InvoiceError> {
    let query = "INSERT INTO invoices (id, amount) VALUES ($1, $2)";
    // SQL should be in separate file
}
```

```rust
// ✅ Correct - SQL in separate file
// sql/invoices/create_invoice.sql
INSERT INTO invoices (id, amount) VALUES ($1, $2);

// src/services/invoices/create_invoice.rs
const CREATE_INVOICE_SQL: &str = include_str!("../../../sql/invoices/create_invoice.sql");

pub async fn create_invoice(input: CreateInvoiceInput) -> Result<Invoice, InvoiceError> {
    // Use CREATE_INVOICE_SQL
}
```

## Best Practices

- Never put SQL strings inline in Rust code
- One SQL file per query
- Use `include_str!()` to load SQL files
