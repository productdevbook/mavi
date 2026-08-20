use chrono::{DateTime, Utc};
use mavi_contract::{Endpoint, Method, Permission, Shape};
use mavi_core::{
    Action, Capability, Currency, ErrorCode, MaviError, Money, Page, PageRequest, ProductId,
    Result, SiteContext,
};
use mavi_storage::SiteTx;
use serde::{Deserialize, Serialize};
use serde_json::json;
use sqlx::Row;

use crate::{ShopService, decode_cursor, encode_cursor};

pub const PRODUCT_NOT_FOUND: &str = "shop_product_not_found";
pub const PRODUCT_SLUG_INVALID: &str = "shop_product_slug_invalid";
pub const PRODUCT_SLUG_TAKEN: &str = "shop_product_slug_taken";
pub const PRODUCT_NAME_INVALID: &str = "shop_product_name_invalid";
pub const PRODUCT_DESCRIPTION_INVALID: &str = "shop_product_description_invalid";
pub const PRODUCT_PRICE_INVALID: &str = "shop_product_price_invalid";
pub const PRODUCT_STOCK_INVALID: &str = "shop_product_stock_invalid";
pub const PRODUCT_NOT_FOR_SALE: &str = "shop_product_not_for_sale";

const MAX_SLUG_CHARS: usize = 160;
const MAX_NAME_CHARS: usize = 300;
const MAX_DESCRIPTION_CHARS: usize = 10_000;
const MAX_STOCK: i32 = 1_000_000_000;

const PRODUCT_COLUMNS: &str = "id, slug, name, description, price_minor, currency, stock_on_hand, on_sale, created_at, updated_at";

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct ProductPrice {
    pub minor: i64,
    pub currency: String,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct CreateProduct {
    pub slug: String,
    pub name: String,
    #[serde(default)]
    pub description: Option<String>,
    pub price: ProductPrice,
    pub stock: i32,
    #[serde(default = "default_on_sale")]
    pub on_sale: bool,
}

#[derive(Clone, Debug, Default, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct UpdateProduct {
    pub name: Option<String>,
    /// `None` means unchanged; `Some(None)` explicitly clears the description.
    pub description: Option<Option<String>>,
    pub price_minor: Option<i64>,
    pub stock: Option<i32>,
    pub on_sale: Option<bool>,
}

#[derive(Clone, Debug, Default, Deserialize, Eq, PartialEq, Serialize)]
pub struct ProductListFilter {
    #[serde(flatten)]
    pub page: PageRequest,
}

#[derive(Clone, Debug, Default, Deserialize, Eq, PartialEq, Serialize)]
pub struct PublicProductListFilter {
    #[serde(flatten)]
    pub page: PageRequest,
}

