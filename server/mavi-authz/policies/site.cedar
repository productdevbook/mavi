permit (principal, action, resource)
when {
    principal.site_id == resource.site_id &&
    principal.site_id == context.site_id &&
    principal.plugins.contains(context.plugin) &&
    resource.plugins.contains(context.plugin) &&
    (context.resource_type == "*" || context.resource_type == resource.kind) &&
    (principal.permissions.contains(context.permission) ||
     resource.permissions.contains(context.permission))
};
