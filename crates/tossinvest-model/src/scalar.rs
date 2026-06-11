//! Numeric scalars. Every monetary / quantity / rate value on the wire is a JSON
//! **string** with `format: decimal` — never a float. These newtypes (de)serialize
//! as strings and carry exact [`rust_decimal::Decimal`] values.

use rust_decimal::Decimal;

/// Error returned when constructing a validated value from untrusted input.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum ValidationError {
    /// A quantity that must be a non-negative integer was not.
    #[error("expected a non-negative integer, got `{0}`")]
    NotNonNegInteger(String),
    /// A field failed its format/pattern constraint.
    #[error("invalid {what}: `{value}` ({reason})")]
    Invalid {
        /// The logical field/type name.
        what: &'static str,
        /// The offending value.
        value: String,
        /// Human-readable reason.
        reason: &'static str,
    },
}

macro_rules! decimal_newtype {
    ($(#[$m:meta])* $name:ident) => {
        $(#[$m])*
        #[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Default)]
        pub struct $name(pub Decimal);

        impl $name {
            /// The zero value.
            pub const ZERO: Self = Self(Decimal::ZERO);
            /// The inner [`Decimal`].
            pub fn get(self) -> Decimal {
                self.0
            }
        }

        impl ::core::fmt::Debug for $name {
            fn fmt(&self, f: &mut ::core::fmt::Formatter<'_>) -> ::core::fmt::Result {
                write!(f, "{}({})", stringify!($name), self.0)
            }
        }
        impl ::core::fmt::Display for $name {
            fn fmt(&self, f: &mut ::core::fmt::Formatter<'_>) -> ::core::fmt::Result {
                ::core::fmt::Display::fmt(&self.0, f)
            }
        }
        impl ::core::convert::From<Decimal> for $name {
            fn from(d: Decimal) -> Self {
                Self(d)
            }
        }
        impl ::serde::Serialize for $name {
            fn serialize<S: ::serde::Serializer>(&self, s: S) -> ::core::result::Result<S::Ok, S::Error> {
                s.collect_str(&self.0)
            }
        }
        impl<'de> ::serde::Deserialize<'de> for $name {
            fn deserialize<D: ::serde::Deserializer<'de>>(d: D) -> ::core::result::Result<Self, D::Error> {
                let s = <::std::string::String as ::serde::Deserialize>::deserialize(d)?;
                s.parse::<Decimal>().map(Self).map_err(::serde::de::Error::custom)
            }
        }
    };
}

decimal_newtype! {
    /// An exact decimal money/price/quantity value (wire: a decimal string).
    Dec
}
decimal_newtype! {
    /// A ratio expressed as a fraction (`0.1077` = 10.77%), not a percentage.
    Ratio
}
decimal_newtype! {
    /// A percentage-style rate as published by the API (e.g. commission rates).
    Percent
}

/// A non-negative **integer** quantity (wire pattern `^\d+$`), validated on construction.
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Default)]
pub struct IntQty(Decimal);

impl IntQty {
    /// Construct from a [`Decimal`], rejecting negatives and non-integers.
    pub fn new(d: Decimal) -> Result<Self, ValidationError> {
        if d < Decimal::ZERO || d.fract() != Decimal::ZERO {
            return Err(ValidationError::NotNonNegInteger(d.to_string()));
        }
        Ok(Self(d))
    }
    /// Construct from an unsigned integer (always valid).
    pub fn from_u64(n: u64) -> Self {
        Self(Decimal::from(n))
    }
    /// The inner [`Decimal`].
    pub fn get(self) -> Decimal {
        self.0
    }
}

impl ::core::fmt::Debug for IntQty {
    fn fmt(&self, f: &mut ::core::fmt::Formatter<'_>) -> ::core::fmt::Result {
        write!(f, "IntQty({})", self.0)
    }
}
impl ::core::fmt::Display for IntQty {
    fn fmt(&self, f: &mut ::core::fmt::Formatter<'_>) -> ::core::fmt::Result {
        ::core::fmt::Display::fmt(&self.0, f)
    }
}
impl ::serde::Serialize for IntQty {
    fn serialize<S: ::serde::Serializer>(&self, s: S) -> ::core::result::Result<S::Ok, S::Error> {
        s.collect_str(&self.0)
    }
}
impl<'de> ::serde::Deserialize<'de> for IntQty {
    fn deserialize<D: ::serde::Deserializer<'de>>(d: D) -> ::core::result::Result<Self, D::Error> {
        let s = <String as ::serde::Deserialize>::deserialize(d)?;
        let dec = s.parse::<Decimal>().map_err(::serde::de::Error::custom)?;
        Self::new(dec).map_err(::serde::de::Error::custom)
    }
}
