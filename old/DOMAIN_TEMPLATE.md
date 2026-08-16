# Domain template

Copy this into the pull request that introduces a domain, and fill it in before
writing the first line of it. It is one page on purpose: if a question here has
no answer yet, that is the thing to settle, not the code.

---

## `<domain>`

**What it is, in a sentence.**

**Who reaches it.** Which roles, and through which surface — panel, public
site, MCP, or an operator-only screen. A public surface is named as public here
or it does not get one.

**Tables.** Names and the shape of each. Which are tenant-scoped (nearly all of
them) and which are control-plane. State columns are Postgres enums, not text.

**States.** Every state machine in this domain, with its states and the
transitions that are legal. This is what the check constraint is written from.

**Events.** What it emits, with the name each event is published under. If it
emits nothing, say that and why.

**Trash.** Does anything here soft-delete? If so, what restoring one means when
the things it pointed at are gone.

**i18n.** Does it hold content in more than one language? Are its panel strings
extracted?

**Rate limits.** Any endpoint reachable without an account needs one named
here — forms, checkout, sign-in, anything a visitor can post to.

**Retention.** Anything holding a person's data says how long it is kept and
what removes it.

**What it deliberately does not do.**

---

## Cross-cutting checklist

A domain is not finished until every line is true or crossed out with a reason.

- [ ] Tenant-scoped tables have `tenant_id`, an RLS policy, and are only
      reached through `TenantTx`
- [ ] Every endpoint declares the permission it wants, and a test proves a role
      without it is refused
- [ ] Mutations write an audit row through `Auditable`
- [ ] Events are emitted through `EmitsEvents`, in the transaction that made
      the change
- [ ] Panel strings extracted and translated, English and Turkish
- [ ] Soft-delete wired if the domain needs it
- [ ] Rate limit on every unauthenticated endpoint
- [ ] Input validated at the API boundary, with the error shape everything else
      uses
- [ ] Every listing is paginated; nothing returns an unbounded set
- [ ] Migration written expand-then-contract, and reversible or explicitly not
- [ ] Foreign keys on every relationship, indexes on every one of them
- [ ] Check constraints for every state column
- [ ] Tests run against a real Postgres
- [ ] Tracing span with the domain and the operation on it
