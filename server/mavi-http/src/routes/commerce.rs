#![allow(clippy::wildcard_imports)]

use super::super::*;

pub(super) fn shop_routes() -> Router<HttpState> {
    Router::new()
        .route(
            "/api/v1/shop/products",
            get(list_shop_products).post(create_shop_product),
        )
        .route(
            "/api/v1/shop/products/{id}",
            get(read_shop_product)
                .patch(update_shop_product)
                .delete(delete_shop_product),
        )
        .route("/public/v1/shop/products", get(list_public_shop_products))
        .route(
            "/api/v1/shop/coupons",
            get(list_shop_coupons).post(create_shop_coupon),
        )
        .route("/api/v1/shop/coupons/{id}", delete(delete_shop_coupon))
        .route("/api/v1/shop/orders", get(list_shop_orders))
        .route("/api/v1/shop/orders/{id}", get(read_shop_order))
        .route(
            "/api/v1/shop/orders/{id}/transition",
            post(transition_shop_order),
        )
        .route("/public/v1/shop/orders", post(checkout_shop_order))
}

async fn list_shop_products(
    State(state): State<HttpState>,
    Extension(context): Extension<SiteContext>,
    Query(filter): Query<ProductListFilter>,
) -> Result<Json<Page<Product>>, HttpError> {
    require_grant_for(
        &state,
        &context,
        Grant::new(Capability::Shop, Action::View),
        "ShopProduct",
        "shop_products",
    )?;
    let mut transaction = state.runtime.begin(&context).await.map_err(HttpError)?;
    let products = state
        .shop
        .list_products(&mut transaction, &context, &filter)
        .await
        .map_err(HttpError)?;
    transaction.commit().await.map_err(HttpError)?;
    Ok(Json(products))
}

async fn create_shop_product(
    State(state): State<HttpState>,
    Extension(context): Extension<SiteContext>,
    Json(input): Json<CreateProduct>,
) -> Result<(StatusCode, Json<Product>), HttpError> {
    require_grant_for(
        &state,
        &context,
        Grant::new(Capability::Shop, Action::Write),
        "ShopProduct",
        "shop_products",
    )?;
    let mut transaction = state.runtime.begin(&context).await.map_err(HttpError)?;
    let product = state
        .shop
        .create_product(&mut transaction, &context, &input)
        .await
        .map_err(HttpError)?;
    transaction.commit().await.map_err(HttpError)?;
    Ok((StatusCode::CREATED, Json(product)))
}

async fn read_shop_product(
    State(state): State<HttpState>,
    Extension(context): Extension<SiteContext>,
    Path(id): Path<ProductId>,
) -> Result<Json<Product>, HttpError> {
    require_grant_for(
        &state,
        &context,
        Grant::new(Capability::Shop, Action::View),
        "ShopProduct",
        id.to_string(),
    )?;
    let mut transaction = state.runtime.begin(&context).await.map_err(HttpError)?;
    let product = state
        .shop
        .get_product(&mut transaction, &context, id)
        .await
        .map_err(HttpError)?;
    transaction.commit().await.map_err(HttpError)?;
    Ok(Json(product))
}

async fn update_shop_product(
    State(state): State<HttpState>,
    Extension(context): Extension<SiteContext>,
    Path(id): Path<ProductId>,
    Json(input): Json<UpdateProduct>,
) -> Result<Json<Product>, HttpError> {
    require_grant_for(
        &state,
        &context,
        Grant::new(Capability::Shop, Action::Write),
        "ShopProduct",
        id.to_string(),
    )?;
    let mut transaction = state.runtime.begin(&context).await.map_err(HttpError)?;
    let product = state
        .shop
        .update_product(&mut transaction, &context, id, &input)
        .await
        .map_err(HttpError)?;
    transaction.commit().await.map_err(HttpError)?;
    Ok(Json(product))
}

async fn delete_shop_product(
    State(state): State<HttpState>,
    Extension(context): Extension<SiteContext>,
    Path(id): Path<ProductId>,
) -> Result<StatusCode, HttpError> {
    require_grant_for(
        &state,
        &context,
        Grant::new(Capability::Shop, Action::Delete),
        "ShopProduct",
        id.to_string(),
    )?;
    let mut transaction = state.runtime.begin(&context).await.map_err(HttpError)?;
    state
        .shop
        .delete_product(&mut transaction, &context, id)
        .await
        .map_err(HttpError)?;
    transaction.commit().await.map_err(HttpError)?;
    Ok(StatusCode::NO_CONTENT)
}

async fn list_public_shop_products(
    State(state): State<HttpState>,
    Extension(context): Extension<SiteContext>,
    Query(filter): Query<PublicProductListFilter>,
) -> Result<Json<Page<PublicProduct>>, HttpError> {
    let mut transaction = state.runtime.begin(&context).await.map_err(HttpError)?;
    let products = state
        .shop
        .list_public_products(&mut transaction, &context, &filter)
        .await
        .map_err(HttpError)?;
    transaction.commit().await.map_err(HttpError)?;
    Ok(Json(products))
}

