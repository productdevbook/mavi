//! The router, and what every request goes through on its way in.
//!
//! An [`Endpoint`] says what it takes, what it gives and who may reach it, and
//! the router builds the description of the API out of those declarations —
//! so a path that is served and a path that is documented cannot differ.
//!
//! The layers, in order: which site this address is, who is asking, what they
//! may do, and how often they may ask. A handler is reached with those already
//! decided, which is why none of them can be forgotten in one.
use std::collections::HashSet;
use std::sync::Arc;

use axum::extract::{FromRequestParts, Request, State};
use axum::http::header::{AUTHORIZATION, COOKIE, HOST, HeaderName};
use axum::http::request::Parts;
use axum::middleware::{Next, from_fn_with_state};
use axum::response::{IntoResponse, Response};
use axum::routing::{MethodRouter, delete, get, patch, post, put};
use axum::{Router, middleware};
use uuid::Uuid;

use super::authz::{self, Needs, Permit, Principal};
use super::clock::{Clock, SystemClock};
use super::db::Db;
use super::error::{AppError, Result};
use super::outside::Outside;
use super::ratelimit::{self, Limit};
use super::tenant::{Site, TenantId, resolve_host};

pub const USER_COOKIE: &str = "mavi_user";
pub const STUDENT_COOKIE: &str = "mavi_student";
pub const OPERATOR_COOKIE: &str = "mavi_operator";
pub const REQUEST_ID_HEADER: HeaderName = HeaderName::from_static("x-request-id");

#[derive(Clone, Debug)]
pub struct AppState {
    pub db: Db,
    pub clock: Arc<dyn Clock>,
    /// How many proxies sit in front. Zero means the forwarded-for header is
    /// not believed at all, which is the default: believing one that nothing
    /// wrote lets a caller choose the rate limit key it is counted under.
    pub proxy_hops: usize,
    /// Whether an outbound webhook may reach a private address. False
    /// everywhere but a test, which has nowhere else to put its receiver.
    pub allow_private_destinations: bool,
    pub store: Arc<super::storage::Store>,
    /// What a site's stored credentials are sealed with.
    pub keyring: Arc<super::crypto::Keyring>,
    /// Where mail goes. Recorded where nothing is configured, which is a
    /// machine that is obviously not sending rather than quietly not sending.
    pub mailer: Arc<super::mailer::Mailer>,
    /// Whoever takes the money. Absent where none is configured, which refuses
    /// rather than pretends.
    pub payments: Arc<super::payments::Payments>,
    /// Who turns a site's files into pages. Direct where nothing is
    /// configured, which is a site whose files are its pages.
    pub builder: Arc<super::builder::Builder>,
    /// Who turns an uploaded video into something a browser can play.
    pub transcoder: Arc<super::transcoder::Transcoder>,
    /// What something outside this crate added: its own endpoints, its own
    /// kinds of work. Empty where nothing is built against this crate, which
    /// is this crate on its own.
    pub outside: Arc<Outside>,
}

impl AppState {
    /// Whether a transcoder's answer was signed by the transcoder this machine
    /// hands work to. Signed the same way a webhook is, with the key it was
    /// given, because it is the same problem: something on the network saying
    /// a thing about a site.
    #[must_use]
    pub fn transcoder_signature_holds(&self, body: &str, signature: &str) -> bool {
        match &*self.transcoder {
            super::transcoder::Transcoder::AsUploaded => false,
            super::transcoder::Transcoder::Elsewhere(elsewhere) => {
                let expected = super::webhook::sign(
                    &super::secret::Secret::new(elsewhere.key.expose().as_bytes().to_vec()),
                    "video",
                    0,
                    body,
                );

                super::secret::same(expected.as_bytes(), signature.as_bytes())
            }
        }
    }

