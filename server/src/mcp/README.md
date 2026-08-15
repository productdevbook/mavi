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
- Not the whole protocol. Two methods — listing and calling — which are the two
  this serves.
