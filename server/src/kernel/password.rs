//! Argon2id, with the parameters written down rather than left to a default
//! that can move under a stored hash.

use argon2::password_hash::{
    PasswordHash, PasswordHasher, PasswordVerifier, SaltString, rand_core,
};
use argon2::{Algorithm, Argon2, Params, Version};

use super::error::{AppError, Result};
use crate::kernel::say;

/// 19 MiB and two passes: the OWASP figure, and the one the stored hashes were
/// made with. A stored hash carries its own parameters, so raising these later
/// does not invalidate anything — it only applies to what is hashed after.
fn argon2() -> Argon2<'static> {
    let params = Params::new(19 * 1024, 2, 1, None).expect("parameters are in range");
    Argon2::new(Algorithm::Argon2id, Version::V0x13, params)
}

pub fn hash(password: &str) -> Result<String> {
    let salt = SaltString::generate(&mut rand_core::OsRng);

    argon2()
        .hash_password(password.as_bytes(), &salt)
        .map(|hash| hash.to_string())
        .map_err(|_| AppError::Invalid(say::PASSWORD_CANNOT_STORED.into()))
}

/// False for a wrong password and false for a stored hash that cannot be read.
/// A hash nobody can parse is not a reason to let somebody in.
#[must_use]
pub fn verify(password: &str, stored: &str) -> bool {
    PasswordHash::new(stored).is_ok_and(|parsed| {
        argon2()
            .verify_password(password.as_bytes(), &parsed)
            .is_ok()
    })
}

/// Verifying against a hash nobody holds, so that an address with no account
/// costs the same as one with a wrong password.
pub fn waste_the_same_time(password: &str) {
    static NOBODY: std::sync::LazyLock<String> =
        std::sync::LazyLock::new(|| hash("not a password anybody has").expect("hashes"));

    let _ = verify(password, &NOBODY);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_password_verifies_against_its_own_hash_and_no_other() {
        let stored = hash("open sesame").expect("hash");
        assert!(verify("open sesame", &stored));
        assert!(!verify("open sesamé", &stored));
        assert!(!verify("open sesame", "not a hash"));
    }

    #[test]
    fn the_same_password_hashes_differently_twice() {
        assert_ne!(hash("same").expect("hash"), hash("same").expect("hash"));
    }
}