#[derive(Clone, Debug, Serialize)]
pub struct Product {
    pub id: ProductId,
    pub slug: String,
    pub name: String,
    pub description: Option<String>,
    pub price: Money,
    pub stock: i32,
    pub on_sale: bool,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

#[derive(Clone, Debug, Serialize)]
pub struct PublicProduct {
    pub id: ProductId,
    pub slug: String,
    pub name: String,
    pub description: Option<String>,
    pub price: Money,
    pub can_be_bought: bool,
}

#[derive(Clone, Debug)]
pub(crate) struct ProductSnapshot {
    pub id: ProductId,
    pub name: String,
    pub price: Money,
    pub stock: i32,
    pub on_sale: bool,
}

fn default_on_sale() -> bool {
    true
}

pub fn api() -> mavi_contract::Api {
    mavi_contract::Api::new(endpoints()).with_shapes(shapes())
}

#[allow(clippy::too_many_lines)]
fn endpoints() -> Vec<Endpoint> {
    let view = Permission {
        capability: Capability::Shop,
        action: Action::View,
    };
    let write = Permission {
        capability: Capability::Shop,
        action: Action::Write,
    };
    let delete = Permission {
        capability: Capability::Shop,
        action: Action::Delete,
    };
    vec![
        Endpoint::new(
            Method::Get,
            "/api/v1/shop/products",
            "shop.products.list",
            "List site products with an opaque cursor",
        )
        .account_or_assistant()
        .requires(view)
        .takes_query("ProductListFilter")
        .returns(200, "ProductPage")
        .refuses([
            ErrorCode::Forbidden,
            ErrorCode::Validation,
            ErrorCode::Internal,
        ]),
        Endpoint::new(
            Method::Post,
            "/api/v1/shop/products",
            "shop.products.create",
            "Create a site product",
        )
        .account_or_assistant()
        .requires(write)
        .takes("CreateProduct")
        .returns(201, "Product")
        .changes(false)
        .refuses([
            ErrorCode::Forbidden,
            ErrorCode::Validation,
            ErrorCode::Conflict,
            ErrorCode::Internal,
        ]),
        Endpoint::new(
            Method::Get,
            "/api/v1/shop/products/{id}",
            "shop.products.read",
            "Read one site product",
        )
        .account_or_assistant()
        .requires(view)
        .returns(200, "Product")
        .refuses([
            ErrorCode::Forbidden,
            ErrorCode::NotFound,
            ErrorCode::Internal,
        ]),
        Endpoint::new(
            Method::Patch,
            "/api/v1/shop/products/{id}",
            "shop.products.update",
            "Update a product without changing its currency",
        )
        .account_or_assistant()
        .requires(write)
        .takes("UpdateProduct")
        .returns(200, "Product")
        .changes(true)
        .refuses([
            ErrorCode::Forbidden,
            ErrorCode::Validation,
            ErrorCode::NotFound,
            ErrorCode::Internal,
        ]),
        Endpoint::new(
            Method::Delete,
            "/api/v1/shop/products/{id}",
            "shop.products.delete",
            "Remove a product from the active catalog",
        )
        .account_or_assistant()
        .requires(delete)
        .returns(204, "Empty")
        .changes(true)
        .refuses([
            ErrorCode::Forbidden,
            ErrorCode::NotFound,
            ErrorCode::Internal,
        ]),
        Endpoint::new(
            Method::Get,
            "/public/v1/shop/products",
            "shop.public.products.list",
            "List products available to a public storefront",
        )
        .public()
        .takes_query("PublicProductListFilter")
        .returns(200, "PublicProductPage")
        .refuses([ErrorCode::Validation, ErrorCode::Internal]),
    ]
}

#[allow(clippy::too_many_lines)]
fn shapes() -> Vec<Shape> {
    vec![
        Shape::new(
            "ProductPrice",
            json!({
                "type": "object",
                "required": ["minor", "currency"],
                "additionalProperties": false,
                "properties": {
                    "minor": {"type": "integer", "format": "int64", "minimum": 0},
                    "currency": {"type": "string", "pattern": "^[A-Z]{3}$"}
                }
            }),
        ),
        Shape::new(
            "CreateProduct",
            json!({
                "type": "object",
                "required": ["slug", "name", "price", "stock"],
                "additionalProperties": false,
                "properties": {
                    "slug": {"type": "string", "minLength": 1, "maxLength": MAX_SLUG_CHARS},
                    "name": {"type": "string", "minLength": 1, "maxLength": MAX_NAME_CHARS},
                    "description": {"type": ["string", "null"], "maxLength": MAX_DESCRIPTION_CHARS},
                    "price": {"$ref": "#/components/schemas/ProductPrice"},
                    "stock": {"type": "integer", "minimum": 0, "maximum": MAX_STOCK},
                    "on_sale": {"type": "boolean"}
                }
            }),
        ),
        Shape::new(
            "UpdateProduct",
            json!({
                "type": "object",
                "additionalProperties": false,
                "properties": {
                    "name": {"type": ["string", "null"], "maxLength": MAX_NAME_CHARS},
                    "description": {"type": ["string", "null"], "maxLength": MAX_DESCRIPTION_CHARS},
                    "price_minor": {"type": ["integer", "null"], "format": "int64", "minimum": 0},
                    "stock": {"type": ["integer", "null"], "minimum": 0, "maximum": MAX_STOCK},
                    "on_sale": {"type": ["boolean", "null"]}
                }
            }),
        ),
        Shape::new(
            "ProductListFilter",
            json!({"type": "object", "properties": {
                "after": {"type": ["string", "null"], "maxLength": 512},
                "limit": {"type": "integer", "minimum": 1, "maximum": 100}
            }}),
        ),
        Shape::new(
            "PublicProductListFilter",
            json!({"type": "object", "properties": {
                "after": {"type": ["string", "null"], "maxLength": 512},
                "limit": {"type": "integer", "minimum": 1, "maximum": 100}
            }}),
        ),
        Shape::new(
            "Product",
            json!({
                "type": "object",
                "required": ["id", "slug", "name", "description", "price", "stock", "on_sale", "created_at", "updated_at"],
                "properties": {
                    "id": {"type": "string", "format": "uuid"},
                    "slug": {"type": "string"},
                    "name": {"type": "string"},
                    "description": {"type": ["string", "null"]},
                    "price": {"$ref": "#/components/schemas/Money"},
                    "stock": {"type": "integer"},
                    "on_sale": {"type": "boolean"},
                    "created_at": {"type": "string", "format": "date-time"},
                    "updated_at": {"type": "string", "format": "date-time"}
                }
            }),
        ),
        Shape::new(
            "PublicProduct",
            json!({
                "type": "object",
                "required": ["id", "slug", "name", "description", "price", "can_be_bought"],
                "properties": {
                    "id": {"type": "string", "format": "uuid"},
                    "slug": {"type": "string"},
                    "name": {"type": "string"},
                    "description": {"type": ["string", "null"]},
                    "price": {"$ref": "#/components/schemas/Money"},
                    "can_be_bought": {"type": "boolean"}
                }
            }),
        ),
        Shape::new(
            "Money",
            json!({
                "type": "object",
                "required": ["minor", "currency"],
                "properties": {
                    "minor": {"type": "integer", "format": "int64", "minimum": 0},
                    "currency": {"type": "string", "pattern": "^[A-Z]{3}$"}
                }
            }),
        ),
        Shape::new(
            "ProductPage",
            json!({"type": "object", "required": ["items", "next_cursor"], "properties": {
                "items": {"type": "array", "items": {"$ref": "#/components/schemas/Product"}},
                "next_cursor": {"type": ["string", "null"], "maxLength": 512}
            }}),
        ),
        Shape::new(
            "PublicProductPage",
            json!({"type": "object", "required": ["items", "next_cursor"], "properties": {
                "items": {"type": "array", "items": {"$ref": "#/components/schemas/PublicProduct"}},
                "next_cursor": {"type": ["string", "null"], "maxLength": 512}
            }}),
        ),
    ]
}

impl ShopService {
    pub async fn list_products(
        &self,
        tx: &mut SiteTx,
        context: &SiteContext,
        filter: &ProductListFilter,
    ) -> Result<Page<Product>> {
        list_product_rows(tx, context, &filter.page, false).await
    }