    #[must_use]
    pub fn new(db: Db) -> Self {
        Self {
            db,
            clock: Arc::new(SystemClock),
            proxy_hops: 0,
            allow_private_destinations: false,
            // Where what anybody uploaded is kept. The image sets this to a
            // volume; unset, it is a directory beside the process, which is
            // right while somebody is looking at it on their own machine and
            // is lost on the first restart anywhere else. `UPLOADS_DIR` is
            // what this was called before the project had its own name.
            store: Arc::new(super::storage::Store::Disk(super::storage::LocalDisk::at(
                std::env::var("MAVI_DATA_DIR")
                    .or_else(|_| std::env::var("UPLOADS_DIR"))
                    .unwrap_or_else(|_| "uploads".to_owned()),
            ))),
            // Invented where none was given, which means a machine restarted
            // without its key cannot open what the last one sealed — and says
            // so by failing rather than by pretending.
            mailer: Arc::new(super::mailer::Mailer::from_env()),
            payments: Arc::new(super::payments::Payments::from_env()),
            builder: Arc::new(super::builder::Builder::from_env()),
            transcoder: Arc::new(super::transcoder::Transcoder::from_env()),
            keyring: Arc::new(
                std::env::var("MAVI_KEYS")
                    .ok()
                    .and_then(|raw| super::crypto::Keyring::from_env(&raw).ok())
                    .unwrap_or_else(super::crypto::Keyring::invented),
            ),
            outside: Arc::new(Outside::default()),
        }
    }
}

#[derive(Clone, Copy, Debug)]
pub struct RequestId(pub Uuid);

/// Whoever runs the machine, on a request to its own screens.
#[derive(Clone, Copy, Debug)]
pub struct Console {
    pub operator_id: Option<Uuid>,
    pub request_id: RequestId,
    pub ip: Option<std::net::IpAddr>,
}

impl Console {
    pub fn require_operator(&self) -> Result<Uuid> {
        self.operator_id.ok_or(AppError::Unauthenticated)
    }
}

impl<S: Send + Sync> FromRequestParts<S> for Console {
    type Rejection = AppError;

    async fn from_request_parts(parts: &mut Parts, _state: &S) -> Result<Self> {
        parts
            .extensions
            .get::<Console>()
            .copied()
            .ok_or(AppError::Unauthenticated)
    }
}

#[derive(Clone, Debug)]
pub struct Caller {
    pub site: Site,
    pub request_id: RequestId,
    pub ip: Option<String>,
    pub user: Option<SignedIn>,
    /// Somebody learning on the site's own front. A different table, a
    /// different cookie, and no grants at all: a student's token opens nothing
    /// in the panel, and a panel account is not a student.
    pub student: Option<SignedInStudent>,
}

#[derive(Clone, Copy, Debug)]
pub struct SignedInStudent {
    pub student_id: Uuid,
    pub session_id: Uuid,
}

#[derive(Clone, Debug)]
pub struct SignedIn {
    pub user_id: Uuid,
    pub session_id: Uuid,
    pub role_key: String,
    pub grants: HashSet<String>,
}

impl Caller {
    #[must_use]
    pub fn tenant(&self) -> TenantId {
        self.site.tenant
    }

    /// Who is asking, where a handler needs them to be somebody.
    pub fn require_user(&self) -> Result<&SignedIn> {
        self.user.as_ref().ok_or(AppError::Unauthenticated)
    }

    pub fn require_student(&self) -> Result<SignedInStudent> {
        self.student.ok_or(AppError::Unauthenticated)
    }

    /// Asked again with the record in hand. The guard on the route knows the
    /// capability and nothing about whose the row is; only the handler that
    /// has read it does, which is why record-level questions are asked here.
    pub fn may(&self, needs: Needs, owner: Option<Uuid>) -> Result<Permit> {
        authz::check(
            &self.principal()?,
            needs,
            authz::Resource::Site {
                id: self.site.tenant.0,
                frozen: self.site.frozen,
            },
            owner,
        )
    }

