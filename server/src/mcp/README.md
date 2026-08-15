# mcp

A second surface onto the same data, for something that is not a person.

**Who reaches it.** A panel account, with whatever grants it holds. One
endpoint, `/mcp`, rate limited because a tool loop runs until it is told
otherwise.

**Tables it owns.** None. Every tool reads through the same domains the panel
does, in the same tenant transaction, under the same policy.

**Every tool consumes a grant from the same matrix.** Not a copy of it, not a
list beside it: the same `Needs` the panel's own endpoint asks for, answered by
the same engine. A test asserts every tool's grant is a real one, and another
gives somebody `content:view` and only that, then watches the shop's tool be
absent from the list and refused when called anyway.

**What is listed is what this caller may use.** The same rule the panel's menu
follows — and unlike a menu, the refusal underneath it is what actually stops
anything, which is #174 answered in the place it would otherwise reappear.

**A site on hold reads and does not write**, through here as through anywhere,
because the hold is in the policy rather than in the panel.

**The envelope is JSON-RPC.** A caller sends `{jsonrpc, id, method, params}` —
the shape every MCP client sends, `id` included — and is answered with
`{jsonrpc, id, result}` or `{jsonrpc, id, error}`. `id`'s absence means the
message is a notification: nothing is owed back beyond `202 Accepted`, and a
method this build does not answer is accepted the same silent way rather than
refused, because a future protocol version's notifications are not this
caller's mistake. A method that expects an answer and does not get one it
recognises is told so as a JSON-RPC error object, code `-32601`, rather than
with this API's own refusal shape — that is what a JSON-RPC client parses.

**`jsonrpc` is named and not checked.** The description says a caller sends
it, because a client generated from that description should — but its value
is never read. Rejecting a value this build has not seen (a future `"2.1"`)
would be breaking on the specification's own schedule, not a choice made here.

**`initialize` is answered.** A client that cannot complete the handshake
cannot reach `tools/list` at all, so the one capability declared is `tools`,
and nothing this surface does not have — no `prompts`, no `resources`, no
`sampling` back to the client.

**What it deliberately does not do.**

- Not seventy-four tools. The old surface had that many in one match of four
  thousand six hundred lines; these are the ones worth having, and each new one
  is a `Tool` with a grant on it.
- No tool that changes something without going through the domain that owns it.
  `posts_write` is the same work the panel does — the same language check, the
  same declared fields, the same audit row — reached from a different door.
- A tool that changes something has
  to answer with a receipt like every other mutation, and that is its own
  change.
- Not the whole protocol. `initialize`, listing and calling — the rest of what
  a session could ask (resources, prompts, sampling, cancellation, a resumable
  stream) is unserved: every POST answers with one JSON object rather than
  opening `text/event-stream`, and there is no session id, so a client relying
  on either will not get past the handshake believing it has more than this.
