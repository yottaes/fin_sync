use {
    super::audit::NewAuditEntry,
    super::error::PipelineError,
    super::id::{EventId, ExternalId},
    super::money::Money,
    serde::{Deserialize, Serialize},
    std::fmt,
    uuid::Uuid,
};

// ── Process result ───────────────────────────────────────────────────────────

#[derive(Debug)]
pub enum ProcessResult {
    /// New payment row inserted.
    Created(Uuid),
    /// Existing payment row updated (status advanced).
    Updated(Uuid),
    /// Event is older than what we've already processed — no state change.
    Stale(Uuid),
    /// Stripe event was already processed (duplicate delivery).
    Duplicate,
    /// Transition is not valid per state machine — logged as anomaly.
    Anomaly(Uuid),
}

// ── Existing payment (read model for decisions) ──────────────────────────────

/// Current state of a payment row, returned by repo for decision-making.
pub struct ExistingPayment {
    pub id: Uuid,
    pub status: PaymentStatus,
    pub last_provider_ts: i64,
}

// ── Decision types ───────────────────────────────────────────────────────────

pub enum PaymentAction {
    Advance { old_status: PaymentStatus },
    SameStatus,
    TemporalStale,
    LogAnomaly { current: PaymentStatus },
}

impl ExistingPayment {
    /// Pure decision: what action to take given an incoming payment event.
    /// Called only when an existing row is found — the `None` (insert) case
    /// is handled by the caller before reaching this method.
    pub fn decide(&self, incoming: &NewPayment) -> PaymentAction {
        if *incoming.status() == self.status {
            PaymentAction::SameStatus
        } else if incoming.provider_ts() < self.last_provider_ts {
            PaymentAction::TemporalStale
        } else if !self.status.can_transition_to(incoming.status()) {
            PaymentAction::LogAnomaly {
                current: self.status.clone(),
            }
        } else {
            PaymentAction::Advance {
                old_status: self.status.clone(),
            }
        }
    }
}

// ── Passthrough event ────────────────────────────────────────────────────────

/// Event that we log but don't process as a payment (charges, unknown types).
pub struct PassthroughEvent {
    pub external_id: Option<String>,
    pub event_id: String,
    pub event_type: String,
    pub provider_ts: i64,
    pub raw_payload: serde_json::Value,
    pub actor: String,
}

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
    ///
    /// PI rows (pi_xxx):  Pending → Succeeded | Failed
    /// Refund rows (re_xxx): Pending → Refunded | Failed
    pub fn can_transition_to(&self, new: &Self) -> bool {
        matches!(
            (self, new),
            (Self::Pending, Self::Succeeded)
                | (Self::Pending, Self::Failed)
                | (Self::Pending, Self::Refunded)
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

/// Named params for constructing a NewPayment. All fields explicit at the call site.
pub struct NewPaymentParams {
    pub external_id: ExternalId,
    pub source: String,
    pub event_type: String,
    pub direction: PaymentDirection,
    pub money: Money,
    pub status: PaymentStatus,
    pub metadata: serde_json::Value,
    pub raw_event: serde_json::Value,
    pub last_event_id: EventId,
    pub parent_external_id: Option<ExternalId>,
    pub provider_ts: i64,
}

/// For INSERT — id auto-generated via Uuid::now_v7().
#[derive(Debug, Clone)]
pub struct NewPayment {
    id: Uuid,
    external_id: ExternalId,
    source: String,
    event_type: String,
    direction: PaymentDirection,
    money: Money,
    status: PaymentStatus,
    metadata: serde_json::Value,
    raw_event: serde_json::Value,
    last_event_id: EventId,
    parent_external_id: Option<ExternalId>,
    provider_ts: i64,
}

impl NewPayment {
    pub fn new(p: NewPaymentParams) -> Self {
        Self {
            id: Uuid::now_v7(),
            external_id: p.external_id,
            source: p.source,
            event_type: p.event_type,
            direction: p.direction,
            money: p.money,
            status: p.status,
            metadata: p.metadata,
            raw_event: p.raw_event,
            last_event_id: p.last_event_id,
            parent_external_id: p.parent_external_id,
            provider_ts: p.provider_ts,
        }
    }

    pub fn id(&self) -> Uuid {
        self.id
    }

    pub fn external_id(&self) -> &str {
        self.external_id.as_str()
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
        self.last_event_id.as_str()
    }

    pub fn parent_external_id(&self) -> Option<&str> {
        self.parent_external_id.as_ref().map(|id| id.as_str())
    }

    pub fn provider_ts(&self) -> i64 {
        self.provider_ts
    }

    pub fn audit_entry(&self, actor: &str, action: &str) -> NewAuditEntry {
        NewAuditEntry {
            id: Uuid::now_v7(),
            entity_type: "payment".to_string(),
            entity_id: Some(self.id),
            external_id: Some(self.external_id.clone().into_inner()),
            event_id: self.last_event_id.clone().into_inner(),
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

//Tests
#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::money::{Currency, Money, MoneyAmount};

    #[test]
    fn can_transition_valid_paths() {
        use PaymentStatus::*;
        assert!(Pending.can_transition_to(&Succeeded));
        assert!(Pending.can_transition_to(&Failed));
        assert!(Pending.can_transition_to(&Refunded));
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
        assert!(!Succeeded.can_transition_to(&Refunded));
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
        use crate::domain::id::{EventId, ExternalId};

        let p = NewPayment::new(NewPaymentParams {
            external_id: ExternalId::new("pi_123").unwrap(),
            source: "stripe".into(),
            event_type: "payment_intent.succeeded".into(),
            direction: PaymentDirection::Inbound,
            money: Money::new(MoneyAmount::new(5000).unwrap(), Currency::Eur),
            status: PaymentStatus::Succeeded,
            metadata: serde_json::json!({}),
            raw_event: serde_json::json!({"id": "evt_1"}),
            last_event_id: EventId::new("evt_1").unwrap(),
            parent_external_id: None,
            provider_ts: 1709136000,
        });

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
