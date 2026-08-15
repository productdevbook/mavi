//! The binary this crate builds on its own — nothing outside it, so
//! [`mavi::start::start`] is called with an empty [`mavi::kernel::outside::Outside`].
//! Something that depends on this crate as a library calls the same function
//! with its own.

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    tracing_subscriber::fmt()
        .json()
        .with_env_filter(tracing_subscriber::EnvFilter::from_default_env())
        .init();

    let mut asked = std::env::args().skip(1);

    match asked.next().as_deref() {
        None => mavi::start::start(mavi::kernel::outside::Outside::default()).await,

        // The way back in, on the host. There is one account that can grant
        // the owner role and nothing on the web can reset it, which is the
        // trade this makes: whoever can run this can already read the
        // database, so it adds nothing to reach and takes away the day
        // somebody forgets their password and there is no way in at all.
        Some("reset-password") => match asked.next() {
            Some(email) => mavi::recover::reset_password(&email).await,
            None => Err("reset-password wants the address of the account, \
                         and reads the new password from standard input"
                .into()),
        },

        Some(other) => Err(format!("{other} is not something this does").into()),
    }
}
