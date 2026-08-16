//! What this installation keeps, and for how long.
//!
//! Every table holding something about a person has a line here saying how long
//! it holds it, and the machine's own housekeeping reads this rather than each
//! domain remembering to forget. A table with somebody's own words in it and no
//! line here fails a test rather than quietly keeping them forever.
//!
//! The tables are the domains' and so are the sweeps, so the list is here rather
//! than in the kernel; what a line of it looks like is
//! [`kernel::retention`](crate::kernel::retention), and an outside crate writes
//! its own lines in the same shape.
pub use crate::kernel::retention::{Keeps, Policy};

pub const POLICIES: &[Policy] = &[
    Policy {
        table: "form_submissions",
        keeps: Keeps::AsTheSiteSays {
            column: "forms.retention_days",
        },
        swept_by: "forms.sweep",
    },
    Policy {
        // An order is a financial record before it is anything else, and how
        // long one is kept is a thing tax law says rather than a preference.
        table: "orders",
        keeps: Keeps::Days(3650),
        swept_by: "shop.sweep-orders",
    },
    Policy {
        // Somebody's address, and what was sent to it. Two years is long
        // enough to answer "did you send me that" and short enough not to be
        // a mailing list nobody meant to keep.
        table: "email_log",
        keeps: Keeps::Days(730),
        swept_by: "mail.sweep-log",
    },
    Policy {
        table: "subscribers",
        keeps: Keeps::WithItsSubject,
        swept_by: "the site removing them, or their own unsubscribe",
    },
    Policy {
        table: "students",
        keeps: Keeps::WithItsSubject,
        swept_by: "the site removing them",
    },
    Policy {
        table: "student_sessions",
        keeps: Keeps::Days(30),
        swept_by: "sessions.sweep",
    },
    Policy {
        // Not an address, only what a day's salt hashed one to, and gone with
        // the day. Here because a check that only looks at column names would
        // otherwise miss it, and it is worth saying out loud.
        table: "visitor_marks",
        keeps: Keeps::Days(2),
        swept_by: "analytics.sweep-marks",
    },
    Policy {
        table: "media",
        keeps: Keeps::WithItsSubject,
        swept_by: "the site removing the file",
    },
    Policy {
        table: "users",
        keeps: Keeps::WithItsSubject,
        swept_by: "the site removing the account",
    },
    Policy {
        table: "sessions",
        keeps: Keeps::Days(30),
        swept_by: "sessions.sweep",
    },
    Policy {
        table: "tickets",
        keeps: Keeps::Days(7),
        swept_by: "sessions.sweep",
    },
    Policy {
        table: "audit_log",
        keeps: Keeps::Days(365),
        swept_by: "audit.sweep",
    },
    Policy {
        // `said_by` names who said it, and `environment` gathers the browser
        // and window rather than asking for them — the same shape of thing an
        // audit row is, so it is kept the same length of time.
        table: "reports",
        keeps: Keeps::Days(365),
        swept_by: "reports.sweep",
    },
    Policy {
        table: "webhook_deliveries",
        keeps: Keeps::Days(30),
        swept_by: "webhooks.sweep",
    },
];

#[must_use]
pub fn policy_for(table: &str) -> Option<&'static Policy> {
    POLICIES.iter().find(|policy| policy.table == table)
}

/// This crate's own policies, followed by whatever an outside crate handed
/// in. What a gate test checks about `POLICIES` needs to hold for an outside
/// crate's own tables too, and this is the one list that has both.
#[must_use]
pub fn all(outside: &crate::kernel::outside::Outside) -> Vec<Policy> {
    POLICIES
        .iter()
        .copied()
        .chain(outside.policies.iter().copied())
        .collect()
}
