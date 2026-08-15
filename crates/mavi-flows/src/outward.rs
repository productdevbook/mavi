//! Where a site is allowed to send a request of its own.
//!
//! A flow that calls an address is the one place in this whole crate family
//! where **somebody using the site decides what the server connects to**. That
//! is not a small thing: the server is inside whatever network it is running
//! in, and things in there answer without asking who is calling.
//!
//! The addresses that matter are not exotic. `http://localhost:5432` is the
//! database. `http://169.254.169.254/` is where a cloud machine keeps its own
//! credentials. `http://10.0.0.5/` is whatever else is on the same private
//! network — a metrics page, another customer's site, an admin panel that
//! believed nobody outside could reach it.
//!
//! So this is an allowlist of shapes rather than a list of what to refuse: it
//! must be `http` or `https`, it must have a host, and that host must not
//! resolve to somewhere only this machine can reach. What cannot be answered
//! here is the DNS name that *points* at one of those addresses — that is
//! answered when the request is actually made, by whatever makes it, and this
//! refuses everything it can see from the text alone.

use std::net::{IpAddr, Ipv4Addr, Ipv6Addr};

use mavi_core::error::{Error, Result};
use mavi_core::say::Say;

pub const THAT_IS_NOT_AN_ADDRESS_TO_CALL: &str = "that_is_not_an_address_to_call";
pub const THAT_ADDRESS_IS_INSIDE_THIS_MACHINE: &str = "that_address_is_inside_this_machine";

/// Names that mean this machine however they are spelled.
const OURSELVES: &[&str] = &[
    "localhost",
    "localhost.localdomain",
    "ip6-localhost",
    "ip6-loopback",
    // The name a cloud machine's own credentials answer at. Worth naming
    // rather than leaving to the address check, because it is a name and
    // whoever types it knows exactly what they are doing.
    "metadata.google.internal",
];

/// An address a site's own flow may call.
///
/// Checked from the text and nothing else. Whatever finally makes the request
/// checks again after resolving the name, because a name that answers with a
/// private address today is a name somebody can point anywhere tomorrow.
pub fn to_call(address: &str) -> Result<String> {
    let address = address.trim();

    let refuse = || Error::invalid(Say::of(THAT_IS_NOT_AN_ADDRESS_TO_CALL));

    let rest = address
        .strip_prefix("https://")
        .or_else(|| address.strip_prefix("http://"))
        .ok_or_else(refuse)?;

    // Everything up to the first slash: the host and, if somebody wrote one,
    // a port and a name and password.
    let authority = rest.split(['/', '?', '#']).next().unwrap_or_default();

    // A name and a password in an address is a credential in the database and
    // in every log that ever prints it.
    if authority.is_empty() || authority.contains('@') || authority.contains(' ') {
        return Err(refuse());
    }

    // `[::1]:8080` keeps its brackets until here, because the colons inside
    // them are part of the address rather than a port.
    let host = match authority.strip_prefix('[') {
        Some(inside) => inside.split(']').next().unwrap_or_default(),
        None => authority.split(':').next().unwrap_or_default(),
    };

    if host.is_empty() {
        return Err(refuse());
    }

    if ourselves(host) {
        return Err(Error::invalid(Say::of(THAT_ADDRESS_IS_INSIDE_THIS_MACHINE)));
    }

    Ok(address.to_owned())
}

/// Whether this host is somewhere only this machine can reach.
#[must_use]
pub fn ourselves(host: &str) -> bool {
    let host = host.to_ascii_lowercase();

    if OURSELVES.contains(&host.as_str()) || host.ends_with(".localhost") {
        return true;
    }

    match host.parse::<IpAddr>() {
        Ok(IpAddr::V4(four)) => inside_four(four),
        Ok(IpAddr::V6(six)) => inside_six(six),
        // A name. Whether it points inside is a question for whoever resolves
        // it, not for this.
        Err(_) => false,
    }
}

fn inside_four(address: Ipv4Addr) -> bool {
    address.is_loopback()
        || address.is_private()
        || address.is_link_local()
        || address.is_broadcast()
        || address.is_documentation()
        || address.is_unspecified()
        // 100.64.0.0/10, which is what a carrier or a container network hands
        // out and which `is_private` does not count.
        || (address.octets()[0] == 100 && (64..128).contains(&address.octets()[1]))
}

