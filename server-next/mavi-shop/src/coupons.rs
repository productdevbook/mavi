use chrono::{DateTime, Utc};
use mavi_contract::{Endpoint, Method, Permission, Shape};
use mavi_core::{
    Action, Capability, CouponId, Currency, ErrorCode, MaviError, Money, Page, PageRequest, Result,
    SiteContext,
};
use mavi_storage::SiteTx;
use serde::{Deserialize, Serialize};
use serde_json::json;
use sqlx::Row;

use crate::{ShopService, decode_cursor, encode_cursor};

pub const COUPON_NOT_FOUND: &str = "shop_coupon_not_found";
pub const COUPON_CODE_INVALID: &str = "shop_coupon_code_invalid";
pub const COUPON_CODE_TAKEN: &str = "shop_coupon_code_taken";
pub const COUPON_RULE_INVALID: &str = "shop_coupon_rule_invalid";
pub const COUPON_EXPIRED: &str = "shop_coupon_expired";
pub const COUPON_EXHAUSTED: &str = "shop_coupon_exhausted";
pub const COUPON_CURRENCY_MISMATCH: &str = "shop_coupon_currency_mismatch";

const MAX_CODE_CHARS: usize = 40;
const MAX_USES: i64 = 1_000_000_000;

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum CouponKind {
    Percent,
    Amount,
}

impl CouponKind {
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Percent => "percent",
            Self::Amount => "amount",
        }
    }

    fn parse(value: &str) -> Result<Self> {
        match value {
            "percent" => Ok(Self::Percent),
            "amount" => Ok(Self::Amount),
            _ => Err(MaviError::Internal),
        }
    }
}

#[derive(Clone, Debug, Default, Deserialize, Eq, PartialEq, Serialize)]
pub struct CouponListFilter {
    #[serde(flatten)]
    pub page: PageRequest,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct CreateCoupon {
    pub code: String,
    pub percent: Option<u32>,
    pub amount_minor: Option<i64>,
    pub currency: Option<String>,
    pub max_uses: Option<i64>,
    pub expires_at: Option<DateTime<Utc>>,
}

#[derive(Clone, Debug, Serialize)]
pub struct Coupon {
    pub id: CouponId,
    pub code: String,
    pub kind: CouponKind,
    pub percent: Option<u32>,
    pub amount: Option<Money>,
    pub max_uses: Option<i64>,
    pub expires_at: Option<DateTime<Utc>>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

#[derive(Clone, Debug)]
pub(crate) struct CouponForCheckout {
    pub id: CouponId,
    pub coupon: Coupon,
    pub used: i64,
}

type ValidatedCouponInput = (String, CouponKind, Option<i32>, Option<Money>, Option<i64>);

pub fn api() -> mavi_contract::Api {
    mavi_contract::Api::new(endpoints()).with_shapes(shapes())
}

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
            "/api/v1/shop/coupons",
            "shop.coupons.list",
            "List site coupons with an opaque cursor",
        )
        .account_or_assistant()
        .requires(view)
        .takes_query("CouponListFilter")
        .returns(200, "CouponPage")
        .refuses([
            ErrorCode::Forbidden,
            ErrorCode::Validation,
            ErrorCode::Internal,
        ]),
        Endpoint::new(
            Method::Post,
            "/api/v1/shop/coupons",
            "shop.coupons.create",
            "Create a percentage or amount coupon",
        )
        .account_or_assistant()
        .requires(write)
        .takes("CreateCoupon")
        .returns(201, "Coupon")
        .changes(false)
        .refuses([
            ErrorCode::Forbidden,
            ErrorCode::Validation,
            ErrorCode::Conflict,
            ErrorCode::Internal,
        ]),
        Endpoint::new(
            Method::Delete,
            "/api/v1/shop/coupons/{id}",
            "shop.coupons.delete",
            "Remove a coupon from the active catalog",
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
    ]
}

