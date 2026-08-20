# Teaching

A course is its own thing here, not a kind of post: it has modules and lessons,
people are put on it for as long as somebody says, and what they have finished
is kept.

```
course ── module ── lesson ── the video it plays
   └── enrolment ── who, and until when
```

## A student is not a panel account

Two different tables, two different sessions, two different cookies. A student
signs in at the site's own front, holds no grants at all, and reaches nothing
in the panel — and the test that says so is one of the ones worth reading:
a panel account is not a student, and a student reaches nothing in the panel.

Putting somebody on a course makes their account and hands back a password
once. It is kept as a hash and cannot be read again; if it is lost, give them
access again and a new one appears.

## Access that ends

An enrolment has an end, or does not. Ninety days sold is ninety days: after
that the course stops opening, the listing stops showing it, and the lesson
behind a typed address is not there.

What they finished stays finished. Letting somebody back in is one call rather
than an enrolment written again, and what was watched is not watched again.

## The video

Uploaded like anything else, and then handed to whatever this machine is
configured to make it playable with — which may be nothing at all, in which
case what was uploaded is what is played.

It is served from `/student/v1/learning/lessons/{id}/media`, decided per
request against who is enrolled, never cached in between, and refused if the
lesson has no media. Public site files use `/public/v1/files/{id}`; a lesson's
media does not.

## What a curriculum is built with

Modules and lessons, in order, over the API — so a site can be set up by
somebody who is not sitting in front of the panel. Two things cannot sit in one
place: being told so is better than two lessons quietly numbered the same.