async fn list_shop_coupons(
    State(state): State<HttpState>,
    Extension(context): Extension<SiteContext>,
    Query(filter): Query<CouponListFilter>,
) -> Result<Json<Page<Coupon>>, HttpError> {
    require_grant_for(
        &state,
        &context,
        Grant::new(Capability::Shop, Action::View),
        "ShopCoupon",
        "shop_coupons",
    )?;
    let mut transaction = state.runtime.begin(&context).await.map_err(HttpError)?;
    let coupons = state
        .shop
        .list_coupons(&mut transaction, &context, &filter)
        .await
        .map_err(HttpError)?;
    transaction.commit().await.map_err(HttpError)?;
    Ok(Json(coupons))
}

async fn create_shop_coupon(
    State(state): State<HttpState>,
    Extension(context): Extension<SiteContext>,
    Json(input): Json<CreateCoupon>,
) -> Result<(StatusCode, Json<Coupon>), HttpError> {
    require_grant_for(
        &state,
        &context,
        Grant::new(Capability::Shop, Action::Write),
        "ShopCoupon",
        "shop_coupons",
    )?;
    let mut transaction = state.runtime.begin(&context).await.map_err(HttpError)?;
    let coupon = state
        .shop
        .create_coupon(&mut transaction, &context, &input)
        .await
        .map_err(HttpError)?;
    transaction.commit().await.map_err(HttpError)?;
    Ok((StatusCode::CREATED, Json(coupon)))
}

async fn delete_shop_coupon(
    State(state): State<HttpState>,
    Extension(context): Extension<SiteContext>,
    Path(id): Path<CouponId>,
) -> Result<StatusCode, HttpError> {
    require_grant_for(
        &state,
        &context,
        Grant::new(Capability::Shop, Action::Delete),
        "ShopCoupon",
        id.to_string(),
    )?;
    let mut transaction = state.runtime.begin(&context).await.map_err(HttpError)?;
    state
        .shop
        .delete_coupon(&mut transaction, &context, id)
        .await
        .map_err(HttpError)?;
    transaction.commit().await.map_err(HttpError)?;
    Ok(StatusCode::NO_CONTENT)
}

async fn list_shop_orders(
    State(state): State<HttpState>,
    Extension(context): Extension<SiteContext>,
    Query(filter): Query<OrderListFilter>,
) -> Result<Json<Page<OrderSummary>>, HttpError> {
    require_grant_for(
        &state,
        &context,
        Grant::new(Capability::Shop, Action::View),
        "ShopOrder",
        "shop_orders",
    )?;
    let mut transaction = state.runtime.begin(&context).await.map_err(HttpError)?;
    let orders = state
        .shop
        .list_orders(&mut transaction, &context, &filter)
        .await
        .map_err(HttpError)?;
    transaction.commit().await.map_err(HttpError)?;
    Ok(Json(orders))
}

async fn read_shop_order(
    State(state): State<HttpState>,
    Extension(context): Extension<SiteContext>,
    Path(id): Path<OrderId>,
) -> Result<Json<Order>, HttpError> {
    require_grant_for(
        &state,
        &context,
        Grant::new(Capability::Shop, Action::View),
        "ShopOrder",
        id.to_string(),
    )?;
    let mut transaction = state.runtime.begin(&context).await.map_err(HttpError)?;
    let order = state
        .shop
        .get_order(&mut transaction, &context, id)
        .await
        .map_err(HttpError)?;
    transaction.commit().await.map_err(HttpError)?;
    Ok(Json(order))
}

async fn transition_shop_order(
    State(state): State<HttpState>,
    Extension(context): Extension<SiteContext>,
    Path(id): Path<OrderId>,
    Json(input): Json<OrderTransition>,
) -> Result<Json<Order>, HttpError> {
    require_grant_for(
        &state,
        &context,
        Grant::new(Capability::Shop, Action::Write),
        "ShopOrder",
        id.to_string(),
    )?;
    let mut transaction = state.runtime.begin(&context).await.map_err(HttpError)?;
    let order = state
        .shop
        .transition_order(&mut transaction, &context, id, &input)
        .await
        .map_err(HttpError)?;
    transaction.commit().await.map_err(HttpError)?;
    Ok(Json(order))
}

async fn checkout_shop_order(
    State(state): State<HttpState>,
    Extension(context): Extension<SiteContext>,
    Json(input): Json<CheckoutInput>,
) -> Result<(StatusCode, Json<CheckoutReceipt>), HttpError> {
    let mut transaction = state.runtime.begin(&context).await.map_err(HttpError)?;
    let receipt = state
        .shop
        .checkout(&mut transaction, &context, &input)
        .await
        .map_err(HttpError)?;
    transaction.commit().await.map_err(HttpError)?;
    Ok((StatusCode::CREATED, Json(receipt)))
}
