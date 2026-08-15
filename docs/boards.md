# Boards

A board is whatever a site works through in stages: applications, enquiries,
repair jobs, enrolment requests. The site says what the stages are and what
they are called.

## Why this is not a pipeline

The first attempt at this shipped a table called `leads` with six stages
written into the code — new, contacted, qualified, proposal, won, lost — and
three kinds of applicant: person, company, agency. That is one software
agency's sales process and its idea of who writes in. Every site on the machine
got both, and a menu entry named after it, whether it took enrolment requests
or quote requests or nothing at all.

So: a board is a name and a list of stages the site wrote, and a card is a
title, whatever else somebody wanted to say, optionally who it belongs to and
what it is worth.

## What the software knows about a stage

Its name and where it sits. Nothing else — no `won`, no `lost`, no colour with
a meaning attached. A site that wants "closed, and we lost it" names a stage
that.

That is a narrower answer than the one this document used to describe, and it
is the honest one: every meaning the software attaches to a stage is a meaning
somebody's board does not share.

## Moving a card

By being told which stage it is in. Dragging is a way of saying that and not
the only way — a screen that can only move things by dragging is a screen that
cannot be used on a phone, and a card that has to be dragged across four
columns to be closed is four columns of scrolling.

## What a card remembers

What it is, anything else worth saying, and notes added as things happen —
which is what somebody actually reads when a card comes back after two weeks.
