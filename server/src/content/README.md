# content

Posts, pages, and what they are filed under.

**Who reaches it.** The panel, with `content:*` for posts and `taxonomy:*` for
categories and tags. Nothing here is public yet: the pages a visitor reads are
served by the edge from what publishing produced, not from this.

**Tables it owns.** `languages`, `content_types`, `posts`, `terms`,
`post_terms`, `redirects`.

**One table for a post and a page**, separated by a column, because everything
else about them is identical and two tables drift.

**One table for a category and a tag.** They differ in whether they nest and in
nothing else. Two tables is how the two came to disagree about which posts were
under what, and a post's relationship to either is one relationship.

**A custom field is `jsonb`**, indexed with GIN. What a site adds to its own
kind of thing is something it can ask a question about — "every recipe under
thirty minutes" is a query — rather than text nothing can read.

**A kind of thing is declared before it is written.** What it declares is what
may be written into a post's fields: an undeclared name is a typo that would
otherwise be stored and read as an empty page, a required field is required, a
choice is one of the choices, and fields with no kind of thing behind them are
refused because nothing could ask about them.

**A filter asks about a declared field, by name**, with the name checked
against the declaration before it reaches the query: `?type=recipe&field=
minutes&at_most=30`. A name nothing declared is refused rather than quietly
matching nothing, because a filter that silently returns everything is a screen
showing the wrong posts and saying nothing about it. One question at a time —
equal to, at most, or at least.

**Throwing a declaration away leaves what was written under it.** The type
stops existing and the posts keep their fields: tidying up a type is not a
reason to lose a hundred pages.

**States.** `draft → scheduled | published`, `scheduled → draft | published`,
`published → draft | archived`, `archived → draft`. The list is in one function
and the schema's own check is written from it. Publishing sets the moment it
happened; leaving published clears it.

**Scheduling is a state and a moment**, and a post cannot be scheduled without
one — the database says so as well as the handler, and a moment that has
already passed is refused. A job publishes what is due and emits
`post.published` for each, and the moment it was scheduled for is kept beside
the moment it actually went, so a post that went a day late because nothing was
running is something somebody can see rather than guess at.

**Ownership.** The route's guard answers "may this person do this kind of thing
at all", with the caller as the owner — at the point it runs there is no record
to ask about, and somebody holding `content:write:own` is entitled to write one.
A handler that then reaches a particular record asks again with that record's
author, which is where an author is told no about somebody else's.

**Events.** `post.published` and `post.unpublished`, carrying what the post is
and where, not its body.

**Addresses.** A slug that changes leaves a redirect behind, so what somebody
linked to keeps working. Two posts in one language cannot answer on one
address, and the database is what says so.

**Languages.** A post is written in one of the site's own languages and the
site says which those are. The panel's own language is English or Turkish and
is not this.

**Trash.** Posts soft-delete. Terms do not: what a term means is the posts
under it, and a term nobody can see is not a thing a site restores. Deleting
one leaves the posts.

**A body is stored as it was written, and nothing here sanitises it.** A site's
own author writing an embed into their own page is the feature, not the attack,
and stripping it would make the CMS unable to do what a CMS is for. What that
costs is written down here rather than left implicit: a body reaches the world
through the theme's own build on the site's own address, so a script in one runs
as that site — against that site's visitors, not this machine's. Two things keep
it there. Nothing this API serves renders a body as HTML: the answer is JSON and
the policy on it allows nothing at all. And an assistant's key can write a body,
so a site handing one out is handing out the ability to put a script on its own
pages; that is why the key lasts a day, is revocable, and cannot reach the
settings that hold anybody else's credentials.

**What it deliberately does not do.**

- No revisions. What a post was before its last change is in the audit log and
  not in a table of its own.
- No SEO lint (`page_issue`) and no import/export (`portable`). Both are named
  in #188 and neither is here.
