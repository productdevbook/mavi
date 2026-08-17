// Generated from the canonical Mavi API. Do not edit by hand.

export type JsonValue = null | boolean | number | string | JsonValue[] | { [key: string]: JsonValue };

export interface ApiKeyCreated {
  id: string;
  name: string;
  token: string;
  grants: Grant[];
  expires_at?: string | null;
}

export interface Content {
  id: string;
  site_id: string;
  kind: string;
  language: string;
  slug: string;
  title: string;
  excerpt: string | null;
  body: string;
  fields: Record<string, unknown>;
  publication: Publication;
  revision: number;
  created_at: string;
  updated_at: string;
}

export type ContentFieldKind = "text" | "long" | "email" | "number" | "choice" | "boolean";

export interface ContentListFilter {
  after?: string | null;
  limit?: number;
  kind?: string | null;
  language?: string | null;
  status?: PublicationStatus;
}

export interface ContentPage {
  items: Content[];
  next_cursor: string | null;
}

export interface ContentRevision {
  content_id: string;
  revision: number;
  kind: string;
  language: string;
  slug: string;
  title: string;
  excerpt: string | null;
  body: string;
  fields: Record<string, unknown>;
  publication: Publication;
  created_at: string;
}

export interface ContentRevisionListFilter {
  after?: string | null;
  limit?: number;
}

export interface ContentRevisionPage {
  items: ContentRevision[];
  next_cursor: string | null;
}

export interface ContentType {
  site_id: string;
  kind: string;
  name: string;
  fields: ContentTypeField[];
  created_at: string;
  updated_at: string;
}

export interface ContentTypeField {
  key: string;
  label: string;
  required: boolean;
  kind: ContentFieldKind;
  options: string[];
}

export interface ContentTypeListFilter {
  after?: string | null;
  limit?: number;
}

export interface ContentTypePage {
  items: ContentType[];
  next_cursor: string | null;
}

export interface CreateApiKey {
  name: string;
  grants: Grant[];
  expires_at?: string | null;
}

export interface CreateContent {
  kind: string;
  language: string;
  slug: string;
  title: string;
  excerpt?: string | null;
  body?: string;
  fields?: Record<string, unknown>;
  publication?: PublicationInput;
}

export interface CreateLanguage {
  tag: string;
  name: string;
  is_default?: boolean;
}

export interface CreatePerson {
  email: string;
  name: string;
  password: string;
  role_ids?: string[];
}

export interface CreateRole {
  name: string;
  grants?: Grant[];
}

export interface DeclareContentType {
  name: string;
  fields?: ContentTypeField[];
}

export type Empty = Record<string, unknown>;

export interface ErrorBody {
  code: string;
  message: string;
  field?: string | null;
}

export interface ErrorEnvelope {
  error: ErrorBody;
}

export interface Grant {
  capability: string;
  action: string;
}

export interface Language {
  site_id: string;
  tag: string;
  name: string;
  is_default: boolean;
  created_at: string;
  updated_at: string;
}

export interface LanguageListFilter {
  after?: string | null;
  limit?: number;
}

export interface LanguagePage {
  items: Language[];
  next_cursor: string | null;
}

export interface LoginInput {
  email: string;
  password: string;
}

export interface PeopleListFilter {
  after?: string | null;
  limit?: number;
  status?: PersonListFilterStatus;
}

export interface Person {
  id: string;
  site_id: string;
  email: string;
  name: string;
}

export type PersonListFilterStatus = "active" | "suspended" | "removed";

export interface PersonPage {
  items: PersonRecord[];
  next_cursor: string | null;
}

export interface PersonRecord {
  id: string;
  site_id: string;
  email: string;
  name: string;
  status: PersonListFilterStatus;
  role_ids: string[];
  created_at: string;
  updated_at: string;
}

export type Publication = "draft" | "archived" | Record<string, unknown> | Record<string, unknown>;

export type PublicationInput = "draft" | "publish" | "archive" | Record<string, unknown>;

export type PublicationStatus = "draft" | "scheduled" | "published" | "archived";

export interface ReplaceRoleGrants {
  grants: Grant[];
}

export interface Role {
  id: string;
  site_id: string;
  name: string;
  grants: Grant[];
  created_at: string;
}

export interface RoleListFilter {
  after?: string | null;
  limit?: number;
}

export interface RolePage {
  items: Role[];
  next_cursor: string | null;
}

export interface ScheduleContent {
  at: string;
}

export interface SessionCreated {
  id: string;
  token: string;
  expires_at: string;
}

export interface SetupInput {
  site_name: string;
  email: string;
  name: string;
  password: string;
}

export interface SetupStatus {
  initialized: boolean;
}

