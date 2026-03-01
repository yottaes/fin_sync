use derive_more::Display;
use serde::{Deserialize, Serialize};

use super::error::PipelineError;

/// Payment-intent or refund identifier (`pi_xxx`, `re_xxx`).
#[derive(Debug, Clone, PartialEq, Eq, Display, Serialize, Deserialize)]
#[serde(transparent)]
pub struct ExternalId(String);

impl ExternalId {
    pub fn new(id: impl Into<String>) -> Result<Self, PipelineError> {
        let id = id.into();
        if !(id.starts_with("pi_") || id.starts_with("re_")) {
            return Err(PipelineError::Validation(format!(
                "ExternalId must start with pi_ or re_, got: {id}"
            )));
        }
        Ok(Self(id))
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
    pub fn new(id: impl Into<String>) -> Result<Self, PipelineError> {
        let id = id.into();
        if !id.starts_with("evt_") {
            return Err(PipelineError::Validation(format!(
                "EventId must start with evt_, got: {id}"
            )));
        }
        Ok(Self(id))
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }

    pub fn into_inner(self) -> String {
        self.0
    }
}
