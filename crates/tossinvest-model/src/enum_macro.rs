//! The `open_enum!` macro: string enums that round-trip unknown values verbatim.
//!
//! The Toss Open API explicitly requires clients to tolerate unknown enum values
//! ("클라이언트는 unknown enum 값을 허용하도록 구현해야 합니다"). Every spec enum that
//! carries that clause is modeled with this macro, which adds an `Unknown(String)`
//! tail variant that **preserves** the original wire string (for logging and
//! forward-compatibility) instead of discarding it like `#[serde(other)]` would.

/// Define an open string enum with a value-preserving `Unknown(String)` fallback.
///
/// ```ignore
/// open_enum! {
///     /// Currency code.
///     pub enum Currency { Krw => "KRW", Usd => "USD" }
/// }
/// ```
macro_rules! open_enum {
    (
        $(#[$meta:meta])*
        $vis:vis enum $name:ident {
            $( $(#[$vmeta:meta])* $variant:ident => $wire:literal ),* $(,)?
        }
    ) => {
        $(#[$meta])*
        #[derive(Clone, PartialEq, Eq, Hash)]
        $vis enum $name {
            $( $(#[$vmeta])* $variant, )*
            /// A value not known at code-generation time. The raw wire string is
            /// preserved; clients must tolerate unknown values.
            Unknown(::std::string::String),
        }

        impl $name {
            /// The on-the-wire string for this value.
            pub fn as_wire(&self) -> &str {
                match self {
                    $( Self::$variant => $wire, )*
                    Self::Unknown(s) => s.as_str(),
                }
            }
            /// `true` unless this is an [`Self::Unknown`] value.
            pub fn is_known(&self) -> bool {
                !matches!(self, Self::Unknown(_))
            }
        }

        impl ::core::fmt::Debug for $name {
            fn fmt(&self, f: &mut ::core::fmt::Formatter<'_>) -> ::core::fmt::Result {
                match self {
                    $( Self::$variant => f.write_str(concat!(stringify!($name), "::", stringify!($variant))), )*
                    Self::Unknown(s) => write!(f, concat!(stringify!($name), "::Unknown({:?})"), s),
                }
            }
        }

        impl ::core::fmt::Display for $name {
            fn fmt(&self, f: &mut ::core::fmt::Formatter<'_>) -> ::core::fmt::Result {
                f.write_str(self.as_wire())
            }
        }

        impl ::core::convert::From<&str> for $name {
            fn from(s: &str) -> Self {
                match s {
                    $( $wire => Self::$variant, )*
                    other => Self::Unknown(other.to_owned()),
                }
            }
        }

        impl ::core::str::FromStr for $name {
            type Err = ::core::convert::Infallible;
            fn from_str(s: &str) -> ::core::result::Result<Self, Self::Err> {
                ::core::result::Result::Ok(Self::from(s))
            }
        }

        impl ::serde::Serialize for $name {
            fn serialize<S: ::serde::Serializer>(&self, ser: S) -> ::core::result::Result<S::Ok, S::Error> {
                ser.serialize_str(self.as_wire())
            }
        }

        impl<'de> ::serde::Deserialize<'de> for $name {
            fn deserialize<D: ::serde::Deserializer<'de>>(de: D) -> ::core::result::Result<Self, D::Error> {
                let s = <::std::string::String as ::serde::Deserialize>::deserialize(de)?;
                ::core::result::Result::Ok(Self::from(s.as_str()))
            }
        }
    };
}

pub(crate) use open_enum;
