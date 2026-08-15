# Flows

A flow is a thing a site does on its own: something happens, and steps run in
order. A form arrives and somebody is written to; an order is paid and a
webhook is called; a wait, and then the next step.

One trigger and a list. No branches, no conditions: two triggers is two flows,
and a flow that reads like a program is a program somebody has to debug through
a web page.

## What sets one off

| Trigger | When |
|---|---|
| `form.submitted` | somebody sends one of the site's forms |
| `post.published` | anything the site publishes |
| `post.unpublished` | anything it takes down |
| `order.paid` | money arrived for an order |
| `order.fulfilled` | somebody said the parcel has gone |
| `refund.made` | money went back |
| `stock.low` | what is left of something crossed the line the site set |

A trigger nothing here emits is refused when the flow is made, rather than
being a flow that waits for ever for something nobody sends.

## What a step can be

| Step | What it does |
|---|---|
| `send_mail` | writes to somebody, through the site's own mail |
| `call_webhook` | posts to an address, signed |
| `wait` | comes back later |
| `add_to_list` | puts an address on one of the site's mailing lists |

Each step's settings are whatever it needs, as JSON, which is what the API
keeps.

## Why waiting does not hold anything

A step that waits goes back into the queue with a moment to come back at. A
worker holding a thread for two days is a worker that is not doing anything
else, and a machine restarted in the middle of one is a flow that never
finishes.

## What a run remembers

Which step it is on, as a number of its own. A flow rewritten while one of its
runs is in the air does not move that run: what it is doing is what it was
doing.

A step that fails stops the run and says why, and the run is there to be read
afterwards — a flow that silently did nothing is worse than one that stopped.

## What a flow signs in with

Secrets by name: a step says "use `stripe`", and what `stripe` is is kept
sealed with the machine's own key. Nothing answers with one — the name is the
whole of what can be read back, which is the reason a name exists at all.