    pub fn principal(&self) -> Result<Principal> {
        let user = self.user.as_ref().ok_or(AppError::Unauthenticated)?;

        Ok(Principal::SiteUser {
            id: user.user_id,
            site: self.site.tenant.0,
            grants: user.grants.clone(),
        })
    }
}

impl<S: Send + Sync> FromRequestParts<S> for Caller {
    type Rejection = AppError;

    async fn from_request_parts(parts: &mut Parts, _state: &S) -> Result<Self> {
        parts
            .extensions
            .get::<Caller>()
            .cloned()
            .ok_or(AppError::Unauthenticated)
    }
}

/// Handed to a handler only where the engine said yes, and taken by the query
/// functions in a domain — so a read that never asked does not compile.
impl<S: Send + Sync> FromRequestParts<S> for Permit {
    type Rejection = AppError;

    async fn from_request_parts(parts: &mut Parts, _state: &S) -> Result<Self> {
        parts
            .extensions
            .get::<Permit>()
            .copied()
            .ok_or(AppError::Forbidden)
    }
}

/// Who a route is for. There is no default: an endpoint names one.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Audience {
    User,
    Student,
    Public,
    /// The machine's own screens. Reached on an address that is nobody's site,
    /// so these are mounted outside the resolution every other route goes
    /// through, and a site's grants never add up to one of them.
    Operator,
}

/// Named rather than optional, so choosing no limit is a choice somebody made.
#[derive(Clone, Copy, Debug)]
pub enum RatePolicy {
    None,
    Per(Limit),
}

/// What a route declares before it can be mounted.
#[derive(Clone, Copy, Debug)]
pub struct Guard {
    pub audience: Audience,
    /// `None` only where nothing is being reached on a site's behalf — signing
    /// in, and the endpoints a visitor uses.
    pub needs: Option<Needs>,
    pub rate: RatePolicy,
}

#[derive(Clone)]
pub struct Endpoint {
    path: &'static str,
    method: &'static str,
    guard: Guard,
    domain: &'static str,
    /// What it takes and what it gives back, for the description of the API
    /// that everything else is generated from. There is one list of endpoints,
    /// so there is one description; nothing here is written twice.
    pub(crate) shape: Shape,
    router: MethodRouter<AppState>,
}

#[derive(Clone, Default)]
pub(crate) struct Shape {
    pub takes: Option<utoipa::openapi::RefOr<utoipa::openapi::Schema>>,
    pub gives: Option<utoipa::openapi::RefOr<utoipa::openapi::Schema>>,
    pub named: Vec<(String, utoipa::openapi::RefOr<utoipa::openapi::Schema>)>,
}

impl std::fmt::Debug for Endpoint {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Endpoint")
            .field("path", &self.path)
            .field("guard", &self.guard)
            .finish_non_exhaustive()
    }
}

macro_rules! verb {
    ($name:ident, $method:ident, $verb:literal) => {
        pub fn $name<H, T>(path: &'static str, guard: Guard, handler: H) -> Self
        where
            H: axum::handler::Handler<T, AppState>,
            T: 'static,
        {
            Self::new(path, $verb, guard, $method(handler))
        }
    };
}

impl Endpoint {
    verb!(get, get, "get");
    verb!(post, post, "post");
    verb!(put, put, "put");
    verb!(patch, patch, "patch");
    verb!(delete, delete, "delete");

    /// Says what this endpoint takes and gives. Types that say what they are —
    /// everything but the handful that take nothing.
    #[must_use]
    pub fn takes<T: utoipa::PartialSchema + utoipa::ToSchema>(mut self) -> Self {
        self.shape.takes = Some(T::schema());
        self.shape.named.extend(named_schemas::<T>());
        self
    }

    #[must_use]
    pub fn gives<T: utoipa::PartialSchema + utoipa::ToSchema>(mut self) -> Self {
        self.shape.gives = Some(T::schema());
        self.shape.named.extend(named_schemas::<T>());
        self
    }

    /// Which domain this belongs to. Set where the endpoints are gathered
    /// rather than at each one: a domain is what a module is, and saying it on
    /// every endpoint is a hundred chances to say it differently.
    #[must_use]
    pub fn within(mut self, domain: &'static str) -> Self {
        self.domain = domain;
        self
    }

    #[must_use]
    pub fn domain(&self) -> &'static str {
        self.domain
    }

    #[must_use]
    pub fn method(&self) -> &'static str {
        self.method
    }

    #[must_use]
    pub fn path(&self) -> &'static str {
        self.path
    }

    #[must_use]
    pub fn guard(&self) -> Guard {
        self.guard
    }

    fn new(
        path: &'static str,
        method: &'static str,
        guard: Guard,
        router: MethodRouter<AppState>,
    ) -> Self {
        Self {
            path,
            method,
            guard,
            domain: "",
            shape: Shape::default(),
            router,
        }
    }
}

