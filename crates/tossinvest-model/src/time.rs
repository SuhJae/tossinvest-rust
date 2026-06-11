//! Time type aliases. The API expresses timestamps as ISO 8601 / RFC 3339 with an
//! explicit offset (always `+09:00`, KST), and plain dates as `YYYY-MM-DD`.

use chrono::{DateTime, FixedOffset, NaiveDate};

/// An offset-aware timestamp (KST, `+09:00`). The offset is preserved, not normalized.
pub type KstDateTime = DateTime<FixedOffset>;

/// A calendar date with no time component (`YYYY-MM-DD`).
pub type KstDate = NaiveDate;