fn shapes() -> Vec<Shape> {
    vec![
        Shape::new(
            "CouponKind",
            json!({"type": "string", "enum": ["percent", "amount"]}),
        ),
        Shape::new(
            "CouponListFilter",
            json!({"type": "object", "properties": {
                "after": {"type": ["string", "null"], "maxLength": 512},
                "limit": {"type": "integer", "minimum": 1, "maximum": 100}
            }}),
        ),
        Shape::new(
            "CreateCoupon",
            json!({
                "type": "object",
                "required": ["code"],
                "additionalProperties": false,
                "properties": {
                    "code": {"type": "string", "minLength": 3, "maxLength": MAX_CODE_CHARS},
                    "percent": {"type": ["integer", "null"], "minimum": 1, "maximum": 100},
                    "amount_minor": {"type": ["integer", "null"], "format": "int64", "minimum": 1},
                    "currency": {"type": ["string", "null"], "pattern": "^[A-Z]{3}$"},
                    "max_uses": {"type": ["integer", "null"], "minimum": 1, "maximum": MAX_USES},
                    "expires_at": {"type": ["string", "null"], "format": "date-time"}
                }
            }),
        ),
        Shape::new(
            "Coupon",
            json!({
                "type": "object",
                "required": ["id", "code", "kind", "percent", "amount", "max_uses", "expires_at", "created_at", "updated_at"],
                "properties": {
                    "id": {"type": "string", "format": "uuid"},
                    "code": {"type": "string"},
                    "kind": {"$ref": "#/components/schemas/CouponKind"},
                    "percent": {"type": ["integer", "null"]},
                    "amount": {"oneOf": [{"$ref": "#/components/schemas/Money"}, {"type": "null"}]},
                    "max_uses": {"type": ["integer", "null"]},
                    "expires_at": {"type": ["string", "null"], "format": "date-time"},
                    "created_at": {"type": "string", "format": "date-time"},
                    "updated_at": {"type": "string", "format": "date-time"}
                }
            }),
        ),
        Shape::new(
            "CouponPage",
            json!({"type": "object", "required": ["items", "next_cursor"], "properties": {
                "items": {"type": "array", "items": {"$ref": "#/components/schemas/Coupon"}},
                "next_cursor": {"type": ["string", "null"], "maxLength": 512}
            }}),
        ),
    ]
}

impl ShopService {
    pub async fn list_coupons(
        &self,
        tx: &mut SiteTx,
        context: &SiteContext,
        filter: &CouponListFilter,
    ) -> Result<Page<Coupon>> {
        let after = filter.page.after.as_ref().map(decode_cursor).transpose()?;
        let limit = i64::from(filter.page.effective_limit());
        let mut query = sqlx::QueryBuilder::<sqlx::Postgres>::new(
            "select id, code, kind, percent, amount_minor, currency, max_uses, expires_at,
                    created_at, updated_at
               from shop_coupons where site_id = ",
        );
        query.push_bind(context.site_id.into_uuid());
        query.push(" and deleted_at is null");
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
        let mut items = rows.iter().map(from_row).collect::<Result<Vec<_>>>()?;
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

    pub async fn create_coupon(
        &self,
        tx: &mut SiteTx,
        context: &SiteContext,
        input: &CreateCoupon,
    ) -> Result<Coupon> {
        let (code, kind, percent, amount, max_uses) = validate_input(input)?;
        let id = CouponId::new();
        let row = sqlx::query(
            "insert into shop_coupons
                (site_id, id, code, kind, percent, amount_minor, currency, max_uses, expires_at)
             values ($1, $2, $3, $4, $5, $6, $7, $8, $9)
             returning id, code, kind, percent, amount_minor, currency, max_uses, expires_at,
                       created_at, updated_at",
        )
        .bind(context.site_id.into_uuid())
        .bind(id.into_uuid())
        .bind(&code)
        .bind(kind.as_str())
        .bind(percent)
        .bind(amount.map(|money| money.minor))
        .bind(amount.map(|money| money.currency.to_string()))
        .bind(max_uses)
        .bind(input.expires_at)
        .fetch_one(tx.conn())
        .await
        .map_err(|error| map_write_error(&error))?;
        let coupon = from_row(&row)?;
        crate::audit(
            tx,
            context,
            "shop.coupon.created",
            "ShopCoupon",
            id.into_uuid(),
            json!({"code": coupon.code, "kind": coupon.kind}),
        )
        .await?;
        Ok(coupon)
    }

    pub async fn delete_coupon(
        &self,
        tx: &mut SiteTx,
        context: &SiteContext,
        id: CouponId,
    ) -> Result<()> {
        let changed = sqlx::query(
            "update shop_coupons
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
                resource: COUPON_NOT_FOUND,
            });
        }
        crate::audit(
            tx,
            context,
            "shop.coupon.deleted",
            "ShopCoupon",
            id.into_uuid(),
            json!({}),
        )
        .await
    }
}