impl std::fmt::Debug for Shape {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Shape")
            .field("takes", &self.takes.is_some())
            .field("gives", &self.gives.is_some())
            .finish_non_exhaustive()
    }
}

/// Names that hold other things rather than being things.
///
/// A `Vec<Post>` is not a shape called `Vec`, and a `Page<Post>` is not a shape
/// called `Page`: registering them that way put one component under each name
/// and left whichever endpoint was described last. What is inside them is
/// registered by `schemas`, which is the part anybody refers to.
const CONTAINERS: [&str; 2] = ["Vec", "Page"];

fn named_schemas<T: utoipa::ToSchema>()
-> Vec<(String, utoipa::openapi::RefOr<utoipa::openapi::Schema>)> {
    let mut all = Vec::new();
    T::schemas(&mut all);

    let name = T::name().to_string();

    if !CONTAINERS.contains(&name.as_str()) {
        all.push((name, <T as utoipa::PartialSchema>::schema()));
    }

    all
}

#[derive(Clone, Copy, Debug)]
struct Declared {
    path: &'static str,
    method: &'static str,
    guard: Guard,
    domain: &'static str,
}

pub fn mount(state: AppState, endpoints: Vec<Endpoint>) -> Router {
    let (console, sites): (Vec<Endpoint>, Vec<Endpoint>) = endpoints
        .into_iter()
        .partition(|endpoint| endpoint.guard.audience == Audience::Operator);

    let mut router = Router::new();

    for endpoint in sites {
        let declared = Declared {
            path: endpoint.path,
            method: endpoint.method,
            guard: endpoint.guard,
            domain: endpoint.domain,
        };

        let guarded = endpoint.router.route_layer(from_fn_with_state(
            state.clone(),
            move |state: State<AppState>, request: Request, next: Next| {
                admit(state, declared, request, next)
            },
        ));

        router = router.route(endpoint.path, guarded);
    }

    let mut own = Router::new();

    for endpoint in console {
        let declared = Declared {
            path: endpoint.path,
            method: endpoint.method,
            guard: endpoint.guard,
            domain: endpoint.domain,
        };

        let guarded = endpoint.router.route_layer(from_fn_with_state(
            state.clone(),
            move |state: State<AppState>, request: Request, next: Next| {
                admit_operator(state, declared, request, next)
            },
        ));

        own = own.route(endpoint.path, guarded);
    }

    router
        // Anything on a site's own address that is not one of its endpoints is
        // one of its pages. Inside the layer that resolves the address, because
        // which site's page it is is the whole question.
        .fallback(crate::edge::serve)
        .layer(middleware::from_fn_with_state(state.clone(), identify))
        // Outside the resolution: the machine's own screens are reached on an
        // address that is nobody's site, and everything above would turn them
        // away.
        .merge(own.layer(middleware::from_fn_with_state(
            state.clone(),
            identify_operator,
        )))
        // Outside the layer: a probe arrives at an address no site claims, and
        // so does whatever is scraping the numbers.
        // Alive is not ready. A pod whose database has gone is not something
        // to restart — restarting it does not bring the database back — so
        // liveness stays cheap and readiness is what asks.
        .route("/healthz", get(|| async { "ok" }))
        .route("/readyz", get(ready))
        .route("/metrics", get(|| async { super::metrics::render() }))
        .route(
            "/openapi.json",
            get(|State(state): State<AppState>| async move {
                axum::Json(crate::openapi_with(&state.outside))
            }),
        )
        .layer(middleware::from_fn(super::metrics::observe))
        // Outermost, so that what a browser is told is on every answer this
        // machine gives, including the ones nothing above ever sees.
        .layer(middleware::from_fn(super::browser::told))
        .with_state(state)
}

