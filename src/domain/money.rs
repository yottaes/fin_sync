use {
    super::error::PipelineError,
    serde::{Deserialize, Serialize},
    std::fmt,
    std::ops::{Add, Sub},
};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct MoneyAmount(i64);

impl MoneyAmount {
    pub fn new(cents: i64) -> Result<Self, PipelineError> {
        if cents < 0 {
            return Err(PipelineError::Validation(format!(
                "MoneyAmount cannot be negative, got: {cents}"
            )));
        }
        Ok(Self(cents))
    }

    pub fn cents(&self) -> i64 {
        self.0
    }

    pub fn checked_add(self, other: MoneyAmount) -> Option<MoneyAmount> {
        self.0.checked_add(other.0).map(MoneyAmount)
    }

    pub fn checked_sub(self, other: MoneyAmount) -> Option<MoneyAmount> {
        self.0
            .checked_sub(other.0)
            .filter(|&v| v >= 0)
            .map(MoneyAmount)
    }
}

impl Add for MoneyAmount {
    type Output = MoneyAmount;

    fn add(self, rhs: MoneyAmount) -> MoneyAmount {
        self.checked_add(rhs).expect("MoneyAmount overflow")
    }
}

impl Sub for MoneyAmount {
    type Output = MoneyAmount;

    fn sub(self, rhs: MoneyAmount) -> MoneyAmount {
        self.checked_sub(rhs).expect("MoneyAmount underflow")
    }
}

impl fmt::Display for MoneyAmount {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.0)
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Currency {
    Usd,
    Eur,
    Gbp,
    Jpy,
}

impl Currency {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Usd => "usd",
            Self::Eur => "eur",
            Self::Gbp => "gbp",
            Self::Jpy => "jpy",
        }
    }
}

impl fmt::Display for Currency {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.as_str())
    }
}

impl TryFrom<&str> for Currency {
    type Error = PipelineError;

    fn try_from(s: &str) -> Result<Self, Self::Error> {
        match s {
            "usd" => Ok(Self::Usd),
            "eur" => Ok(Self::Eur),
            "gbp" => Ok(Self::Gbp),
            "jpy" => Ok(Self::Jpy),
            other => Err(PipelineError::Validation(format!(
                "unknown currency: {other}"
            ))),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Money {
    amount: MoneyAmount,
    currency: Currency,
}

impl Money {
    pub fn new(amount: MoneyAmount, currency: Currency) -> Self {
        Self { amount, currency }
    }

    pub fn amount(&self) -> MoneyAmount {
        self.amount
    }

    pub fn currency(&self) -> &Currency {
        &self.currency
    }
}