export interface SiteSettings {
  site_id: string;
  name: string;
  timezone: string;
  updated_at: string;
}

export interface UpdateContent {
  slug?: string | null;
  title?: string | null;
  excerpt?: string | null;
  body?: string | null;
  fields?: Record<string, unknown> | null;
  publication?: PublicationInput;
}

export interface UpdateLanguage {
  name?: string | null;
  is_default?: boolean | null;
}

export interface UpdatePersonStatus {
  status: PersonListFilterStatus;
}

export interface UpdateSiteSettings {
  name?: string | null;
  timezone?: string | null;
}

export interface MaviOperation {
  method: "get" | "post" | "put" | "patch" | "delete";
  path: string;
  input: { location: "json" | "query"; shape: string } | null;
  output: string | null;
  status: number;
  authentication: string;
  permission: { capability: string; action: string } | null;
}

export const operations = {
  "setup.status": { method: "get", path: "/api/v1/setup", input: null, output: "SetupStatus", status: 200, authentication: "public", permission: null },
  "setup.initialize": { method: "post", path: "/api/v1/setup", input: { location: "json", shape: "SetupInput" }, output: "Person", status: 201, authentication: "public", permission: null },
  "auth.session.create": { method: "post", path: "/api/v1/auth/sessions", input: { location: "json", shape: "LoginInput" }, output: "SessionCreated", status: 201, authentication: "public", permission: null },
  "auth.session.revoke": { method: "delete", path: "/api/v1/auth/sessions/current", input: null, output: "Empty", status: 204, authentication: "account", permission: null },
  "auth.api_key.create": { method: "post", path: "/api/v1/auth/api-keys", input: { location: "json", shape: "CreateApiKey" }, output: "ApiKeyCreated", status: 201, authentication: "account", permission: { capability: "people", action: "write" } },
  "auth.api_key.revoke": { method: "delete", path: "/api/v1/auth/api-keys/{id}", input: null, output: "Empty", status: 204, authentication: "account_or_assistant", permission: { capability: "people", action: "delete" } },
  "people.list": { method: "get", path: "/api/v1/people", input: { location: "query", shape: "PeopleListFilter" }, output: "PersonPage", status: 200, authentication: "account_or_assistant", permission: { capability: "people", action: "view" } },
  "people.create": { method: "post", path: "/api/v1/people", input: { location: "json", shape: "CreatePerson" }, output: "PersonRecord", status: 201, authentication: "account_or_assistant", permission: { capability: "people", action: "write" } },
  "people.status.update": { method: "patch", path: "/api/v1/people/{id}/status", input: { location: "json", shape: "UpdatePersonStatus" }, output: "PersonRecord", status: 200, authentication: "account_or_assistant", permission: { capability: "people", action: "write" } },
  "roles.list": { method: "get", path: "/api/v1/roles", input: { location: "query", shape: "RoleListFilter" }, output: "RolePage", status: 200, authentication: "account_or_assistant", permission: { capability: "people", action: "view" } },
  "roles.create": { method: "post", path: "/api/v1/roles", input: { location: "json", shape: "CreateRole" }, output: "Role", status: 201, authentication: "account_or_assistant", permission: { capability: "people", action: "write" } },
  "roles.grants.replace": { method: "put", path: "/api/v1/roles/{id}/grants", input: { location: "json", shape: "ReplaceRoleGrants" }, output: "Role", status: 200, authentication: "account_or_assistant", permission: { capability: "people", action: "write" } },
  "content.list": { method: "get", path: "/api/v1/content", input: { location: "query", shape: "ContentListFilter" }, output: "ContentPage", status: 200, authentication: "account_or_assistant", permission: { capability: "content", action: "view" } },
  "content.read": { method: "get", path: "/api/v1/content/{id}", input: null, output: "Content", status: 200, authentication: "account_or_assistant", permission: { capability: "content", action: "view" } },
  "content.create": { method: "post", path: "/api/v1/content", input: { location: "json", shape: "CreateContent" }, output: "Content", status: 201, authentication: "account_or_assistant", permission: { capability: "content", action: "write" } },
  "content.update": { method: "patch", path: "/api/v1/content/{id}", input: { location: "json", shape: "UpdateContent" }, output: "Content", status: 200, authentication: "account_or_assistant", permission: { capability: "content", action: "write" } },
  "content.publish": { method: "post", path: "/api/v1/content/{id}/publish", input: null, output: "Content", status: 200, authentication: "account_or_assistant", permission: { capability: "publish", action: "write" } },
  "content.schedule": { method: "post", path: "/api/v1/content/{id}/schedule", input: { location: "json", shape: "ScheduleContent" }, output: "Content", status: 200, authentication: "account_or_assistant", permission: { capability: "publish", action: "write" } },
  "content.archive": { method: "post", path: "/api/v1/content/{id}/archive", input: null, output: "Content", status: 200, authentication: "account_or_assistant", permission: { capability: "publish", action: "write" } },
  "content.trash": { method: "delete", path: "/api/v1/content/{id}", input: null, output: "Empty", status: 204, authentication: "account_or_assistant", permission: { capability: "trash", action: "delete" } },
  "content.restore": { method: "post", path: "/api/v1/content/{id}/restore", input: null, output: "Content", status: 200, authentication: "account_or_assistant", permission: { capability: "trash", action: "write" } },
  "content.public_read": { method: "get", path: "/public/v1/content/{slug}", input: null, output: "Content", status: 200, authentication: "public", permission: null },
  "content_types.list": { method: "get", path: "/api/v1/content-types", input: { location: "query", shape: "ContentTypeListFilter" }, output: "ContentTypePage", status: 200, authentication: "account_or_assistant", permission: { capability: "content", action: "view" } },
  "content_types.upsert": { method: "put", path: "/api/v1/content-types/{kind}", input: { location: "json", shape: "DeclareContentType" }, output: "ContentType", status: 200, authentication: "account_or_assistant", permission: { capability: "content", action: "write" } },
  "content_types.delete": { method: "delete", path: "/api/v1/content-types/{kind}", input: null, output: "Empty", status: 204, authentication: "account_or_assistant", permission: { capability: "content", action: "delete" } },
  "content.revisions.list": { method: "get", path: "/api/v1/content/{id}/revisions", input: { location: "query", shape: "ContentRevisionListFilter" }, output: "ContentRevisionPage", status: 200, authentication: "account_or_assistant", permission: { capability: "content", action: "view" } },
  "content.revisions.read": { method: "get", path: "/api/v1/content/{id}/revisions/{revision}", input: null, output: "ContentRevision", status: 200, authentication: "account_or_assistant", permission: { capability: "content", action: "view" } },
  "settings.read": { method: "get", path: "/api/v1/settings", input: null, output: "SiteSettings", status: 200, authentication: "account_or_assistant", permission: { capability: "settings", action: "view" } },
  "settings.update": { method: "patch", path: "/api/v1/settings", input: { location: "json", shape: "UpdateSiteSettings" }, output: "SiteSettings", status: 200, authentication: "account_or_assistant", permission: { capability: "settings", action: "write" } },
  "languages.list": { method: "get", path: "/api/v1/languages", input: { location: "query", shape: "LanguageListFilter" }, output: "LanguagePage", status: 200, authentication: "account_or_assistant", permission: { capability: "settings", action: "view" } },
  "languages.create": { method: "post", path: "/api/v1/languages", input: { location: "json", shape: "CreateLanguage" }, output: "Language", status: 201, authentication: "account_or_assistant", permission: { capability: "settings", action: "write" } },
  "languages.update": { method: "patch", path: "/api/v1/languages/{tag}", input: { location: "json", shape: "UpdateLanguage" }, output: "Language", status: 200, authentication: "account_or_assistant", permission: { capability: "settings", action: "write" } },
  "languages.delete": { method: "delete", path: "/api/v1/languages/{tag}", input: null, output: "Empty", status: 204, authentication: "account_or_assistant", permission: { capability: "settings", action: "delete" } },
} as const satisfies Record<string, MaviOperation>;