/// Whether this process can do anything useful: it has a database and the
/// database answers. A probe that says yes without asking is a probe that sends
/// traffic to a pod that cannot serve it.
async fn ready(State(state): State<AppState>) -> Response {
    let asked = tokio::time::timeout(std::time::Duration::from_secs(2), async {
        let mut conn = state.db.operator().await?;
        sqlx::query("select 1").execute(conn.conn()).await?;
        conn.commit().await
    })
    .await;

    match asked {
        Ok(Ok(())) => "ready".into_response(),
        Ok(Err(why)) => {
            tracing::warn!(error = %why, "not ready");
            (axum::http::StatusCode::SERVICE_UNAVAILABLE, "not ready").into_response()
        }
        Err(_) => {
            tracing::warn!("not ready: the database did not answer in time");
            (axum::http::StatusCode::SERVICE_UNAVAILABLE, "not ready").into_response()
        }
    }
}

async fn identify(
    State(state): State<AppState>,
    mut request: Request,
    next: Next,
) -> Result<Response> {
    let request_id = RequestId(Uuid::now_v7());

    let host = request
        .headers()
        .get(HOST)
        .and_then(|value| value.to_str().ok())
        .ok_or(AppError::UnknownHost)?
        .to_owned();

    let site = resolve_host(&state.db, &host).await?;
    let ip = client_ip(&request, state.proxy_hops);

    // A cookie is sent by the browser whichever page asked, so a change made
    // with one is asked where it came from. A bearer token is not sent by
    // anybody's page.
    if bearer(&request).is_none() && cookie(&request, USER_COOKIE).is_some() {
        super::browser::asked_by_this_site(request.method(), request.headers(), &host)?;
    }

    let token = bearer(&request).or_else(|| cookie(&request, USER_COOKIE));
    let user = match token {
        Some(token) => session_of(&state.db, site.tenant, &token).await?,
        None => None,
    };

    // A student's token is looked for separately and never in the same place:
    // one cookie is not the other, and a bearer token is a panel account's.
    let student = match cookie(&request, STUDENT_COOKIE) {
        Some(token) => student_session(&state.db, site.tenant, &token).await?,
        None => None,
    };

    request.extensions_mut().insert(Caller {
        site,
        request_id,
        ip,
        user,
        student,
    });

    let mut response = next.run(request).await;

    response.headers_mut().insert(
        REQUEST_ID_HEADER,
        request_id
            .0
            .to_string()
            .parse()
            .map_err(|_| AppError::Bug("a request id is always a header value"))?,
    );

    Ok(response)
}

/// One span per request, named by the domain it belongs to, so that what is
/// slow reads as "the shop" rather than as a path — and one line in the numbers
/// per domain, for the same reason.
async fn admit(
    state: State<AppState>,
    declared: Declared,
    request: Request,
    next: Next,
) -> Result<Response> {
    use tracing::Instrument as _;

    let span = tracing::info_span!(
        "domain",
        domain = declared.domain,
        route = declared.path,
        method = declared.method,
    );

    let answered = admitting(state, declared, request, next)
        .instrument(span)
        .await;

    super::metrics::domain_answered(declared.domain, answered.is_ok());

    answered
}

