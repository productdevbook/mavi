# forms

A site's own forms, and what people send through them.

**Who reaches it.** The panel, with `forms:view`, `forms:write` and
`forms:delete`. One endpoint is public — the one a visitor posts to — and it is
the only thing in this domain reachable without an account.

**Tables it owns.** `forms` and `form_submissions`. A form's slug is unique,
so there is one `contact`.

**States.** A form is active or it is not; an inactive form is not there as far
as a visitor is concerned, which is a 404 rather than a refusal. Both tables
soft-delete, and a deleted form is not served, not listed, and takes nothing.

**Events.** `form.submitted`, carrying the form's id and the submission's — not
what was written. A receiver is told that somebody filled a form in and where to
read it; the answers stay on the site.

**Jobs.** `forms.sweep` takes away submissions older than the form said to keep
them, because that number is the form's to choose.

**Retention.** `retention_days` on the form, between one day and ten years,
365 by default. Registered in `retention`, which a test reads: a table
holding somebody's own words and no policy fails the build.

**Rate limit.** Twenty submissions a minute per address per form. A form is the
one thing on a site anybody can write to, and it is where a site gets buried.
An answer is bounded at ten thousand characters and a submission at a hundred
answers, before either reaches the database.

**i18n.** The form's own labels are the site's content and are stored as
written. Nothing in this domain returns a sentence the panel shows.

**What it deliberately does not do.**

- No CAPTCHA or bot wall. The rate limit is the whole defence for now; where
  one is wanted, it goes in front of the public endpoint and nowhere else.
- No file uploads. A form takes text; a form that takes a file is the media
  domain's problem and its limits are different.
- No notification mail on submission. That belongs to the mail domain and hangs
  off `form.submitted` rather than being wired in here.
- No editing a submission. What somebody sent is what they sent; the only
  things that happen to one are being read, marked seen, and going away.
