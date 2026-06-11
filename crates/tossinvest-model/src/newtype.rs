//! Domain newtypes — distinct string/integer wrappers so identifiers, symbols, and
//! cursors cannot be accidentally interchanged.

use crate::scalar::ValidationError;
use serde::{Deserialize, Serialize};

macro_rules! string_newtype {
    ($(#[$m:meta])* $name:ident) => {
        $(#[$m])*
        #[derive(Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
        #[serde(transparent)]
        pub struct $name(pub String);

        impl $name {
            /// Borrow the inner string.
            pub fn as_str(&self) -> &str {
                &self.0
            }
            /// Consume and return the inner string.
            pub fn into_inner(self) -> String {
                self.0
            }
        }
        impl ::core::fmt::Debug for $name {
            fn fmt(&self, f: &mut ::core::fmt::Formatter<'_>) -> ::core::fmt::Result {
                write!(f, concat!(stringify!($name), "({:?})"), self.0)
            }
        }
        impl ::core::fmt::Display for $name {
            fn fmt(&self, f: &mut ::core::fmt::Formatter<'_>) -> ::core::fmt::Result {
                f.write_str(&self.0)
            }
        }
        impl ::core::convert::From<String> for $name {
            fn from(s: String) -> Self {
                Self(s)
            }
        }
        impl ::core::convert::From<&str> for $name {
            fn from(s: &str) -> Self {
                Self(s.to_owned())
            }
        }
        impl ::core::convert::AsRef<str> for $name {
            fn as_ref(&self) -> &str {
                &self.0
            }
        }
    };
}

string_newtype! {
    /// A security symbol. KR: 6-digit numeric (`005930`); US: ticker (`AAPL`).
    /// Use [`Symbol::new`] to validate against the API pattern `^[A-Za-z0-9.-]+$`.
    Symbol
}
string_newtype! {
    /// An opaque, server-issued order identifier. Modify/cancel mint a *new* one.
    OrderId
}
string_newtype! {
    /// A client-supplied idempotency key for order creation (10-minute window).
    /// Use [`ClientOrderId::new`] to validate (`^[A-Za-z0-9-_]+$`, max 36 chars).
    ClientOrderId
}
string_newtype! {
    /// An opaque pagination cursor.
    Cursor
}
string_newtype! {
    /// A request identifier (equal to the `X-Request-Id` response header).
    RequestId
}
string_newtype! {
    /// An ISIN code (ISO 6166), e.g. `KR7005930003`.
    IsinCode
}

impl Symbol {
    /// Construct a validated symbol (`^[A-Za-z0-9.-]+$`, non-empty).
    pub fn new(s: impl Into<String>) -> Result<Self, ValidationError> {
        let s = s.into();
        if s.is_empty() || !s.chars().all(|c| c.is_ascii_alphanumeric() || c == '.' || c == '-') {
            return Err(ValidationError::Invalid {
                what: "Symbol",
                value: s,
                reason: "must match ^[A-Za-z0-9.-]+$",
            });
        }
        Ok(Self(s))
    }
}

impl ClientOrderId {
    /// Construct a validated idempotency key (`^[A-Za-z0-9-_]+$`, 1..=36 chars).
    pub fn new(s: impl Into<String>) -> Result<Self, ValidationError> {
        let s = s.into();
        if s.is_empty() || s.len() > 36 || !s.chars().all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_') {
            return Err(ValidationError::Invalid {
                what: "ClientOrderId",
                value: s,
                reason: "max 36 chars, ^[A-Za-z0-9-_]+$",
            });
        }
        Ok(Self(s))
    }
}

/// An account sequence number — the value passed in the `X-Tossinvest-Account` header.
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Debug, Serialize, Deserialize)]
#[serde(transparent)]
pub struct AccountSeq(pub i64);

impl AccountSeq {
    /// The inner integer.
    pub fn get(self) -> i64 {
        self.0
    }
}
impl ::core::fmt::Display for AccountSeq {
    fn fmt(&self, f: &mut ::core::fmt::Formatter<'_>) -> ::core::fmt::Result {
        ::core::fmt::Display::fmt(&self.0, f)
    }
}
impl ::core::convert::From<i64> for AccountSeq {
    fn from(n: i64) -> Self {
        Self(n)
    }
}