async fn admitting(
    State(state): State<AppState>,
    declared: Declared,
    mut request: Request,
    next: Next,
) -> Result<Response> {
    let caller = request
        .extensions()
        .get::<Caller>()
        .cloned()
        .ok_or(AppError::Unauthenticated)?;

    // Whoever is asking, by account or by address. With neither — a request
    // that arrived on a socket nothing said the peer of, which in practice is
    // a test — there is nobody to count, and counting them all as one person
    // would make one caller's limit everybody's.
    if let RatePolicy::Per(limit) = declared.guard.rate
        && let Some(who) = caller
            .user
            .as_ref()
            .map(|user| user.user_id.to_string())
            .or_else(|| caller.ip.clone())
    {
        ratelimit::spend(
            &state.db,
            &format!("{}:{}:{who}", caller.tenant(), declared.path),
            limit,
        )
        .await?;
    }

    match declared.guard.audience {
        Audience::Public => {}
        // Mounted on its own router; nothing of a site's reaches one.
        Audience::Operator => return Err(AppError::Forbidden),
        Audience::User => {
            if caller.user.is_none() {
                return Err(AppError::Unauthenticated);
            }
        }
        Audience::Student => {
            if caller.student.is_none() {
                return Err(AppError::Unauthenticated);
            }
        }
    }

    if let Some(needs) = declared.guard.needs {
        let principal = caller.principal()?;
        let resource = authz::Resource::Site {
            id: caller.site.tenant.0,
            frozen: caller.site.frozen,
        };

        // The caller as the owner, because at this point there is no record to
        // ask about — a post being written does not exist yet, and somebody
        // holding only `content:write:own` is entitled to write one. What that
        // means is that this gate is the coarse one: it answers "may this
        // person do this kind of thing at all". A handler that then reaches a
        // particular record asks again with that record's owner, which is
        // where an author is told no about somebody else's.
        let owner = caller.user.as_ref().map(|user| user.user_id);

        match authz::check(&principal, needs, resource, owner) {
            Ok(permit) => {
                request.extensions_mut().insert(permit);
            }
            Err(refusal) => {
                refused(&state, &caller, declared.path, needs).await;
                return Err(refusal);
            }
        }
    }

    let changes = matches!(declared.method, "post" | "put" | "patch" | "delete");
    let response = next.run(request).await;

    // A mutation answers with what it wrote down, or it does not answer. The
    // check is here rather than in each handler because a handler that forgot
    // is exactly the one that would forget to check.
    if changes
        && response.status().is_success()
        && response.extensions().get::<super::audit::Wrote>().is_none()
    {
        tracing::error!(
            path = declared.path,
            "a change was made and nothing recorded it"
        );
        return Err(AppError::Bug("a change was made and nothing recorded it"));
    }

    Ok(response)
}

async fn identify_operator(
    State(state): State<AppState>,
    mut request: Request,
    next: Next,
) -> Result<Response> {
    let request_id = RequestId(Uuid::now_v7());

    if bearer(&request).is_none() && cookie(&request, OPERATOR_COOKIE).is_some() {
        let host = request
            .headers()
            .get(HOST)
            .and_then(|value| value.to_str().ok())
            .unwrap_or_default()
            .to_owned();

        super::browser::asked_by_this_site(request.method(), request.headers(), &host)?;
    }

    let token = bearer(&request).or_else(|| cookie(&request, OPERATOR_COOKIE));

    let operator_id = match token {
        Some(token) => operator_session(&state.db, &token).await?,
        None => None,
    };

    let ip = client_ip(&request, state.proxy_hops).and_then(|ip| ip.parse().ok());

    request.extensions_mut().insert(Console {
        operator_id,
        request_id,
        ip,
    });

    let mut response = next.run(request).await;

    response.headers_mut().insert(
        REQUEST_ID_HEADER,
        request_id
            .0
            .to_string()
            .parse()
            .map_err(|_| AppError::Bug("a request id is always a header value"))?,
    );

    Ok(response)
}

