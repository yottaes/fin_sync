use {
    super::audit::NewAuditEntry,
    super::error::PipelineError,
    super::money::Money,
    serde::{Deserialize, Serialize},
    std::fmt,
    uuid::Uuid,
};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum PaymentStatus {
    Succeeded,
    Failed,
    Pending,
    Refunded,
}

impl PaymentStatus {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Succeeded => "succeeded",
            Self::Failed => "failed",
            Self::Pending => "pending",
            Self::Refunded => "refunded",
        }
    }

    /// Exhaustive transition table. Every allowed edge is listed explicitly.
    /// If it's not here, it's not allowed.
    pub fn can_transition_to(&self, new: &Self) -> bool {
        matches!(
            (self, new),
            (Self::Pending, Self::Succeeded)
                | (Self::Pending, Self::Failed)
                | (Self::Succeeded, Self::Refunded)
        )
    }
}

impl fmt::Display for PaymentStatus {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.as_str())
    }
}

impl TryFrom<&str> for PaymentStatus {
    type Error = PipelineError;

    fn try_from(s: &str) -> Result<Self, Self::Error> {
        match s {
            "succeeded" => Ok(Self::Succeeded),
            "failed" => Ok(Self::Failed),
            "pending" => Ok(Self::Pending),
            "refunded" => Ok(Self::Refunded),
            other => Err(PipelineError::Validation(format!(
                "unknown payment status: {other}"
            ))),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum PaymentDirection {
    Inbound,
    Outbound,
}

impl PaymentDirection {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Inbound => "inbound",
            Self::Outbound => "outbound",
        }
    }
}

impl TryFrom<&str> for PaymentDirection {
    type Error = PipelineError;

    fn try_from(s: &str) -> Result<Self, Self::Error> {
        match s {
            "inbound" => Ok(Self::Inbound),
            "outbound" => Ok(Self::Outbound),
            other => Err(PipelineError::Validation(format!(
                "unknown payment direction: {other}"
            ))),
        }
    }
}

/// For INSERT — id generated in Rust via Uuid::now_v7().
#[derive(Debug, Clone)]
pub struct NewPayment {
    id: Uuid,
    external_id: String,
    source: String,
    event_type: String,
    direction: PaymentDirection,
    money: Money,
    status: PaymentStatus,
    metadata: serde_json::Value,
    raw_event: serde_json::Value,
    last_event_id: String,
    parent_external_id: Option<String>,
    stripe_created: i64,
}

impl NewPayment {
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        id: Uuid,
        external_id: String,
        source: String,
        event_type: String,
        direction: PaymentDirection,
        money: Money,
        status: PaymentStatus,
        metadata: serde_json::Value,
        raw_event: serde_json::Value,
        last_event_id: String,
        parent_external_id: Option<String>,
        stripe_created: i64,
    ) -> Self {
        Self {
            id,
            external_id,
            source,
            event_type,
            direction,
            money,
            status,
            metadata,
            raw_event,
            last_event_id,
            parent_external_id,
            stripe_created,
        }
    }

    pub fn id(&self) -> Uuid {
        self.id
    }

    pub fn external_id(&self) -> &str {
        &self.external_id
    }

    pub fn source(&self) -> &str {
        &self.source
    }

    pub fn event_type(&self) -> &str {
        &self.event_type
    }

    pub fn direction(&self) -> &PaymentDirection {
        &self.direction
    }

    pub fn money(&self) -> &Money {
        &self.money
    }

    pub fn status(&self) -> &PaymentStatus {
        &self.status
    }

    pub fn metadata(&self) -> &serde_json::Value {
        &self.metadata
    }

    pub fn raw_event(&self) -> &serde_json::Value {
        &self.raw_event
    }

    pub fn last_event_id(&self) -> &str {
        &self.last_event_id
    }

    pub fn parent_external_id(&self) -> Option<&str> {
        self.parent_external_id.as_deref()
    }

    pub fn stripe_created(&self) -> i64 {
        self.stripe_created
    }

    pub fn audit_entry(&self, actor: &str, action: &str) -> NewAuditEntry {
        NewAuditEntry {
            id: Uuid::now_v7(),
            entity_type: "payment".to_string(),
            entity_id: Some(self.id),
            external_id: Some(self.external_id.clone()),
            event_id: self.last_event_id.clone(),
            action: action.to_string(),
            actor: actor.to_string(),
            detail: serde_json::json!({
                "event_type": self.event_type,
                "amount": self.money.amount().cents(),
                "currency": self.money.currency().as_str(),
                "status": self.status.as_str(),
            }),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::money::{Currency, Money, MoneyAmount};

    #[test]
    fn can_transition_valid_paths() {
        use PaymentStatus::*;
        assert!(Pending.can_transition_to(&Succeeded));
        assert!(Pending.can_transition_to(&Failed));
        assert!(Succeeded.can_transition_to(&Refunded));
    }

    #[test]
    fn can_transition_invalid_paths() {
        use PaymentStatus::*;
        // same status
        assert!(!Pending.can_transition_to(&Pending));
        assert!(!Succeeded.can_transition_to(&Succeeded));
        // backwards
        assert!(!Succeeded.can_transition_to(&Pending));
        assert!(!Failed.can_transition_to(&Pending));
        // impossible edges
        assert!(!Failed.can_transition_to(&Succeeded));
        assert!(!Failed.can_transition_to(&Refunded));
        assert!(!Pending.can_transition_to(&Refunded));
        // terminal
        assert!(!Refunded.can_transition_to(&Pending));
        assert!(!Refunded.can_transition_to(&Succeeded));
    }

    #[test]
    fn status_as_str_roundtrip() {
        let statuses = [
            PaymentStatus::Pending,
            PaymentStatus::Succeeded,
            PaymentStatus::Failed,
            PaymentStatus::Refunded,
        ];
        for s in &statuses {
            let parsed = PaymentStatus::try_from(s.as_str()).unwrap();
            assert_eq!(&parsed, s);
        }
    }

    #[test]
    fn status_try_from_unknown_is_err() {
        let result = PaymentStatus::try_from("cancelled");
        assert!(result.is_err());
    }

    #[test]
    fn direction_as_str_roundtrip() {
        assert_eq!(
            PaymentDirection::try_from("inbound").unwrap(),
            PaymentDirection::Inbound
        );
        assert_eq!(
            PaymentDirection::try_from("outbound").unwrap(),
            PaymentDirection::Outbound
        );
    }

    #[test]
    fn direction_try_from_unknown_is_err() {
        assert!(PaymentDirection::try_from("lateral").is_err());
    }

    #[test]
    fn new_payment_audit_entry() {
        let p = NewPayment::new(
            Uuid::now_v7(),
            "pi_123".into(),
            "stripe".into(),
            "payment_intent.succeeded".into(),
            PaymentDirection::Inbound,
            Money::new(MoneyAmount::new(5000), Currency::Eur),
            PaymentStatus::Succeeded,
            serde_json::json!({}),
            serde_json::json!({"id": "evt_1"}),
            "evt_1".into(),
            None,
            1709136000,
        );

        let audit = p.audit_entry("webhook:stripe", "created");
        assert_eq!(audit.action, "created");
        assert_eq!(audit.actor, "webhook:stripe");
        assert_eq!(audit.entity_id, Some(p.id()));
        assert_eq!(audit.external_id.as_deref(), Some("pi_123"));
        assert_eq!(audit.event_id, "evt_1");
        assert_eq!(audit.detail["currency"], "eur");
        assert_eq!(audit.detail["amount"], 5000);
    }
}
