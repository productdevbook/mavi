#![allow(clippy::wildcard_imports)]

use super::super::*;

pub(super) fn analytics_routes() -> Router<HttpState> {
    Router::new()
        .route("/public/v1/analytics/events", post(record_analytics_events))
        .route("/api/v1/analytics/events", get(list_analytics_events))
        .route("/api/v1/analytics/daily", get(list_analytics_daily))
        .route("/api/v1/analytics/prune", post(prune_analytics))
}

async fn record_analytics_events(
    State(state): State<HttpState>,
    Extension(context): Extension<SiteContext>,
    Json(input): Json<AnalyticsEventBatch>,
) -> Result<(StatusCode, Json<AnalyticsReceipt>), HttpError> {
    let mut transaction = state.runtime.begin(&context).await.map_err(HttpError)?;
    let receipt = state
        .analytics
        .record_batch(&mut transaction, &context, &input)
        .await
        .map_err(HttpError)?;
    transaction.commit().await.map_err(HttpError)?;
    Ok((StatusCode::ACCEPTED, Json(receipt)))
}

async fn list_analytics_events(
    State(state): State<HttpState>,
    Extension(context): Extension<SiteContext>,
    Query(filter): Query<EventListFilter>,
) -> Result<Json<Page<AnalyticsEvent>>, HttpError> {
    require_analytics_grant(
        &state,
        &context,
        Action::View,
        "AnalyticsEvent",
        "event_collection",
    )?;
    let mut transaction = state.runtime.begin(&context).await.map_err(HttpError)?;
    let page = state
        .analytics
        .list_events(&mut transaction, &filter)
        .await
        .map_err(HttpError)?;
    transaction.commit().await.map_err(HttpError)?;
    Ok(Json(page))
}

async fn list_analytics_daily(
    State(state): State<HttpState>,
    Extension(context): Extension<SiteContext>,
    Query(filter): Query<DailyListFilter>,
) -> Result<Json<Page<DailyAggregate>>, HttpError> {
    require_analytics_grant(
        &state,
        &context,
        Action::View,
        "AnalyticsDaily",
        "daily_collection",
    )?;
    let mut transaction = state.runtime.begin(&context).await.map_err(HttpError)?;
    let page = state
        .analytics
        .list_daily(&mut transaction, &filter)
        .await
        .map_err(HttpError)?;
    transaction.commit().await.map_err(HttpError)?;
    Ok(Json(page))
}

async fn prune_analytics(
    State(state): State<HttpState>,
    Extension(context): Extension<SiteContext>,
    Json(input): Json<PruneAnalytics>,
) -> Result<Json<PruneReceipt>, HttpError> {
    require_analytics_grant(
        &state,
        &context,
        Action::Delete,
        "AnalyticsRetention",
        "retention",
    )?;
    let mut transaction = state.runtime.begin(&context).await.map_err(HttpError)?;
    let receipt = state
        .analytics
        .prune(&mut transaction, &context, &input)
        .await
        .map_err(HttpError)?;
    transaction.commit().await.map_err(HttpError)?;
    Ok(Json(receipt))
}
