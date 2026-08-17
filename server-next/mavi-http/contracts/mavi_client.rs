// Generated from the canonical Mavi API. Do not edit by hand.

use serde::{Deserialize, Serialize};
use serde_json::Value;

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct ApiKeyCreated {
    pub id: String,
    pub name: String,
    pub token: String,
    pub grants: Vec<Grant>,
    pub expires_at: Option<String>,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct Content {
    pub id: String,
    pub site_id: String,
    pub kind: String,
    pub language: String,
    pub slug: String,
    pub title: String,
    pub excerpt: Option<String>,
    pub body: String,
    pub fields: Value,
    pub publication: Publication,
    pub revision: i64,
    pub created_at: String,
    pub updated_at: String,
}

pub type ContentFieldKind = String;

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct ContentListFilter {
    pub after: Option<String>,
    pub limit: Option<i64>,
    pub kind: Option<String>,
    pub language: Option<String>,
    pub status: Option<PublicationStatus>,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct ContentPage {
    pub items: Vec<Content>,
    pub next_cursor: Option<String>,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct ContentType {
    pub site_id: String,
    pub kind: String,
    pub name: String,
    pub fields: Vec<ContentTypeField>,
    pub created_at: String,
    pub updated_at: String,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct ContentTypeField {
    pub key: String,
    pub label: String,
    pub required: bool,
    pub kind: ContentFieldKind,
    pub options: Vec<String>,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct ContentTypeListFilter {
    pub after: Option<String>,
    pub limit: Option<i64>,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct ContentTypePage {
    pub items: Vec<ContentType>,
    pub next_cursor: Option<String>,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct CreateApiKey {
    pub name: String,
    pub grants: Vec<Grant>,
    pub expires_at: Option<String>,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct CreateContent {
    pub kind: String,
    pub language: String,
    pub slug: String,
    pub title: String,
    pub excerpt: Option<String>,
    pub body: Option<String>,
    pub fields: Option<Value>,
    pub publication: Option<PublicationInput>,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct CreateLanguage {
    pub tag: String,
    pub name: String,
    pub is_default: Option<bool>,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct CreatePerson {
    pub email: String,
    pub name: String,
    pub password: String,
    pub role_ids: Option<Vec<String>>,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct CreateRole {
    pub name: String,
    pub grants: Option<Vec<Grant>>,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct DeclareContentType {
    pub name: String,
    pub fields: Option<Vec<ContentTypeField>>,
}

pub type Empty = Value;

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct ErrorBody {
    pub code: String,
    pub message: String,
    pub field: Option<String>,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct ErrorEnvelope {
    pub error: ErrorBody,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct Grant {
    pub capability: String,
    pub action: String,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct Language {
    pub site_id: String,
    pub tag: String,
    pub name: String,
    pub is_default: bool,
    pub created_at: String,
    pub updated_at: String,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct LanguageListFilter {
    pub after: Option<String>,
    pub limit: Option<i64>,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct LanguagePage {
    pub items: Vec<Language>,
    pub next_cursor: Option<String>,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct LoginInput {
    pub email: String,
    pub password: String,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct PeopleListFilter {
    pub after: Option<String>,
    pub limit: Option<i64>,
    pub status: Option<PersonListFilterStatus>,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct Person {
    pub id: String,
    pub site_id: String,
    pub email: String,
    pub name: String,
}

pub type PersonListFilterStatus = String;

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct PersonPage {
    pub items: Vec<PersonRecord>,
    pub next_cursor: Option<String>,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct PersonRecord {
    pub id: String,
    pub site_id: String,
    pub email: String,
    pub name: String,
    pub status: PersonListFilterStatus,
    pub role_ids: Vec<String>,
    pub created_at: String,
    pub updated_at: String,
}

pub type Publication = Value;

pub type PublicationInput = Value;

pub type PublicationStatus = String;

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct ReplaceRoleGrants {
    pub grants: Vec<Grant>,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct Role {
    pub id: String,
    pub site_id: String,
    pub name: String,
    pub grants: Vec<Grant>,
    pub created_at: String,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct RoleListFilter {
    pub after: Option<String>,
    pub limit: Option<i64>,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct RolePage {
    pub items: Vec<Role>,
    pub next_cursor: Option<String>,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct ScheduleContent {
    pub at: String,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct SessionCreated {
    pub id: String,
    pub token: String,
    pub expires_at: String,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct SetupInput {
    pub site_name: String,
    pub email: String,
    pub name: String,
    pub password: String,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct SetupStatus {
    pub initialized: bool,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct SiteSettings {
    pub site_id: String,
    pub name: String,
    pub timezone: String,
    pub updated_at: String,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct UpdateContent {
    pub slug: Option<String>,
    pub title: Option<String>,
    pub excerpt: Option<String>,
    pub body: Option<String>,
    pub fields: Option<Value>,
    pub publication: Option<PublicationInput>,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct UpdateLanguage {
    pub name: Option<String>,
    pub is_default: Option<bool>,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct UpdatePersonStatus {
    pub status: PersonListFilterStatus,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct UpdateSiteSettings {
    pub name: Option<String>,
    pub timezone: Option<String>,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct OperationDefinition {
    pub name: &'static str,
    pub method: &'static str,
    pub path: &'static str,
    pub request: Option<&'static str>,
    pub response: Option<&'static str>,
    pub status: u16,
    pub authentication: &'static str,
    pub capability: Option<&'static str>,
    pub action: Option<&'static str>,
}

pub const OPERATIONS: &[OperationDefinition] = &[
    OperationDefinition { name: "setup.status", method: "get", path: "/api/v1/setup", request: None, response: Some("SetupStatus"), status: 200, authentication: "public", capability: None, action: None },
    OperationDefinition { name: "setup.initialize", method: "post", path: "/api/v1/setup", request: Some("SetupInput"), response: Some("Person"), status: 201, authentication: "public", capability: None, action: None },
    OperationDefinition { name: "auth.session.create", method: "post", path: "/api/v1/auth/sessions", request: Some("LoginInput"), response: Some("SessionCreated"), status: 201, authentication: "public", capability: None, action: None },
    OperationDefinition { name: "auth.session.revoke", method: "delete", path: "/api/v1/auth/sessions/current", request: None, response: Some("Empty"), status: 204, authentication: "account", capability: None, action: None },
    OperationDefinition { name: "auth.api_key.create", method: "post", path: "/api/v1/auth/api-keys", request: Some("CreateApiKey"), response: Some("ApiKeyCreated"), status: 201, authentication: "account", capability: Some("people"), action: Some("write") },
    OperationDefinition { name: "auth.api_key.revoke", method: "delete", path: "/api/v1/auth/api-keys/{id}", request: None, response: Some("Empty"), status: 204, authentication: "account_or_assistant", capability: Some("people"), action: Some("delete") },
    OperationDefinition { name: "people.list", method: "get", path: "/api/v1/people", request: Some("PeopleListFilter"), response: Some("PersonPage"), status: 200, authentication: "account_or_assistant", capability: Some("people"), action: Some("view") },
    OperationDefinition { name: "people.create", method: "post", path: "/api/v1/people", request: Some("CreatePerson"), response: Some("PersonRecord"), status: 201, authentication: "account_or_assistant", capability: Some("people"), action: Some("write") },
    OperationDefinition { name: "people.status.update", method: "patch", path: "/api/v1/people/{id}/status", request: Some("UpdatePersonStatus"), response: Some("PersonRecord"), status: 200, authentication: "account_or_assistant", capability: Some("people"), action: Some("write") },
    OperationDefinition { name: "roles.list", method: "get", path: "/api/v1/roles", request: Some("RoleListFilter"), response: Some("RolePage"), status: 200, authentication: "account_or_assistant", capability: Some("people"), action: Some("view") },
    OperationDefinition { name: "roles.create", method: "post", path: "/api/v1/roles", request: Some("CreateRole"), response: Some("Role"), status: 201, authentication: "account_or_assistant", capability: Some("people"), action: Some("write") },
    OperationDefinition { name: "roles.grants.replace", method: "put", path: "/api/v1/roles/{id}/grants", request: Some("ReplaceRoleGrants"), response: Some("Role"), status: 200, authentication: "account_or_assistant", capability: Some("people"), action: Some("write") },
    OperationDefinition { name: "content.list", method: "get", path: "/api/v1/content", request: Some("ContentListFilter"), response: Some("ContentPage"), status: 200, authentication: "account_or_assistant", capability: Some("content"), action: Some("view") },
    OperationDefinition { name: "content.read", method: "get", path: "/api/v1/content/{id}", request: None, response: Some("Content"), status: 200, authentication: "account_or_assistant", capability: Some("content"), action: Some("view") },
    OperationDefinition { name: "content.create", method: "post", path: "/api/v1/content", request: Some("CreateContent"), response: Some("Content"), status: 201, authentication: "account_or_assistant", capability: Some("content"), action: Some("write") },
    OperationDefinition { name: "content.update", method: "patch", path: "/api/v1/content/{id}", request: Some("UpdateContent"), response: Some("Content"), status: 200, authentication: "account_or_assistant", capability: Some("content"), action: Some("write") },
    OperationDefinition { name: "content.publish", method: "post", path: "/api/v1/content/{id}/publish", request: None, response: Some("Content"), status: 200, authentication: "account_or_assistant", capability: Some("publish"), action: Some("write") },
    OperationDefinition { name: "content.schedule", method: "post", path: "/api/v1/content/{id}/schedule", request: Some("ScheduleContent"), response: Some("Content"), status: 200, authentication: "account_or_assistant", capability: Some("publish"), action: Some("write") },
    OperationDefinition { name: "content.archive", method: "post", path: "/api/v1/content/{id}/archive", request: None, response: Some("Content"), status: 200, authentication: "account_or_assistant", capability: Some("publish"), action: Some("write") },
    OperationDefinition { name: "content.trash", method: "delete", path: "/api/v1/content/{id}", request: None, response: Some("Empty"), status: 204, authentication: "account_or_assistant", capability: Some("trash"), action: Some("delete") },
    OperationDefinition { name: "content.restore", method: "post", path: "/api/v1/content/{id}/restore", request: None, response: Some("Content"), status: 200, authentication: "account_or_assistant", capability: Some("trash"), action: Some("write") },
    OperationDefinition { name: "content.public_read", method: "get", path: "/public/v1/content/{slug}", request: None, response: Some("Content"), status: 200, authentication: "public", capability: None, action: None },
    OperationDefinition { name: "content_types.list", method: "get", path: "/api/v1/content-types", request: Some("ContentTypeListFilter"), response: Some("ContentTypePage"), status: 200, authentication: "account_or_assistant", capability: Some("content"), action: Some("view") },
    OperationDefinition { name: "content_types.upsert", method: "put", path: "/api/v1/content-types/{kind}", request: Some("DeclareContentType"), response: Some("ContentType"), status: 200, authentication: "account_or_assistant", capability: Some("content"), action: Some("write") },
    OperationDefinition { name: "content_types.delete", method: "delete", path: "/api/v1/content-types/{kind}", request: None, response: Some("Empty"), status: 204, authentication: "account_or_assistant", capability: Some("content"), action: Some("delete") },
    OperationDefinition { name: "settings.read", method: "get", path: "/api/v1/settings", request: None, response: Some("SiteSettings"), status: 200, authentication: "account_or_assistant", capability: Some("settings"), action: Some("view") },
    OperationDefinition { name: "settings.update", method: "patch", path: "/api/v1/settings", request: Some("UpdateSiteSettings"), response: Some("SiteSettings"), status: 200, authentication: "account_or_assistant", capability: Some("settings"), action: Some("write") },
    OperationDefinition { name: "languages.list", method: "get", path: "/api/v1/languages", request: Some("LanguageListFilter"), response: Some("LanguagePage"), status: 200, authentication: "account_or_assistant", capability: Some("settings"), action: Some("view") },
    OperationDefinition { name: "languages.create", method: "post", path: "/api/v1/languages", request: Some("CreateLanguage"), response: Some("Language"), status: 201, authentication: "account_or_assistant", capability: Some("settings"), action: Some("write") },
    OperationDefinition { name: "languages.update", method: "patch", path: "/api/v1/languages/{tag}", request: Some("UpdateLanguage"), response: Some("Language"), status: 200, authentication: "account_or_assistant", capability: Some("settings"), action: Some("write") },
    OperationDefinition { name: "languages.delete", method: "delete", path: "/api/v1/languages/{tag}", request: None, response: Some("Empty"), status: 204, authentication: "account_or_assistant", capability: Some("settings"), action: Some("delete") },
];
