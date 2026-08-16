# shop

What a site sells, and what happens when somebody buys it.

**Who reaches it.** The panel, with `shop:*`. Three endpoints are public: what
is for sale, checking out, and looking up an order by its id.

**Tables it owns.** `products`, `orders`, `order_items`, `stock_holds`,
`coupons`, `coupon_uses`. The basket is the visitor's, held by the page they
are on; checking out sends its lines in one request.

**Money is minor units and a currency.** There is no way to send a price as a
decimal, and no arithmetic on it that is not `Money`'s — which refuses to add
two currencies and errors rather than wrapping.

**No card ever touches this.** There is nowhere to put one. Checking out asks a
hosted provider for somewhere to send the person, and the card is typed on the
provider's own page; what is kept is the provider's name for the attempt and
nothing that could be used to charge anybody. That is a rule about what this can
be made to do, not a preference.

**A callback is only what it is signed as.** Without a signature it is somebody
saying an order was paid for, and it is refused. What it does is idempotent, and
an amount that is not what the order came to is not a payment.

**Reconciliation asks the provider directly.** A callback that never arrived
leaves the provider holding money for an order this says is unpaid; the pass
finds the difference, puts it right, and says so in the log — because a
difference that keeps appearing is a fault somewhere else.

**Stock is taken with the row locked**, for the length of the transaction that
takes it, so two people reaching for the last one of something do not both get
it. The test runs exactly that, on two connections, at the same time. A check
constraint refuses a negative, so a race that got past everything is a
transaction that fails rather than a shop that owes somebody something.

**What is taken is held, not sold.** A hold lasts thirty minutes and goes back
on the shelf when it lapses, so an abandoned checkout does not keep the last one
all afternoon. Paying releases the hold without returning the stock; refunding
puts it back.

**The same attempt twice buys one thing once.** The caller names its attempt and
the name is unique within the site, so a back button, a retry, and a flaky
network all end at one order.

**A one-use code is used once**, because a use is a row with a unique key on it
rather than a counter something reads and then updates.

**States.** `pending → paid | cancelled`, `paid → fulfilled | refunded`,
`fulfilled → refunded`. Cancelled and refunded are ends. Money only goes one way
through that, which is why it is one function rather than a condition in four
handlers.

**Events.** `order.paid`, `order.fulfilled`, `refund.made`, `stock.low`. The
first two also press a letter to the address on the order — whoever paid, and
whoever it shipped to, whichever way the order got there: an admin marking it,
the provider's own callback, or reconciliation finding a difference.

**Jobs.** `shop.release-holds` puts back what an abandoned checkout was holding.
`shop.drop-stuck` lets go of an order nobody paid for after a day.
`shop.low-stock` says once when there is nearly none of something left.

**What it deliberately does not do.**

- No provider is configured by default, and a site without one gets an order
  with nowhere to pay for it rather than an order that looks payable.
- No partial capture, no saved cards, no subscriptions.
- No cart endpoints yet. The tables are there; checkout takes its lines
  directly, which is what a shop front does anyway.
- No partial refunds, no shipping, no tax. Each is its own decision and none is
  quietly half-made.
