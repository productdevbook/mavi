//! One binary, three things it can be — and the same thing to something that
//! embeds this crate rather than compiles it as one.
//!
//! Everything a process that runs Mavi CMS has to do before it can answer a
//! request or take a job: reading the environment, opening the database,
//! running migrations — this crate's own, and then whatever an outside crate
//! carried in — standing the router up, the worker loop, the scheduler, and
//! coming back down when asked to. `main.rs` is the thin thing that calls
//! this with an empty [`Outside`]; something that depends on this crate
//! instead calls it with its own.
use std::env;

use crate::kernel::config::{Address, Config};
use crate::kernel::crypto::Keyring;
use crate::kernel::db::Db;
use crate::kernel::http::AppState;
use crate::kernel::outside::Outside;
use crate::kernel::worker;

/// Starts the API, the workers, the scheduler — whichever of them `ROLE`
/// asks for — and returns once asked to stop.
///
/// Refuses to start at all if `MAVI_KEYS` is not set and `MAVI_INVENT_KEYS`
/// does not say `yes`: a machine that invents a key cannot read what the
/// last one sealed, and that is cheaper to say here than to discover later.
/// Refuses on `MAVI_URL` for the same kind of reason: a machine that cannot
/// name its own address sends letters with links nobody can click, and says
/// nothing about it.
pub async fn start(mut outside: Outside) -> Result<(), Box<dyn std::error::Error>> {
    let keyring = keys_or_refuse()?;
    let address = address_or_refuse()?;
    crate::jobs::assert_schedules_are_runnable(&outside);

    let doing = Doing::from_env();
    let db = Db::connect(&env::var("DATABASE_URL")?, 16).await?;

    // Under sqlx's own advisory lock, so two replicas starting together do not
    // both try to add the same column. This crate's tables first, always:
    // an outside crate's migrations may assume ours already ran, and ours
    // may never be written to assume the reverse.
    db.migrate().await?;

    if let Some(migrator) = outside.migrations.take() {
        db.migrate_with(migrator).await?;
    }

    // The environment, read here and nowhere below: what a process is
    // started with is the edge's business, and a state built in the middle of
    // one is a global every caller of this crate would inherit.
    let mut state = AppState::new_with(db, Config::from_env(keyring, address));
    state.outside = std::sync::Arc::new(outside);
    // Set where something in front is known to be rewriting the header; unset
    // it is not believed at all.
    state.proxy_hops = env::var("PROXY_HOPS")
        .ok()
        .and_then(|n| n.parse().ok())
        .unwrap_or(0);

    let stop = worker::asked_to_stop();
    let mut running = Vec::new();

    if doing.works {
        // A worker at a time per core, within reason: what they wait on is
        // Postgres and somebody else's HTTP, not this machine's cores.
        let how_many = env::var("WORKERS")
            .ok()
            .and_then(|n| n.parse().ok())
            .unwrap_or(4_usize)
            .clamp(1, 32);

        let me = env::var("HOSTNAME").unwrap_or_else(|_| "worker".to_owned());

        for number in 0..how_many {
            running.push(tokio::spawn(worker::work(
                state.clone(),
                format!("{me}-{number}"),
                stop.clone(),
            )));
        }

        // Every process that works keeps time as well: the scheduler itself
        // takes one lock, so whichever gets there does it and the rest skip.
        running.push(tokio::spawn(worker::keep_time(state.clone(), stop.clone())));
    }

    if doing.answers {
        let app = crate::router(state);
        // Where to answer. Both have defaults that are right in a container
        // and wrong on a machine already serving something on 8080, which is
        // why they are asked for rather than assumed.
        let at = format!(
            "{}:{}",
            env::var("HOST").unwrap_or_else(|_| "0.0.0.0".to_owned()),
            env::var("PORT").unwrap_or_else(|_| "8080".to_owned())
        );

        let listener = tokio::net::TcpListener::bind(&at).await?;

        tracing::info!(address = %listener.local_addr()?, "listening");

        let mut leaving = stop.clone();

        axum::serve(
            listener,
            app.into_make_service_with_connect_info::<std::net::SocketAddr>(),
        )
        .with_graceful_shutdown(async move {
            let _ = leaving.changed().await;
        })
        .await?;
    }

    // Whatever is still holding a job finishes it: a worker killed mid-webhook
    // is a delivery somebody else has to work out the state of.
    for task in running {
        let _ = task.await;
    }

    tracing::info!("stopped");

    Ok(())
}

