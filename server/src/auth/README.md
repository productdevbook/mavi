# auth

How somebody working on a site gets in, and stays in.

**Tables it owns.** `sessions` (with the kernel), `second_factors`,
`recovery_codes`, `oauth_providers`, `oauth_attempts`.

## A session

One door out, `open_session`: however somebody arrived — a password, an
authenticator, another site's account — they leave with the same thing, made
the same way. Signing in revokes whatever that account was carrying before, so
a session handed to somebody by a borrowed screen is not a session they keep.

A wrong password and an address nobody has both answer the same way and take
the same time. Telling them apart, by wording or by timing, is how a list of
addresses gets tested against a site.

## The second factor

TOTP, RFC 6238, the six digits an authenticator app already on somebody's phone
will show. SHA-1 is not a choice here — it is what those apps compute.

- **One per account.** Two authenticators for one login is a way to be locked
  out by whichever was set up second.
- **The secret is sealed** with the machine's keyring, so a copy of the table
  is not a drawer of working authenticators.
- **A code works once.** The step it was for is written down, and nothing at or
  before that step is taken again — a code that works twice is a code somebody
  read over a shoulder.
- **A clock half a minute out is still believed**, one step either way, and no
  wider.
- **Ten recovery codes**, shown once when the factor is confirmed, hashed like a
  session token. One is spent in the statement that finds it, so two sign-ins
  racing spend two codes rather than one twice.
- **Taking it off asks for the password again**, because that is the first
  change a borrowed session would want to make.
- **Nothing says whether an account has one** until the password is right.

The digits are sent with the password rather than exchanged for a half-signed-in
token: a token standing between the two halves is a token worth stealing, and
the panel has the password in hand at the moment it is told the digits are
wanted. What comes back is `second_factor_required`, told apart from every other
no so the panel asks for digits rather than saying the password was wrong.

## Another site's account

Authorization code with PKCE. Which providers a site trusts is the site's own
business, so the addresses are configured rather than compiled in: what this
machine knows is how the exchange goes, not who is on the other end.

- **An attempt is written down before anybody leaves**, and spent as it is found
  when they come back. An answer that arrives twice is one this machine asked
  for once.
- **Nobody is sent anywhere but back into this site.** A whole address there is
  an open redirect, which turns a sign-in screen into a way to make somebody
  else's page look like ours.
- **No account is made here.** Somebody arriving with an address nobody invited
  is turned away — otherwise owning a mailbox is enough to get into a site.
- **An unverified address is not believed**: it is an address somebody typed,
  not one they hold, and typing an editor's address is otherwise the whole
  attack.
- **The second factor is a second factor whichever door was used.**
- **The client secret is sealed and never read back out** by any endpoint.
- Provider addresses are reached through `kernel::outbound`, which resolves,
  checks, and pins — a site owner configuring where this machine sends its
  requests is otherwise a way to ask it to fetch from inside its own network.

## What it deliberately does not do

- No WebAuthn yet. It is the better factor and it is a bigger piece of work
  than what is here; it goes beside TOTP rather than instead of it.
- No SMS. A code that arrives by text is a code somebody else's phone company
  can be talked into handing over.
- No password reset here — that is a ticket, and it lives with the people who
  are invited.