    pub async fn list_public_products(
        &self,
        tx: &mut SiteTx,
        context: &SiteContext,
        filter: &PublicProductListFilter,
    ) -> Result<Page<PublicProduct>> {
        let page = list_product_rows(tx, context, &filter.page, true).await?;
        Ok(Page::new(
            page.items
                .into_iter()
                .map(|product| PublicProduct {
                    id: product.id,
                    slug: product.slug,
                    name: product.name,
                    description: product.description,
                    price: product.price,
                    can_be_bought: product.stock > 0,
                })
                .collect(),
            page.next_cursor,
        ))
    }

    pub async fn get_product(
        &self,
        tx: &mut SiteTx,
        context: &SiteContext,
        id: ProductId,
    ) -> Result<Product> {
        let row = sqlx::QueryBuilder::<sqlx::Postgres>::new("select ")
            .push(PRODUCT_COLUMNS)
            .push(" from shop_products where site_id = ")
            .push_bind(context.site_id.into_uuid())
            .push(" and id = ")
            .push_bind(id.into_uuid())
            .push(" and deleted_at is null")
            .build()
            .fetch_optional(tx.conn())
            .await
            .map_err(|_| MaviError::Internal)?
            .ok_or(MaviError::NotFound {
                resource: PRODUCT_NOT_FOUND,
            })?;
        from_product_row(&row)
    }

