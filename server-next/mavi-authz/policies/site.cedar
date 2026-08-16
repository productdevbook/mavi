// The application supplies principal/resource entities and the request
// context. Role and API-key grants are data, not Rust branches.
permit (principal, action, resource)
when {
    principal.site_id == resource.site_id &&
    principal.site_id == context.site_id &&
    principal.grants.contains(context.grant)
};
