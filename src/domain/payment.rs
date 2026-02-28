use {
    super::audit::NewAuditEntry,
    super::error::PipelineError,
    super::money::Money,
    chrono::{DateTime, Utc},
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

/// Full payment record from DB (for reads).
#[derive(Debug, Clone, Serialize)]
pub struct Payment {
    id: Uuid,
    external_id: String,
    source: String,
    event_type: String,
    direction: PaymentDirection,
    money: Money,
    status: PaymentStatus,
    metadata: serde_json::Value,
    raw_event: serde_json::Value,
    received_at: DateTime<Utc>,
    created_at: DateTime<Utc>,
}

impl Payment {
    pub fn id(&self) -> Uuid {
        self.id
    }

    pub fn status(&self) -> &PaymentStatus {
        &self.status
    }

    pub fn money(&self) -> &Money {
        &self.money
    }

    pub fn transition_status(&mut self, new: PaymentStatus) -> Result<(), PipelineError> {
        let valid = matches!(
            (&self.status, &new),
            (PaymentStatus::Pending, PaymentStatus::Succeeded)
                | (PaymentStatus::Pending, PaymentStatus::Failed)
                | (PaymentStatus::Succeeded, PaymentStatus::Refunded)
        );

        if !valid {
            return Err(PipelineError::Validation(format!(
                "invalid status transition: {} → {}",
                self.status, new
            )));
        }

        self.status = new;
        Ok(())
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

    pub fn audit_entry(&self, actor: &str) -> NewAuditEntry {
        NewAuditEntry {
            id: Uuid::now_v7(),
            entity_type: "payment".to_string(),
            entity_id: self.id,
            action: "created".to_string(),
            actor: actor.to_string(),
            detail: serde_json::json!({
                "external_id": self.external_id,
                "event_type": self.event_type,
                "amount": self.money.amount().cents(),
                "currency": self.money.currency().as_str(),
                "status": self.status.as_str(),
            }),
        }
    }
}
