# What an assistant can do

An assistant is a caller. It signs in the way every other caller does, it holds
what its account holds, and what it may do is what that account may do.

That sentence is the whole design, and everything below is a consequence of it.

## There is no list of tools

A tool **is** an endpoint. `/api/assistant` speaks MCP over JSON-RPC, and what
it offers is the API this installation already publishes, named the way the
protocol wants: `writings.throw-away` becomes `writings_throw_away`.

Nothing is written twice. There is no second query, no second grant, and no
second idea of who may do what — so "forbidden in the panel, allowed over the
assistant's door" is impossible rather than unlikely.

The software this replaces did it the other way: seven hundred lines of
hand-written tools, each with its own SQL and its own declared capability, each
a copy of an endpoint that already existed. Every copy is a place for two
things to drift apart.

## One way in

    admit  →  the handler  →  the rule that a change leaves a record

A request has always gone through those three, in that order. There is now one
function that *is* them — `Door::call` — and both a request and an assistant
call it. There is nothing else to call.

So:

- **What a tool is refused, it is refused in the same words.** The same `key`,
  the same named arguments, the same sentence — because it is the same refusal
  value, not a second rendering of it.
- **What a tool changes leaves the endpoint's own receipt.** `writings.write`,
  never `assistant.talk`. What happened to a writing has one answer, however it
  was asked for.
- **A tool that changes something and leaves no record does not answer.** That
  rule is held against what the endpoint said about itself, not against the
  verb — and an assistant's `POST` carrying a protocol has reads under it.

## The door is not one of the tools

It is mounted after everything else. What an assistant can reach is what was
mounted before that line, so the door is not among them: nothing can ask this
installation to talk to itself.

That is the arrangement rather than a check, and there is a test that asks for
`assistant_talk` in the listing and expects not to find it.

## What is listed

What this caller can do, not what exists — the same rule the panel's menu
follows. A tool somebody cannot reach is not shown.

The listing is a courtesy. The guard is what actually stops anything, and a
tool named directly still refuses whether or not it was listed.

Asked with no owner, deliberately: an `:own` grant reaches what somebody made
themselves, and a listing is a question about nobody in particular. Holding one
is not enough to be offered a tool that would answer about everybody.

## What a tool takes

Straight out of the endpoint's own declaration: each hole in the address and
each narrowing, with its type, its format and its sentence. `id` is described
as a uuid because the endpoint says it is one.

An assistant sends one flat object, so each argument's description says where
it goes — part of the address, or narrowing what comes back.

What an assistant invented is dropped rather than passed on. Carrying an
undeclared argument through would make the declared shape a suggestion.

The body is the one part not carried across whole. What an endpoint takes has a
**name** in the description — `WritingChanges` — and the shapes behind those
names are not written down yet. Until they are, `body` is an object whose
description says which shape it should be. An assistant is told what to send
and not told its fields.

## A refusal is not an error

The two are different things and the difference decides whether a model can
recover.

| | What it is | Where it goes |
|---|---|---|
| "That address is taken" | a tool did its job and said no | a tool result, `isError: true` |
| "There is no such method" | the client is speaking wrongly | JSON-RPC `error`, code `-32601` |

A model is meant to read the first and try something else — a different slug, a
different id. Putting it in the second is how an assistant stops being able to
recover from anything.

## Notifications

No `id` is JSON-RPC's way of saying no answer is wanted, and it is respected:
a method this build has never heard of gets an error naming it when somebody
asked, and silence when nobody did. `id: null` is read the same as absent,
because that is what a client sending it means.

## Getting in

The door needs an account. A protocol that lists what is there before asking
who is asking is one that describes an installation to whoever connects.

## Where it is

| File | What it decides |
|---|---|
| `server/mavi-assistant/` | the protocol: names, what a tool takes, the envelope — no database, no store, no HTTP |
| `server/mavi-everything/src/assistant.rs` | the door: a name becomes an endpoint, arguments become a call |
| `server/mavi-serve/src/lib.rs` | `Door::call` — the one path from a caller to an answer |
