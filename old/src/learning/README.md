# learning

Courses, and the people taking them.

**Who reaches it.** The panel, with `courses:*`, to write a course and put
somebody on it. The site's own front, as a student, to see what they are on and
work through it. Signing in as a student is public and rate limited.

**Tables it owns.** `courses`, `modules`, `lessons`, `students`,
`student_sessions`, `enrolments`, `lesson_progress`.

**A student is not a panel account.** A different table, a different cookie, a
different session table, and no grants at all. A student's cookie opens nothing
in the panel and a panel token is not taken as a student's — there is a test for
each direction, because this is the one place two kinds of person meet.

**What a student sees is what they were put on**, not what the site teaches. A
course nobody enrolled them on is not there, and a lesson on it cannot be
finished. Both are 404s rather than refusals: whether a course exists is not a
question this answers to somebody who is not on it.

**The curriculum is three queries** — the enrolment, the course, and one join
for every module and lesson with this student's progress on it. A test builds a
course with twenty lessons and checks the count did not move, because a lesson
per query is how a course gets slow at exactly the size where it matters.

**Retention.** A student is kept as long as the site keeps them. Their sessions
go thirty days after they lapse, swept with everybody else's.

**What it deliberately does not do.**

- No enrolment by the student. Somebody puts them on a course; there is no
  self-service sign-up and no payment attached to one.
- Enrolling presses `student.invited` and sends it, but also hands the
  password back in the response — whoever is enrolling somebody is not
  signed in as them, so there is no other screen this could be read from if
  the letter never arrives.
- No certificates, no quizzes, no marks. A lesson is finished or it is not.
- No ordering endpoints for modules and lessons. The positions exist and the
  panel has nothing yet to move them with.
