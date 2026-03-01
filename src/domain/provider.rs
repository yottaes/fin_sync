use {
    super::error::PipelineError,
    super::id::ExternalId,
    super::money::Money,
    super::payment::{PaymentDirection, PaymentStatus},
    std::{future::Future, pin::Pin},
};

/// What the service layer gets back after fetching from the provider API.
pub struct FetchedPayment {
    pub external_id: ExternalId,
    pub direction: PaymentDirection,
    pub status: PaymentStatus,
    pub money: Money,
    pub metadata: serde_json::Value,
    pub parent_external_id: Option<ExternalId>,
    pub created: i64,
}

pub trait PaymentProvider: Send + Sync {
    fn fetch_payment(
        &self,
        id: &ExternalId,
    ) -> Pin<Box<dyn Future<Output = Result<FetchedPayment, PipelineError>> + Send + '_>>;
}