export type OperationName = keyof typeof operations;

export interface OperationArguments {
  "setup.status": { path?: never; query?: never; body?: never; }
  "setup.initialize": { path?: never; query?: never; body: SetupInput; }
  "auth.session.create": { path?: never; query?: never; body: LoginInput; }
  "auth.session.revoke": { path?: never; query?: never; body?: never; }
  "auth.api_key.create": { path?: never; query?: never; body: CreateApiKey; }
  "auth.api_key.revoke": { path: { id: string }; query?: never; body?: never; }
  "people.list": { path?: never; query: PeopleListFilter; body?: never; }
  "people.create": { path?: never; query?: never; body: CreatePerson; }
  "people.status.update": { path: { id: string }; query?: never; body: UpdatePersonStatus; }
  "roles.list": { path?: never; query: RoleListFilter; body?: never; }
  "roles.create": { path?: never; query?: never; body: CreateRole; }
  "roles.grants.replace": { path: { id: string }; query?: never; body: ReplaceRoleGrants; }
  "content.list": { path?: never; query: ContentListFilter; body?: never; }
  "content.read": { path: { id: string }; query?: never; body?: never; }
  "content.create": { path?: never; query?: never; body: CreateContent; }
  "content.update": { path: { id: string }; query?: never; body: UpdateContent; }
  "content.publish": { path: { id: string }; query?: never; body?: never; }
  "content.schedule": { path: { id: string }; query?: never; body: ScheduleContent; }
  "content.archive": { path: { id: string }; query?: never; body?: never; }
  "content.trash": { path: { id: string }; query?: never; body?: never; }
  "content.restore": { path: { id: string }; query?: never; body?: never; }
  "content.public_read": { path: { slug: string }; query?: never; body?: never; }
  "content_types.list": { path?: never; query: ContentTypeListFilter; body?: never; }
  "content_types.upsert": { path: { kind: string }; query?: never; body: DeclareContentType; }
  "content_types.delete": { path: { kind: string }; query?: never; body?: never; }
  "content.revisions.list": { path: { id: string }; query: ContentRevisionListFilter; body?: never; }
  "content.revisions.read": { path: { id: string; revision: string }; query?: never; body?: never; }
  "settings.read": { path?: never; query?: never; body?: never; }
  "settings.update": { path?: never; query?: never; body: UpdateSiteSettings; }
  "languages.list": { path?: never; query: LanguageListFilter; body?: never; }
  "languages.create": { path?: never; query?: never; body: CreateLanguage; }
  "languages.update": { path: { tag: string }; query?: never; body: UpdateLanguage; }
  "languages.delete": { path: { tag: string }; query?: never; body?: never; }
}

