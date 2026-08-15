# mail

Lists, subscribers, and the messages a site sends them.

**Who reaches it.** The panel, with `mail:*`. One endpoint is public: leaving a
list, which takes a token rather than an address.

**Tables it owns.** `mail_lists`, `subscribers`, `subscriber_lists`,
`campaigns`, `email_log`, `mail_events`.

**A campaign remembers where it stopped.** `sent_through` is the id of the last
subscriber a batch reached, and the next batch starts after it. Reading
everything already sent in order to find the next hundred is what made this
quadratic before — a list that got slower the further it got — and a test sends
two batches over two hundred and fifty subscribers and asserts the second cost
what the first did.

**Each batch queues the next**, in the transaction that finished it. A list of
ten thousand is a hundred pieces of work rather than one that holds a worker
for an hour, and a process that dies mid-campaign resumes where it stopped.

**Everything sent is written down** — campaign or not, in `email_log`, before
it goes anywhere. What is not written down is not billed, and mail that was not
a campaign is exactly what went uncounted before.

**Leaving is a token.** The link at the bottom of a message carries something
unguessable, kept hashed, and unsubscribing somebody else is not a matter of
typing their address. An unknown token gets the same answer a real one does.

**Somebody who left stays left.** Adding them to a list again does not start
sending to them; their state does that, and only they can change it back.

**Retention.** `email_log` is kept two years and then swept. A subscriber is
kept as long as the site keeps them, or until they leave.

**Everything leaves through one door.** An invitation, a reset and a campaign's
next hundred all go through `mail::post`, which writes the row and queues the
handing over. A request never waits on somebody else's mail server, and what is
written down is what is billed.

**Where it goes is an enum with two arms.** SMTP where one is configured, and
recorded where none is — a machine with nothing set up is obviously not
sending rather than quietly not sending, and a test reads back what would have
gone.

**A provider that refuses for good is a failure**; one that cannot be reached
is an error the queue backs off and tries again. The difference is what stops a
bad address being retried for a week.

**A campaign carries a way out of the list** and a message somebody asked for
by acting does not, because there is no list to leave. The link travels with
the message rather than being pasted into a body by whoever wrote it.

**A bounce or a complaint stops the site writing to them again**, and the same
event arriving twice is one event — the provider's own reference is the key.

**What it deliberately does not do.**

- No templates in campaigns. A campaign carries its own body; the letters a
  site sends one person at a time have their wording in `letters`.
- The provider's callback arrives through the panel rather than at an endpoint
  of its own: a signed webhook receiver belongs at the machine's edge, and a
  site's address is not where a provider should be posting.
- No open or click tracking, and no plans for one.