async fn admit_operator(
    state: State<AppState>,
    declared: Declared,
    request: Request,
    next: Next,
) -> Result<Response> {
    use tracing::Instrument as _;

    let span = tracing::info_span!(
        "domain",
        domain = declared.domain,
        route = declared.path,
        method = declared.method,
    );

    let answered = admitting_operator(state, declared, request, next)
        .instrument(span)
        .await;

    super::metrics::domain_answered(declared.domain, answered.is_ok());

    answered
}

async fn admitting_operator(
    State(state): State<AppState>,
    declared: Declared,
    request: Request,
    next: Next,
) -> Result<Response> {
    let console = request
        .extensions()
        .get::<Console>()
        .copied()
        .ok_or(AppError::Unauthenticated)?;

    // Whoever is asking, not "the console": one bucket for every sign-in
    // attempt on the machine is a way to lock the operator out by trying five
    // times from anywhere. With nobody identifiable — a request on a socket
    // nothing said the peer of, which in practice is a test — there is nobody
    // to count, and the sign-in itself counts by the account being aimed at.
    if let RatePolicy::Per(limit) = declared.guard.rate
        && let Some(who) = console
            .operator_id
            .map(|id| id.to_string())
            .or_else(|| console.ip.map(|ip| ip.to_string()))
    {
        ratelimit::spend(
            &state.db,
            &format!("console:{}:{who}", declared.path),
            limit,
        )
        .await?;
    }

    // Everything but signing in wants somebody signed in. There is no grant to
    // check: an operator is not a role on a site.
    if console.operator_id.is_none() && declared.guard.needs.is_some() {
        return Err(AppError::Unauthenticated);
    }

    Ok(next.run(request).await)
}

async fn operator_session(db: &Db, token: &str) -> Result<Option<Uuid>> {
    let hash = super::token::hash(token);
    let mut conn = db.operator().await?;

    let row: Option<(Uuid,)> = sqlx::query_as(
        "update operator_sessions s
            set last_seen_at = now()
           from operators o
          where s.token_hash = $1
            and s.operator_id = o.id
            and s.revoked_at is null
            and s.expires_at > now()
            and o.active
         returning o.id",
    )
    .bind(&hash[..])
    .fetch_optional(conn.conn())
    .await?;

    conn.commit().await?;

    Ok(row.map(|(id,)| id))
}

/// "Why could I not get in" is a question with an answer in the log.
async fn refused(state: &AppState, caller: &Caller, path: &str, needs: Needs) {
    let Ok(mut conn) = state.db.tenant(caller.tenant()).await else {
        return;
    };

    let written = sqlx::query(
        "insert into audit_log
             (tenant_id, actor_id, actor_kind, action, subject, subject_id, after, request_id)
         values ($1, $2, 'user', 'refused', 'permission', $3, $4, $5)",
    )
    .bind(caller.tenant().0)
    .bind(caller.user.as_ref().map(|user| user.user_id))
    .bind(needs.grant())
    .bind(serde_json::json!({ "path": path }))
    .bind(caller.request_id.0.to_string())
    .execute(conn.conn())
    .await;

    if written.is_ok() {
        let _ = conn.commit().await;
    }
}