export interface OperationResponses {
  "setup.status": SetupStatus;
  "setup.initialize": Person;
  "auth.session.create": SessionCreated;
  "auth.session.revoke": void;
  "auth.api_key.create": ApiKeyCreated;
  "auth.api_key.revoke": void;
  "people.list": PersonPage;
  "people.create": PersonRecord;
  "people.status.update": PersonRecord;
  "roles.list": RolePage;
  "roles.create": Role;
  "roles.grants.replace": Role;
  "content.list": ContentPage;
  "content.read": Content;
  "content.create": Content;
  "content.update": Content;
  "content.publish": Content;
  "content.schedule": Content;
  "content.archive": Content;
  "content.trash": void;
  "content.restore": Content;
  "content.public_read": Content;
  "content_types.list": ContentTypePage;
  "content_types.upsert": ContentType;
  "content_types.delete": void;
  "content.revisions.list": ContentRevisionPage;
  "content.revisions.read": ContentRevision;
  "settings.read": SiteSettings;
  "settings.update": SiteSettings;
  "languages.list": LanguagePage;
  "languages.create": Language;
  "languages.update": Language;
  "languages.delete": void;
}

export interface MaviClientOptions {
  baseUrl: string;
  token?: string;
  fetch?: typeof globalThis.fetch;
}

export class MaviApiError extends Error {
  constructor(
    public readonly status: number,
    public readonly payload: ErrorEnvelope | null,
  ) {
    super(payload?.error.message ?? `Mavi request failed with status ${status}`);
  }
}

export class MaviClient {
  private readonly baseUrl: string;
  private readonly fetcher: typeof globalThis.fetch;

  constructor(private readonly options: MaviClientOptions) {
    this.baseUrl = options.baseUrl.replace(/\/+$/, "");
    this.fetcher = options.fetch ?? globalThis.fetch;
  }

  async call<Name extends OperationName>(
    operation: Name,
    args: OperationArguments[Name],
  ): Promise<OperationResponses[Name]> {
    const definition = operations[operation];
    const values = args as {
      path?: Record<string, string>;
      query?: Record<string, unknown>;
      body?: unknown;
    };
    let path: string = definition.path;
    for (const [name, value] of Object.entries(values.path ?? {})) {
      path = path.replace(`{${name}}`, encodeURIComponent(value));
    }
    const url = new URL(`${this.baseUrl}${path}`);
    for (const [name, value] of Object.entries(values.query ?? {})) {
      if (value !== undefined && value !== null) {
        url.searchParams.set(name, String(value));
      }
    }
    const headers: Record<string, string> = { Accept: "application/json" };
    if (this.options.token) {
      headers.Authorization = `Bearer ${this.options.token}`;
    }
    if (values.body !== undefined) {
      headers["Content-Type"] = "application/json";
    }
    const response = await this.fetcher(url, {
      method: definition.method.toUpperCase(),
      headers,
      body: values.body === undefined ? undefined : JSON.stringify(values.body),
    });
    if (!response.ok) {
      let payload: ErrorEnvelope | null = null;
      try {
        payload = (await response.json()) as ErrorEnvelope;
      } catch {
        payload = null;
      }
      throw new MaviApiError(response.status, payload);
    }
    if (response.status === 204) {
      return undefined as OperationResponses[Name];
    }
    return (await response.json()) as OperationResponses[Name];
  }
}
