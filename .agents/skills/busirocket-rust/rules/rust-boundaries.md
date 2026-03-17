# Boundaries

## Goal

Keep handlers thin (e.g. HTTP route handlers, Tauri command handlers).

## Rule

Handlers should be thin:

- Validate input
- Call a single service function
- Return explicit results

## Examples

```rust
// ✅ Correct - thin handler
// src/commands/create_invoice.rs (or route handler)
use crate::services::invoices::create_invoice;
use crate::models::CreateInvoiceInput;

pub async fn create_invoice_handler(input: CreateInvoiceInput) -> Result<Invoice, String> {
    // Validate input
    if input.amount <= 0.0 {
        return Err("Amount must be positive".to_string());
    }

    // Call service
    create_invoice(input)
        .await
        .map_err(|e| e.to_string())
}
```

```rust
// ❌ Incorrect - fat handler
pub async fn create_invoice_handler(input: CreateInvoiceInput) -> Result<Invoice, String> {
    // Business logic should be in service
    let invoice = Invoice {
        id: generate_id(),
        amount: calculate_tax(input.amount),
        created_at: chrono::Utc::now(),
    };

    // DB access should be in service
    db.invoices.insert(&invoice).await?;

    Ok(invoice)
}
```

## Best Practices

- Keep handlers thin: validate, call service, return
- Move business logic to services
- Keep handlers focused on transport/command concerns