/// A machine with no keys makes one up, and everything it sealed becomes
/// unreadable the next time it restarts — which is not noticed until somebody
/// asks for a credential that was fine yesterday. So it is refused here, where
/// it costs one line to say instead of a day to work out.
///
/// A key that is given and cannot be read is the same failure wearing the
/// clothes of a working machine, so it is read here rather than at the first
/// credential somebody seals.
///
/// The keyring it read is what the state is built with: read once, at the
/// edge, and handed in — rather than read again in the middle by whatever
/// needs it.
fn keys_or_refuse() -> Result<Keyring, Box<dyn std::error::Error>> {
    if env::var_os("MAVI_KEYS").is_some() {
        return Keyring::from_the_environment().map_err(|why| {
            format!(
                "MAVI_KEYS is set and cannot be read: {why}. It is \
                 `1:<thirty-two bytes, base64>`, and a version and comma \
                 for each older key. Nothing starts under a key nobody \
                 gave: what this machine sealed with one it made up \
                 instead would look, later, like the data being wrong."
            )
            .into()
        });
    }

    // Left for whoever is trying this out on a laptop, where nothing sealed
    // matters and typing a key first would be the reason they stopped.
    if env::var("MAVI_INVENT_KEYS").is_ok_and(|said| said == "yes") {
        tracing::warn!(
            "no MAVI_KEYS: making one up, so nothing sealed by this process \
             can be read by the next one"
        );

        return Ok(Keyring::invented());
    }

    Err(
        "MAVI_KEYS is not set. Everything a site seals — its mail password, \
         its payment keys — is sealed with it, and a machine that makes one up \
         cannot read what the last one sealed. Set it, or set \
         MAVI_INVENT_KEYS=yes if nothing here is worth keeping."
            .into(),
    )
}

/// A machine that cannot name its own address sends letters with links nobody
/// can follow, and does it silently: a password reset built out of a bare path
/// is not clickable in a plain-text mail and goes nowhere pasted into a
/// browser, so the failure is a person who never got in rather than anything
/// in a log.
///
/// There is nothing to work it out from, either. Resolving a site from `Host`
/// means every request carries an address, but a scheduled job sending a
/// letter carries none — and defaulting to `localhost` would make links that
/// work on the machine that sent them and nowhere else, which is how this
/// class of bug survives. So it is asked for, once, before anything can send
/// one.
fn address_or_refuse() -> Result<Address, Box<dyn std::error::Error>> {
    match Address::from_the_environment() {
        Ok(Some(address)) => Ok(address),
        Ok(None) => Err(
            "MAVI_URL is not set. It is the address this installation answers \
             on, as somebody outside it would type it — `https://example.com` \
             — and it is what a link in a letter is built from. There is no \
             default: a scheduled job sending a password reset has no request \
             to take an address off, and one guessed here would send links \
             that work on this machine and nowhere else."
                .into(),
        ),
        Err(why) => Err(format!(
            "MAVI_URL is set and cannot be read: {why}. It is a scheme and a \
             host — `https://example.com`, or with the path it is served under \
             — and nothing else; what is joined onto it brings its own query."
        )
        .into()),
    }
}

/// What this process is for.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct Doing {
    answers: bool,
    works: bool,
}

impl Doing {
    /// Both, unless told otherwise: a single process is what somebody trying
    /// this out runs, and it should work without reading anything first.
    fn from_env() -> Self {
        // `ROLE` is what this was called before the project had a name of its
        // own, and something is deployed with it set.
        let said = env::var("MAVI_ROLE").or_else(|_| env::var("ROLE")).ok();

        Self::of(said.as_deref())
    }

    fn of(role: Option<&str>) -> Self {
        match role.map(str::trim).map(str::to_ascii_lowercase).as_deref() {
            Some("api") => Self {
                answers: true,
                works: false,
            },
            Some("worker") => Self {
                answers: false,
                works: true,
            },
            _ => Self {
                answers: true,
                works: true,
            },
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_process_with_nothing_said_about_it_does_everything() {
        assert_eq!(
            Doing::of(None),
            Doing {
                answers: true,
                works: true
            }
        );

        assert_eq!(
            Doing::of(Some("anything else")),
            Doing {
                answers: true,
                works: true
            }
        );
    }

    #[test]
    fn a_worker_does_not_answer_and_an_api_does_not_work() {
        assert!(!Doing::of(Some("worker")).answers);
        assert!(!Doing::of(Some("api")).works);
        assert!(Doing::of(Some("WORKER ")).works);
    }
}
