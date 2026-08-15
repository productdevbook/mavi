-- Two kinds of person, and only one of them could ever sign in.
--
-- An operator owned the machine and signed into a console over every site on
-- it. A user belonged to a site and signed into the panel. Setting up wrote
-- both, from one form, with the same address, the same name and the same
-- password hash — and then only the user could get back in, because **nothing
-- in this repository has ever inserted into `operator_sessions`**. There is no
-- operator sign-in endpoint. The row written at setup could not be used again
-- by anything, here or anywhere else: no console was ever deployed against
-- these tables either.
--
-- So this is not a merge of two things that were both doing work. It is the
-- removal of the half that never did any.
--
-- Dropped rather than moved. Handing them to a crate that depends on this one,
-- through the seam its own migrations travel, was the earlier plan and it was
-- wrong: it would mount another crate's tables inside a site's own database,
-- which is the arrangement all of this is undoing. A console over many
-- installations does not keep its rows in one of them.
--
-- What is worth keeping out of `console_log` folds into `audit_log`, which has
-- had an actor kind for the machine's own work all along. There was one writer
-- — setting up — and no reader.

drop table console_log;

drop table operator_sessions;

drop table operators;