pub(crate) async fn load_for_checkout(
    tx: &mut SiteTx,
    context: &SiteContext,
    code: &str,
) -> Result<CouponForCheckout> {
    let code = normalize_code(code)?;
    let row = sqlx::query(
        "select c.id, c.code, c.kind, c.percent, c.amount_minor, c.currency, c.max_uses,
                c.expires_at, c.created_at, c.updated_at,
                (select count(*) from shop_coupon_uses u
                  where u.site_id = c.site_id and u.coupon_id = c.id) as used
           from shop_coupons c
          where c.site_id = $1 and c.code = $2 and c.deleted_at is null
            for update",
    )
    .bind(context.site_id.into_uuid())
    .bind(&code)
    .fetch_optional(tx.conn())
    .await
    .map_err(|_| MaviError::Internal)?
    .ok_or(MaviError::NotFound {
        resource: COUPON_NOT_FOUND,
    })?;
    let used = row.try_get("used").map_err(|_| MaviError::Internal)?;
    Ok(CouponForCheckout {
        id: CouponId::from_uuid(row.try_get("id").map_err(|_| MaviError::Internal)?),
        coupon: from_row(&row)?,
        used,
    })
}

pub(crate) fn discount(
    total: Money,
    coupon: &CouponForCheckout,
    now: DateTime<Utc>,
) -> Result<Money> {
    if coupon
        .coupon
        .max_uses
        .is_some_and(|max_uses| coupon.used >= max_uses)
    {
        return Err(MaviError::conflict(COUPON_EXHAUSTED));
    }
    if coupon
        .coupon
        .expires_at
        .is_some_and(|expires_at| now >= expires_at)
    {
        return Err(MaviError::conflict(COUPON_EXPIRED));
    }
    let amount = match coupon.coupon.kind {
        CouponKind::Percent => {
            let percent = i128::from(coupon.coupon.percent.ok_or(MaviError::Internal)?);
            let minor = (i128::from(total.minor) * percent) / 100;
            i64::try_from(minor).map_err(|_| MaviError::validation("money_overflow"))?
        }
        CouponKind::Amount => {
            let amount = coupon.coupon.amount.ok_or(MaviError::Internal)?;
            if amount.currency != total.currency {
                return Err(MaviError::validation(COUPON_CURRENCY_MISMATCH));
            }
            amount.minor
        }
    };
    let reduction = Money::new(amount, total.currency)?;
    total.subtract_floor(reduction)
}

fn validate_input(input: &CreateCoupon) -> Result<ValidatedCouponInput> {
    let code = normalize_code(&input.code)?;
    let max_uses = input.max_uses.map(validate_max_uses).transpose()?;
    let rule = match (input.percent, input.amount_minor, input.currency.as_deref()) {
        (Some(percent), None, None) if (1..=100).contains(&percent) => Some((
            CouponKind::Percent,
            Some(i32::try_from(percent).map_err(|_| MaviError::Internal)?),
            None,
        )),
        (None, Some(minor), Some(currency)) => {
            let currency = Currency::parse(currency)
                .map_err(|_| MaviError::validation(COUPON_RULE_INVALID))?;
            let amount = Money::new(minor, currency)
                .map_err(|_| MaviError::validation(COUPON_RULE_INVALID))?;
            if amount.minor == 0 {
                return Err(MaviError::validation(COUPON_RULE_INVALID));
            }
            Some((CouponKind::Amount, None, Some(amount)))
        }
        _ => None,
    };
    let Some((kind, percent, amount)) = rule else {
        return Err(MaviError::validation(COUPON_RULE_INVALID));
    };
    Ok((code, kind, percent, amount, max_uses))
}

