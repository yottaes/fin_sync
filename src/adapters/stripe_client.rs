use {
    crate::domain::{
        error::PipelineError,
        id::ExternalId,
        money::{Currency, Money, MoneyAmount},
        payment::{PaymentDirection, PaymentStatus},
        provider::{FetchedPayment, PaymentProvider},
    },
    std::{future::Future, pin::Pin},
};

pub struct StripeProvider {
    client: stripe::Client,
}

impl StripeProvider {
    pub fn new(secret_key: &str) -> Self {
        Self {
            client: stripe::Client::new(secret_key),
        }
    }
}

impl PaymentProvider for StripeProvider {
    fn fetch_payment(
        &self,
        id: &ExternalId,
    ) -> Pin<Box<dyn Future<Output = Result<FetchedPayment, PipelineError>> + Send + '_>> {
        let id = id.clone();
        Box::pin(async move { self.fetch_payment_inner(&id).await })
    }
}

impl StripeProvider {
    async fn fetch_payment_inner(&self, id: &ExternalId) -> Result<FetchedPayment, PipelineError> {
        let raw = id.as_str();
        if raw.starts_with("pi_") {
            let pi_id = raw
                .parse::<stripe::PaymentIntentId>()
                .map_err(|e| PipelineError::Provider(format!("invalid PaymentIntent id: {e}")))?;
            let pi = stripe::PaymentIntent::retrieve(&self.client, &pi_id, &[])
                .await
                .map_err(|e| PipelineError::Provider(format!("Stripe API: {e}")))?;

            let currency = convert_currency(pi.currency)?;
            let amount = convert_amount(pi.amount)?;
            let status = convert_pi_status(pi.status);
            let metadata = serde_json::to_value(&pi.metadata)?;

            Ok(FetchedPayment {
                external_id: id.clone(),
                direction: PaymentDirection::Inbound,
                status,
                money: Money::new(amount, currency),
                metadata,
                parent_external_id: None,
                created: pi.created,
            })
        } else if raw.starts_with("re_") {
            let refund_id = raw
                .parse::<stripe::RefundId>()
                .map_err(|e| PipelineError::Provider(format!("invalid Refund id: {e}")))?;
            let refund = stripe::Refund::retrieve(&self.client, &refund_id, &[])
                .await
                .map_err(|e| PipelineError::Provider(format!("Stripe API: {e}")))?;

            let currency = convert_currency(refund.currency)?;
            let amount = convert_amount(refund.amount)?;
            let status = convert_refund_status(refund.status.as_deref());
            let metadata = refund
                .metadata
                .as_ref()
                .map(serde_json::to_value)
                .transpose()?
                .unwrap_or(serde_json::Value::Null);

            let parent_pi_id = refund
                .payment_intent
                .as_ref()
                .map(|e| {
                    ExternalId::new(match e {
                        stripe::Expandable::Id(id) => id.to_string(),
                        stripe::Expandable::Object(pi) => pi.id.to_string(),
                    })
                })
                .transpose()?;

            Ok(FetchedPayment {
                external_id: id.clone(),
                direction: PaymentDirection::Outbound,
                status,
                money: Money::new(amount, currency),
                metadata,
                parent_external_id: parent_pi_id,
                created: refund.created,
            })
        } else {
            Err(PipelineError::Provider(format!(
                "unknown external_id prefix: {raw}"
            )))
        }
    }
}

// ── Conversion helpers (moved from stripe_webhook.rs) ───────────────────────

pub fn convert_currency(c: stripe::Currency) -> Result<Currency, PipelineError> {
    match c {
        stripe::Currency::USD => Ok(Currency::Usd),
        stripe::Currency::EUR => Ok(Currency::Eur),
        stripe::Currency::GBP => Ok(Currency::Gbp),
        stripe::Currency::JPY => Ok(Currency::Jpy),
        other => Err(PipelineError::Validation(format!(
            "unsupported currency: {other:?}"
        ))),
    }
}

pub fn convert_amount(amount: i64) -> Result<MoneyAmount, PipelineError> {
    if amount < 0 {
        return Err(PipelineError::Validation("negative amount".into()));
    }
    MoneyAmount::new(amount)
}

pub fn convert_pi_status(status: stripe::PaymentIntentStatus) -> PaymentStatus {
    #[allow(unreachable_patterns)]
    match status {
        stripe::PaymentIntentStatus::Succeeded => PaymentStatus::Succeeded,
        stripe::PaymentIntentStatus::Canceled => PaymentStatus::Failed,
        stripe::PaymentIntentStatus::Processing
        | stripe::PaymentIntentStatus::RequiresAction
        | stripe::PaymentIntentStatus::RequiresCapture
        | stripe::PaymentIntentStatus::RequiresConfirmation
        | stripe::PaymentIntentStatus::RequiresPaymentMethod => PaymentStatus::Pending,
        other => {
            tracing::warn!("unknown PaymentIntentStatus: {other:?}, defaulting to Pending");
            PaymentStatus::Pending
        }
    }
}

pub fn convert_refund_status(status: Option<&str>) -> PaymentStatus {
    match status {
        Some("succeeded") => PaymentStatus::Refunded,
        Some("failed") | Some("canceled") => PaymentStatus::Failed,
        _ => PaymentStatus::Pending,
    }
}
