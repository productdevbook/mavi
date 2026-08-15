// The operator runs the machine. Nothing here is a grant on a site's content;
// what an operator may reach is its own screens and the platform's own records.
permit (
    principal is Operator,
    action,
    resource
);

// A site's own people carry grants, and a grant is data the site edits. The
// policy asks only whether the grant needed is held.
permit (
    principal is SiteUser,
    action,
    resource is Site
)
when { principal in resource && principal.grants.contains(context.needed) };

// A grant that ends in `:own` reaches only what the person made. An author
// holding `content:write:own` edits their own posts and nobody else's; the
// domain says whose the record is, and a domain that does not say passes an
// owner nothing matches.
permit (
    principal is SiteUser,
    action,
    resource is Site
)
when {
    principal in resource &&
    context.owner == principal.id &&
    principal.grants.contains(context.needed_own)
};

// An agency reaches the sites it rents, through the grants it was given.
permit (
    principal is AgencyUser,
    action,
    resource is Site
)
when { principal.grants.contains(context.needed) };

// A customer sees what they are paying for, and never changes it.
permit (
    principal is Customer,
    action == Action::"view",
    resource is Charge
)
when { principal.site == resource.site };