async fn session_of(db: &Db, tenant: TenantId, token: &str) -> Result<Option<SignedIn>> {
    let hash = super::token::hash(token);
    let mut conn = db.tenant(tenant).await?;

    let row: Option<(Uuid, Uuid, String, Vec<String>)> = sqlx::query_as(
        "update sessions s
            set last_seen_at = now()
           from users u
           join roles r on r.id = u.role_id
          where s.token_hash = $1
            and s.user_id = u.id
            and s.revoked_at is null
            and s.expires_at > now()
            and u.state = 'active'
            and u.deleted_at is null
         returning s.id, u.id, r.key, r.grants",
    )
    .bind(&hash[..])
    .fetch_optional(conn.conn())
    .await?;

    conn.commit().await?;

    Ok(row.map(|(session_id, user_id, role_key, grants)| SignedIn {
        user_id,
        session_id,
        role_key,
        grants: grants.into_iter().collect(),
    }))
}

async fn student_session(
    db: &Db,
    tenant: TenantId,
    token: &str,
) -> Result<Option<SignedInStudent>> {
    let hash = super::token::hash(token);
    let mut conn = db.tenant(tenant).await?;

    let row: Option<(Uuid, Uuid)> = sqlx::query_as(
        "update student_sessions s
            set last_seen_at = now()
           from students t
          where s.token_hash = $1
            and s.student_id = t.id
            and s.revoked_at is null
            and s.expires_at > now()
            and t.state = 'active'
            and t.deleted_at is null
         returning s.id, t.id",
    )
    .bind(&hash[..])
    .fetch_optional(conn.conn())
    .await?;

    conn.commit().await?;

    Ok(row.map(|(session_id, student_id)| SignedInStudent {
        student_id,
        session_id,
    }))
}

fn bearer(request: &Request) -> Option<String> {
    request
        .headers()
        .get(AUTHORIZATION)
        .and_then(|value| value.to_str().ok())
        .and_then(|value| value.strip_prefix("Bearer "))
        .map(str::to_owned)
}

fn cookie(request: &Request, name: &str) -> Option<String> {
    request
        .headers()
        .get_all(COOKIE)
        .iter()
        .filter_map(|value| value.to_str().ok())
        .flat_map(|value| value.split(';'))
        .filter_map(|pair| pair.trim().split_once('='))
        .find(|(key, _)| *key == name)
        .map(|(_, value)| value.to_owned())
}

/// Counted from the right, one entry per proxy in front, because everything to
/// the left of what this machine's own ingress wrote was written by whoever was
/// calling. With no proxy configured the header is ignored entirely and the
/// address the socket came from is what counts.
fn client_ip(request: &Request, proxy_hops: usize) -> Option<String> {
    let peer = request
        .extensions()
        .get::<axum::extract::ConnectInfo<std::net::SocketAddr>>()
        .map(|info| info.0.ip().to_string());

    if proxy_hops == 0 {
        return peer;
    }

    let forwarded: Vec<&str> = request
        .headers()
        .get("x-forwarded-for")
        .and_then(|value| value.to_str().ok())
        .map(|value| value.split(',').map(str::trim).collect())
        .unwrap_or_default();

    forwarded
        .len()
        .checked_sub(proxy_hops)
        .and_then(|index| forwarded.get(index))
        .filter(|value| !value.is_empty())
        .map(|value| (*value).to_owned())
        .or(peer)
}

#[cfg(test)]
mod tests {
    use axum::body::Body;

    use super::*;

    fn asking_from(forwarded: &str) -> Request {
        Request::builder()
            .header("x-forwarded-for", forwarded)
            .body(Body::empty())
            .expect("a request")
    }

    #[test]
    fn a_caller_cannot_choose_what_it_is_counted_as() {
        let request = asking_from("1.1.1.1, 2.2.2.2, 3.3.3.3");

        assert_eq!(client_ip(&request, 0), None);
        assert_eq!(client_ip(&request, 1).as_deref(), Some("3.3.3.3"));
        assert_eq!(client_ip(&request, 2).as_deref(), Some("2.2.2.2"));
    }

    #[test]
    fn a_header_shorter_than_the_hops_falls_back_rather_than_trusting_it() {
        assert_eq!(client_ip(&asking_from("9.9.9.9"), 2), None);
    }
}
