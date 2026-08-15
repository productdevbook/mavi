# flows

What a site arranged to happen when something happens.

**Who reaches it.** The panel, with `flows:*`. Nothing here is public.

**Tables it owns.** `flows`, `flow_steps`, `flow_runs`, `flow_run_steps`,
`flow_credentials`.

**A trigger is an event this build emits.** The list is in one function, and a
flow waiting for something nothing sends is refused when it is made — because a
flow that never runs looks exactly like a flow that is broken.

**A run is started by the same transaction that wrote the event.** Emitting
queues the delivery of the event and the start of whatever was waiting for it,
both in the transaction that made the change, so neither happens for something
that rolled back.

**One step at a time, each its own piece of work.** A flow that waits an hour
does not hold a worker for an hour; a step that fails is retried on its own
rather than repeating everything before it; and a run that has failed stays
failed and stays readable — what went wrong on which step is the question a
person asks.

**A step that calls out does it through the outbox**, not from inside the
transaction. Waiting on somebody else's server with a transaction open is how a
database runs out of connections, and delivery already knows how to retry and
where it must not send.

**A credential is sealed and never handed back.** XChaCha20-Poly1305, with the
key's version travelling with the value so that rotating a key is adding one
rather than rewriting a table. Nothing in this API reads one out; the test
checks the stored value does not contain what was put in, and that no endpoint
returns it.

**What it deliberately does not do.**

- No branching, no conditions. Steps run in order. A flow that needs an "if" is
  two flows with different triggers, until something proves otherwise.
- No editing a flow's steps. Made once; changing one is making another.
- No calls to arbitrary third-party APIs beyond an event on the outbox. What
  the credentials are for is that day, and the sealing is in place first.
