//! What this is made of, as a list rather than as a habit.
//!
//! Every endpoint belongs to one of these, every span is named by one, and
//! every number this machine publishes is counted under one. A dashboard can
//! then ask "is the shop slow" rather than "is `/api/sites/checkout` slow",
//! which is the question somebody actually has.
//!
//! Beside the modules rather than in the kernel: this is the list of what this
//! particular installation is made of, and a kernel holding it is a kernel that
//! knows the name of every domain built on it.

/// Every domain there is. A name not here is not one, and a test refuses an
/// endpoint that claims otherwise.
pub const DOMAINS: &[&str] = &[
    "analytics",
    "assistant",
    "audit",
    "auth",
    "boards",
    "content",
    "flows",
    "forms",
    "health",
    "learning",
    "mail",
    "mcp",
    "media",
    "pages",
    "people",
    "plugins",
    "portable",
    "publishing",
    "reports",
    "setup",
    "shop",
    "site",
    "trash",
    "webhooks",
];

#[must_use]
pub fn known(name: &str) -> bool {
    DOMAINS.contains(&name)
}

/// The domain an event belongs to: what comes before the dot. `order.paid` is
/// the shop's, `post.published` is content's — so the name is looked up rather
/// than guessed from the word.
#[must_use]
pub fn of_event(event: &str) -> &'static str {
    let subject = event.split('.').next().unwrap_or_default();

    match subject {
        "order" | "refund" | "stock" | "coupon" | "product" => "shop",
        "post" | "term" | "language" => "content",
        "form" | "submission" => "forms",
        "student" | "enrolment" | "lesson" => "learning",
        "campaign" | "subscriber" | "letter" => "mail",
        "card" | "board" => "boards",
        "media" | "video" => "media",
        "publish" | "build" => "publishing",
        "user" | "role" | "invitation" => "people",
        "site" | "domain" => "site",
        _ => "webhooks",
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_name_an_event_is_counted_under_is_a_domain() {
        for event in [
            "order.paid",
            "post.published",
            "form.received",
            "student.enrolled",
            "campaign.sent",
            "card.moved",
            "media.uploaded",
            "publish.finished",
            "user.invited",
            "something.nobody.named",
        ] {
            let named = of_event(event);

            assert!(
                known(named),
                "{event} is counted under {named}, which is not a domain"
            );
        }
    }

    #[test]
    fn the_list_is_in_order_and_says_nothing_twice() {
        let mut sorted = DOMAINS.to_vec();
        sorted.sort_unstable();
        sorted.dedup();

        assert_eq!(sorted, DOMAINS, "the list of domains is not itself, sorted");
    }
}
