# A lesson's video

Uploaded like anything else a site keeps: the bytes arrive, what kind of file
they are is decided from the bytes, and a row is written. Then a video points
at that file and a lesson points at the video.

```
upload ──▶ media ──▶ video ──▶ lesson
```

## Making it playable

Whatever this machine is configured with. That may be nothing at all — in which
case what was uploaded is what is played, which is right for a course of screen
recordings and wrong for a course of one-hour lectures that have to reach a
phone on a train.

A video that is being made ready says so, and a lesson pointing at one that is
not ready plays nothing rather than a broken player.

## Why it is not served from /uploads

`/uploads/{id}` is public, because what a published page shows is meant to be
seen by anybody. A course's video is not, so it is served from
`/api/learn/videos/{id}`:

- decided per request, against who is on the course and whether their access
  has ended;
- never cached in between, because a cached copy is one that outlives somebody's
  access;
- refused if what it points at is not a video, whatever the row says.

That is the difference between an address that is hard to guess and one that
cannot be shared. This is the second.
