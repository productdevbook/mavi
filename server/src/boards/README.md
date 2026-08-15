# boards

A board, its columns, and the cards on them.

**Who reaches it.** The panel, with `boards:*`. Nothing here is public.

**Tables it owns.** `boards`, `board_stages`, `cards`, `card_notes`.

**A board arrives with somewhere to put things.** Made without columns, it gets
three; a board with none is a board nothing goes on.

**A card's place is a float.** Moving one between two others is one row
changed rather than every row below it renumbered — which is what a drag across
a column of two hundred would otherwise cost.

**A board opens in two queries** whatever is on it: one for the board, one join
for every column and card. A test puts thirty cards on and counts.

**An amount and a currency arrive together or not at all**, and the database
says so as well as the handler. A card is worth something in something, or it
is worth nothing.

**What it deliberately does not do.**

- No automation. A card moving to a column does not send anything, though the
  outbox is right there for the day it should.
- No deleting a column. The cards on it would have to go somewhere and nothing
  says where.
- No filters, no search, no per-person view. A board is small enough to read.
