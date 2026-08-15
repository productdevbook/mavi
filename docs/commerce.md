# Selling things

A site that sells is the same site that publishes: the same accounts, the same
mail, the same design. What is added is products, a basket that lives in a
browser, an order, and somebody to take the money.

## What a product is

Its own table, not a kind of post. A post is writing; a product is a price and
a count of how many are left, and the two want different things from a
database: writing wants languages and terms and a body, a product wants a
number nothing can round and a count nothing can take below zero.

```
products ── slug, name, description, price_minor, currency, stock
```

## What a basket is

A list in the buyer's browser, until they buy it. Nothing is reserved by
putting something in one, no row is written, and a shop is not keeping a
record of what somebody nearly bought.

## What checking out does, in one transaction

1. prices what was asked for, from the products themselves rather than from
   what the browser said things cost;
2. reads the discount code, if there is one, and refuses it here rather than
   after the order is made — a code has a minimum, a limit per person, and a
   currency an amount off is an amount **of**;
3. writes the order, numbered per site: nobody writes in about a uuid;
4. **takes the stock**, which is the whole reason this is one transaction —
   two people reaching for the last one is one of them getting it;
5. asks whoever takes the money where to send the buyer.

The same attempt twice is one order. The buyer's browser holds an idempotency
key, and an order already written under that key comes back rather than being
written again.

## Stock that was taken and not paid for

Held rather than gone: an order nobody pays for lets its stock go on a
schedule, so an abandoned basket does not empty a shelf for ever.

## Being paid

Card details never reach this machine. What is kept is how to ask: a provider,
an address, and a signing secret, sealed with the machine's own key. What comes
back is believed only because it is signed, and what it says happened is
applied once however many times it arrives.

Money that arrives another way — a transfer, cash on the day — is said in the
panel, and the record says who said it.

## What an order can do

```
pending ──▶ paid ──▶ fulfilled
   │          └────▶ refunded
   └──▶ cancelled
```

One function decides every move, which is why money only ever goes one way
through it.
