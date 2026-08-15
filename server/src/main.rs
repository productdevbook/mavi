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

    if let Some(asked) = std::env::args().nth(1) {
        return Err(format!("{asked} is not something this does").into());
    }

    mavi::start::start(mavi::kernel::outside::Outside::default()).await
}
