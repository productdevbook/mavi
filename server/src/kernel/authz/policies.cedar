// The site's own people carry grants, and a grant is data the site edits. The
// policy asks only whether the grant needed is held.
//
// It used to ask `principal in resource` as well — whether this person belongs
// to the site being reached — and that clause was the tenancy. There is one
// site and everybody signed in is on it, so what is left is the grant, which
// is the part that was ever doing the work.
permit (
    principal is SiteUser,
    action,
    resource is Site
)
when { principal.grants.contains(context.needed) };

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
    context.owner == principal.id &&
    principal.grants.contains(context.needed_own)
};