    pub async fn create_product(
        &self,
        tx: &mut SiteTx,
        context: &SiteContext,
        input: &CreateProduct,
    ) -> Result<Product> {
        let slug = validate_slug(&input.slug)?;
        let name = validate_name(&input.name)?;
        let description = input
            .description
            .as_deref()
            .map(validate_description)
            .transpose()?;
        let price = validate_price(&input.price)?;
        let stock = validate_stock(input.stock)?;
        let id = ProductId::new();
        let row = sqlx::QueryBuilder::<sqlx::Postgres>::new(
            "insert into shop_products
                (site_id, id, slug, name, description, price_minor, currency, stock_on_hand, on_sale)
             values (",
        )
        .push_bind(context.site_id.into_uuid())
        .push(", ")
        .push_bind(id.into_uuid())
        .push(", ")
        .push_bind(&slug)
        .push(", ")
        .push_bind(&name)
        .push(", ")
        .push_bind(description.as_deref())
        .push(", ")
        .push_bind(price.minor)
        .push(", ")
        .push_bind(price.currency.to_string())
        .push(", ")
        .push_bind(stock)
        .push(", ")
        .push_bind(input.on_sale)
        .push(") returning ")
        .push(PRODUCT_COLUMNS)
        .build()
        .fetch_one(tx.conn())
        .await
        .map_err(|error| map_write_error(&error))?;
        let product = from_product_row(&row)?;
        crate::audit(
            tx,
            context,
            "shop.product.created",
            "ShopProduct",
            id.into_uuid(),
            json!({"slug": product.slug}),
        )
        .await?;
        Ok(product)
    }

    pub async fn update_product(
        &self,
        tx: &mut SiteTx,
        context: &SiteContext,
        id: ProductId,
        input: &UpdateProduct,
    ) -> Result<Product> {
        let name = input.name.as_deref().map(validate_name).transpose()?;
        let description = input
            .description
            .as_ref()
            .map(|value| value.as_deref().map(validate_description).transpose())
            .transpose()?;
        if let Some(price_minor) = input.price_minor {
            validate_minor(price_minor)?;
        }
        let stock = input.stock.map(validate_stock).transpose()?;
        if name.is_none()
            && input.description.is_none()
            && input.price_minor.is_none()
            && stock.is_none()
            && input.on_sale.is_none()
        {
            return self.get_product(tx, context, id).await;
        }
        let row = sqlx::QueryBuilder::<sqlx::Postgres>::new(
            "update shop_products
                set name = coalesce(",
        )
        .push_bind(name.as_deref())
        .push(", name), description = case when ")
        .push_bind(input.description.is_some())
        .push(" then ")
        .push_bind(description.flatten().as_deref())
        .push(" else description end, price_minor = coalesce(")
        .push_bind(input.price_minor)
        .push(", price_minor), stock_on_hand = coalesce(")
        .push_bind(stock)
        .push(", stock_on_hand), on_sale = coalesce(")
        .push_bind(input.on_sale)
        .push(
            ", on_sale), updated_at = clock_timestamp()
              where site_id = ",
        )
        .push_bind(context.site_id.into_uuid())
        .push(" and id = ")
        .push_bind(id.into_uuid())
        .push(" and deleted_at is null returning ")
        .push(PRODUCT_COLUMNS)
        .build()
        .fetch_optional(tx.conn())
        .await
        .map_err(|_| MaviError::Internal)?
        .ok_or(MaviError::NotFound {
            resource: PRODUCT_NOT_FOUND,
        })?;
        let product = from_product_row(&row)?;
        crate::audit(
            tx,
            context,
            "shop.product.updated",
            "ShopProduct",
            id.into_uuid(),
            json!({}),
        )
        .await?;
        Ok(product)
    }

    pub async fn delete_product(
        &self,
        tx: &mut SiteTx,
        context: &SiteContext,
        id: ProductId,
    ) -> Result<()> {
        let changed = sqlx::query(
            "update shop_products
                set deleted_at = clock_timestamp(), updated_at = clock_timestamp()
              where site_id = $1 and id = $2 and deleted_at is null",
        )
        .bind(context.site_id.into_uuid())
        .bind(id.into_uuid())
        .execute(tx.conn())
        .await
        .map_err(|_| MaviError::Internal)?;
        if changed.rows_affected() == 0 {
            return Err(MaviError::NotFound {
                resource: PRODUCT_NOT_FOUND,
            });
        }
        crate::audit(
            tx,
            context,
            "shop.product.deleted",
            "ShopProduct",
            id.into_uuid(),
            json!({}),
        )
        .await
    }
}

async fn list_product_rows(
    tx: &mut SiteTx,
    context: &SiteContext,
    page: &PageRequest,
    public: bool,
) -> Result<Page<Product>> {
    let after = page.after.as_ref().map(decode_cursor).transpose()?;
    let limit = i64::from(page.effective_limit());
    let mut query = sqlx::QueryBuilder::<sqlx::Postgres>::new("select ");
    query.push(PRODUCT_COLUMNS);
    query.push(" from shop_products where site_id = ");
    query.push_bind(context.site_id.into_uuid());
    query.push(" and deleted_at is null");
    if public {
        query.push(" and on_sale = true");
    }
    if let Some(after) = after {
        query
            .push(" and (created_at, id) < (")
            .push_bind(after.created_at)
            .push(", ")
            .push_bind(after.id)
            .push(")");
    }
    let rows = query
        .push(" order by created_at desc, id desc limit ")
        .push_bind(limit + 1)
        .build()
        .fetch_all(tx.conn())
        .await
        .map_err(|_| MaviError::Internal)?;
    let mut items = rows
        .iter()
        .map(from_product_row)
        .collect::<Result<Vec<_>>>()?;
    let limit = usize::try_from(limit).map_err(|_| MaviError::Internal)?;
    let next_cursor = if items.len() > limit {
        let last = items
            .get(limit.saturating_sub(1))
            .ok_or(MaviError::Internal)?;
        Some(encode_cursor(last.created_at, last.id.into_uuid())?)
    } else {
        None
    };
    items.truncate(limit);
    Ok(Page::new(items, next_cursor))
}

fn from_product_row(row: &sqlx::postgres::PgRow) -> Result<Product> {
    let currency = Currency::parse(
        &row.try_get::<String, _>("currency")
            .map_err(|_| MaviError::Internal)?,
    )?;
    Ok(Product {
        id: ProductId::from_uuid(row.try_get("id").map_err(|_| MaviError::Internal)?),
        slug: row.try_get("slug").map_err(|_| MaviError::Internal)?,
        name: row.try_get("name").map_err(|_| MaviError::Internal)?,
        description: row
            .try_get("description")
            .map_err(|_| MaviError::Internal)?,
        price: Money::new(
            row.try_get("price_minor")
                .map_err(|_| MaviError::Internal)?,
            currency,
        )?,
        stock: row
            .try_get("stock_on_hand")
            .map_err(|_| MaviError::Internal)?,
        on_sale: row.try_get("on_sale").map_err(|_| MaviError::Internal)?,
        created_at: row.try_get("created_at").map_err(|_| MaviError::Internal)?,
        updated_at: row.try_get("updated_at").map_err(|_| MaviError::Internal)?,
    })
}

pub(crate) async fn lock_product(
    tx: &mut SiteTx,
    context: &SiteContext,
    id: ProductId,
) -> Result<ProductSnapshot> {
    let row = sqlx::query(
        "select id, name, price_minor, currency, stock_on_hand, on_sale
           from shop_products
          where site_id = $1 and id = $2 and deleted_at is null
            for update",
    )
    .bind(context.site_id.into_uuid())
    .bind(id.into_uuid())
    .fetch_optional(tx.conn())
    .await
    .map_err(|_| MaviError::Internal)?
    .ok_or(MaviError::NotFound {
        resource: PRODUCT_NOT_FOUND,
    })?;
    let currency = Currency::parse(
        &row.try_get::<String, _>("currency")
            .map_err(|_| MaviError::Internal)?,
    )?;
    Ok(ProductSnapshot {
        id: ProductId::from_uuid(row.try_get("id").map_err(|_| MaviError::Internal)?),
        name: row.try_get("name").map_err(|_| MaviError::Internal)?,
        price: Money::new(
            row.try_get("price_minor")
                .map_err(|_| MaviError::Internal)?,
            currency,
        )?,
        stock: row
            .try_get("stock_on_hand")
            .map_err(|_| MaviError::Internal)?,
        on_sale: row.try_get("on_sale").map_err(|_| MaviError::Internal)?,
    })
}

fn validate_slug(value: &str) -> Result<String> {
    let value = value.trim();
    if value.is_empty()
        || value.chars().count() > MAX_SLUG_CHARS
        || value.starts_with('-')
        || value.ends_with('-')
        || !value.chars().all(|character| {
            character.is_ascii_lowercase() || character.is_ascii_digit() || character == '-'
        })
    {
        return Err(MaviError::validation_field(PRODUCT_SLUG_INVALID, "slug"));
    }
    Ok(value.to_owned())
}

fn validate_name(value: &str) -> Result<String> {
    let value = value.trim();
    if value.is_empty()
        || value.chars().count() > MAX_NAME_CHARS
        || value.chars().any(char::is_control)
    {
        return Err(MaviError::validation_field(PRODUCT_NAME_INVALID, "name"));
    }
    Ok(value.to_owned())
}

fn validate_description(value: &str) -> Result<String> {
    if value.chars().count() > MAX_DESCRIPTION_CHARS || value.contains('\0') {
        return Err(MaviError::validation_field(
            PRODUCT_DESCRIPTION_INVALID,
            "description",
        ));
    }
    Ok(value.to_owned())
}

fn validate_price(value: &ProductPrice) -> Result<Money> {
    let currency = Currency::parse(&value.currency)
        .map_err(|_| MaviError::validation_field(PRODUCT_PRICE_INVALID, "price.currency"))?;
    Money::new(value.minor, currency)
        .map_err(|_| MaviError::validation_field(PRODUCT_PRICE_INVALID, "price.minor"))
}

fn validate_minor(value: i64) -> Result<()> {
    if value < 0 {
        return Err(MaviError::validation_field(
            PRODUCT_PRICE_INVALID,
            "price_minor",
        ));
    }
    Ok(())
}

fn validate_stock(value: i32) -> Result<i32> {
    if !(0..=MAX_STOCK).contains(&value) {
        return Err(MaviError::validation_field(PRODUCT_STOCK_INVALID, "stock"));
    }
    Ok(value)
}

fn map_write_error(error: &sqlx::Error) -> MaviError {
    if let sqlx::Error::Database(database) = error
        && database.constraint() == Some("shop_products_site_slug_active")
    {
        return MaviError::conflict(PRODUCT_SLUG_TAKEN);
    }
    MaviError::Internal
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn product_contract_is_cursor_only_and_public_stock_is_not_exposed() {
        let contract = serde_json::to_string(&api()).expect("contract");
        assert!(contract.contains("PublicProductPage"));
        assert!(!contract.contains("offset"));
        assert!(!contract.contains("on_hand"));
    }

    #[test]
    fn product_values_are_bounded_and_currency_is_immutable_in_updates() {
        assert!(validate_slug("good-product").is_ok());
        assert!(validate_slug("Bad Product").is_err());
        assert!(validate_stock(-1).is_err());
        assert!(
            validate_price(&ProductPrice {
                minor: 100,
                currency: "TRY".to_owned(),
            })
            .is_ok()
        );
        assert!(
            validate_price(&ProductPrice {
                minor: 100,
                currency: "try".to_owned(),
            })
            .is_err()
        );
    }
}