fn normalize_code(value: &str) -> Result<String> {
    let value = value.trim().to_ascii_uppercase();
    if !(3..=MAX_CODE_CHARS).contains(&value.chars().count())
        || !value.chars().all(|character| {
            character.is_ascii_uppercase() || character.is_ascii_digit() || character == '-'
        })
    {
        return Err(MaviError::validation_field(COUPON_CODE_INVALID, "code"));
    }
    Ok(value)
}

fn validate_max_uses(value: i64) -> Result<i64> {
    if !(1..=MAX_USES).contains(&value) {
        return Err(MaviError::validation_field(COUPON_RULE_INVALID, "max_uses"));
    }
    Ok(value)
}

fn from_row(row: &sqlx::postgres::PgRow) -> Result<Coupon> {
    let kind = CouponKind::parse(
        &row.try_get::<String, _>("kind")
            .map_err(|_| MaviError::Internal)?,
    )?;
    let amount_minor: Option<i64> = row
        .try_get("amount_minor")
        .map_err(|_| MaviError::Internal)?;
    let amount_currency: Option<String> =
        row.try_get("currency").map_err(|_| MaviError::Internal)?;
    let amount = match (amount_minor, amount_currency) {
        (Some(minor), Some(currency)) => Some(Money::new(minor, Currency::parse(&currency)?)?),
        (None, None) => None,
        _ => return Err(MaviError::Internal),
    };
    Ok(Coupon {
        id: CouponId::from_uuid(row.try_get("id").map_err(|_| MaviError::Internal)?),
        code: row.try_get("code").map_err(|_| MaviError::Internal)?,
        kind,
        percent: row
            .try_get::<Option<i32>, _>("percent")
            .map_err(|_| MaviError::Internal)?
            .map(|value| u32::try_from(value).map_err(|_| MaviError::Internal))
            .transpose()?,
        amount,
        max_uses: row.try_get("max_uses").map_err(|_| MaviError::Internal)?,
        expires_at: row.try_get("expires_at").map_err(|_| MaviError::Internal)?,
        created_at: row.try_get("created_at").map_err(|_| MaviError::Internal)?,
        updated_at: row.try_get("updated_at").map_err(|_| MaviError::Internal)?,
    })
}

fn map_write_error(error: &sqlx::Error) -> MaviError {
    if let sqlx::Error::Database(database) = error
        && database.constraint() == Some("shop_coupons_site_code_active")
    {
        return MaviError::conflict(COUPON_CODE_TAKEN);
    }
    MaviError::Internal
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn coupon_rules_are_explicit_and_cursor_only() {
        let contract = serde_json::to_string(&api()).expect("contract");
        assert!(contract.contains("CouponPage"));
        assert!(!contract.contains("offset"));
        assert!(normalize_code(" spring-26 ").is_ok());
        assert!(normalize_code("too short").is_err());
    }

    #[test]
    fn percentage_discount_rounds_down_and_amount_never_goes_negative() {
        let input = CreateCoupon {
            code: "THIRD".to_owned(),
            percent: Some(33),
            amount_minor: None,
            currency: None,
            max_uses: None,
            expires_at: None,
        };
        let (_, _, _, _, _) = validate_input(&input).expect("coupon");
        let currency = Currency::parse("TRY").expect("currency");
        let coupon = CouponForCheckout {
            id: CouponId::new(),
            coupon: Coupon {
                id: CouponId::new(),
                code: "THIRD".to_owned(),
                kind: CouponKind::Percent,
                percent: Some(33),
                amount: None,
                max_uses: None,
                expires_at: None,
                created_at: Utc::now(),
                updated_at: Utc::now(),
            },
            used: 0,
        };
        assert_eq!(
            discount(
                Money::new(1001, currency).expect("money"),
                &coupon,
                Utc::now()
            )
            .expect("discount")
            .minor,
            671
        );
    }
}