fn inside_six(address: Ipv6Addr) -> bool {
    if address.is_loopback() || address.is_unspecified() {
        return true;
    }

    // An address that is really a v4 one wearing a v6 hat. Written out because
    // `::ffff:127.0.0.1` is a loopback address that `is_loopback` says no to.
    if let Some(four) = address.to_ipv4_mapped() {
        return inside_four(four);
    }

    let first = address.segments()[0];

    // fc00::/7, the private range, and fe80::/10, link local.
    (first & 0xfe00) == 0xfc00 || (first & 0xffc0) == 0xfe80
}

#[cfg(test)]
mod tests {
    use super::*;

    fn refused(address: &str) -> &'static str {
        to_call(address)
            .expect_err("a refusal")
            .said()
            .expect("a sentence")
            .key
    }

    #[test]
    fn an_ordinary_address_is_called() {
        for right in [
            "https://example.test/hooks/one",
            "http://example.test",
            "https://example.test:8443/a/b?c=d",
        ] {
            assert!(to_call(right).is_ok(), "{right} was refused");
        }
    }

    #[test]
    fn the_database_is_not_somewhere_a_site_calls() {
        // `http://localhost:5432` is not a hypothetical. It is the first thing
        // anybody tries when they are wondering what the server can see.
        for inside in [
            "http://localhost:5432",
            "http://127.0.0.1/",
            "http://[::1]/",
            "http://0.0.0.0/",
            "http://anything.localhost/",
        ] {
            assert_eq!(
                refused(inside),
                THAT_ADDRESS_IS_INSIDE_THIS_MACHINE,
                "{inside} was called"
            );
        }
    }

    #[test]
    fn where_a_cloud_machine_keeps_its_own_credentials_is_not_either() {
        for inside in [
            "http://169.254.169.254/latest/meta-data/",
            "http://metadata.google.internal/",
        ] {
            assert_eq!(
                refused(inside),
                THAT_ADDRESS_IS_INSIDE_THIS_MACHINE,
                "{inside} was called"
            );
        }
    }

    #[test]
    fn nothing_else_on_the_private_network_is_either() {
        // The neighbours: another service, a metrics page, an admin panel that
        // believed nobody outside could reach it.
        for inside in [
            "http://10.0.0.5/",
            "http://192.168.1.1/",
            "http://172.16.0.1/",
            "http://100.64.0.1/",
            "http://[fd00::1]/",
            "http://[fe80::1]/",
        ] {
            assert_eq!(
                refused(inside),
                THAT_ADDRESS_IS_INSIDE_THIS_MACHINE,
                "{inside} was called"
            );
        }
    }

    #[test]
    fn a_loopback_address_wearing_a_v6_hat_is_still_loopback() {
        // `::ffff:127.0.0.1` is the one that gets through a check written the
        // obvious way: it is a v6 address, and `is_loopback` says no.
        assert_eq!(
            refused("http://[::ffff:127.0.0.1]/"),
            THAT_ADDRESS_IS_INSIDE_THIS_MACHINE
        );
    }

    #[test]
    fn nothing_that_is_not_a_web_address_is_called_at_all() {
        for wrong in [
            "",
            "example.test",
            "ftp://example.test",
            "file:///etc/passwd",
            "gopher://example.test",
            "https://",
            // A name and a password in an address is a credential in the
            // database and in every log that prints it.
            "https://someone:hunter2@example.test/",
        ] {
            assert_eq!(
                refused(wrong),
                THAT_IS_NOT_AN_ADDRESS_TO_CALL,
                "{wrong:?} was taken for an address"
            );
        }
    }

    #[test]
    fn a_name_is_left_to_whoever_resolves_it() {
        // This cannot know where a name points, and pretending otherwise would
        // be a check that reads as complete and is not. What it can do is
        // refuse everything visible in the text, and say so.
        assert!(to_call("https://an-address-that-points-inside.test/").is_ok());
    }
}
