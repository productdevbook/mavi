# Where uploaded files are kept

One place, chosen by whoever runs the machine, and a site does not know which
it is.

```
upload ──▶ what the bytes say it is ──▶ the store ──▶ /uploads/{id}
```

## What is checked on the way in

**The bytes, not the name.** A file called `holiday.png` that is not a picture
is refused; a picture called `notes.txt` is kept as a picture. A name proves
nothing, and a name is the only thing a browser sends that somebody chose.

**How big.** A single file has a ceiling, and so does everything a site keeps
altogether — the second one matters more: a full disk on a machine serving
other people's sites is every site rather than one. What a site may keep is set
per site by the operator; a site that has been sold more room has more room.

## What is served, and how

Anything a site has uploaded answers at `/uploads/{id}`, with two headers that
are the whole of the safety here:

- the kind **this machine decided** when the bytes arrived, never the string in
  the row, and `nosniff` so a browser cannot decide otherwise;
- a picture is shown where it stands and everything else is handed over, because
  a file that is shown where it stands is a file that runs where it stands if it
  turns out to be something other than what it said.

## A course's video is not a picture

`/uploads/{id}` is public: what a published page shows is meant to be seen. A
video a lesson plays is not, so it is served from `/api/learn/videos/{id}`
instead — decided per request against who is on the course and whether their
access has ended, never cached in between, and refused outright if what it
points at is not a video.

## Where the bytes actually live

Behind one interface, so a site never knows: this machine's disk by default, or
a bucket where one is configured. Whichever it is, it is the operator's
decision and the same for every site on the machine — a site that could name
its own bucket is a site whose pictures disappear when somebody else stops
paying a bill.
