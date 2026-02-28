use derive_more::Display;
use serde::{Deserialize, Serialize};

/// Payment-intent or refund identifier (`pi_xxx`, `re_xxx`).
#[derive(Debug, Clone, PartialEq, Eq, Display, Serialize, Deserialize)]
#[serde(transparent)]
pub struct ExternalId(String);

impl ExternalId {
    pub fn new(id: impl Into<String>) -> Self {
        let id = id.into();
        assert!(
            id.starts_with("pi_") || id.starts_with("re_"),
            "ExternalId must start with pi_ or re_, got: {id}"
        );
        Self(id)
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }

    pub fn into_inner(self) -> String {
        self.0
    }
}

/// Stripe event identifier (`evt_xxx`).
#[derive(Debug, Clone, PartialEq, Eq, Display, Serialize, Deserialize)]
#[serde(transparent)]
pub struct EventId(String);

impl EventId {
    pub fn new(id: impl Into<String>) -> Self {
        let id = id.into();
        assert!(
            id.starts_with("evt_"),
            "EventId must start with evt_, got: {id}"
        );
        Self(id)
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }

    pub fn into_inner(self) -> String {
        self.0
    }
}
