//! What somebody is learning.
//!
//! A course is modules, a module is lessons, and somebody enrolled works
//! through them. Two audiences again, and further apart than anywhere else in
//! this crate's family: whoever writes a course holds an account and reaches
//! the panel, and whoever takes it holds nothing at all and reaches only what
//! they are on.
//!
//! That is why a student is [`student::Standing`] and not an account with
//! everything switched off — an account with everything switched off is one
//! flag away from an account with something switched on.

pub mod sequence;
pub mod store;
pub mod student;

use mavi_api::{Answers, Endpoint, Is, Method, Parameter, Who};
use mavi_core::error::Code;
use mavi_core::grant::{Access, Needs};
use mavi_core::id;
use mavi_core::page::{Key, Keyset, Kind};
use serde::{Deserialize, Serialize};

pub use sequence::in_this_order;
pub use student::{Standing, may_open, may_sign_in};

id!(
    /// One course.
    CourseId
);

id!(
    /// One part of a course.
    ModuleId
);

id!(
    /// One lesson.
    LessonId
);

id!(
    /// Somebody learning.
    StudentId
);

pub const COURSES: &str = "courses";

#[must_use]
pub const fn to_read() -> Needs {
    Needs::new(COURSES, Access::View)
}

#[must_use]
pub const fn to_write() -> Needs {
    Needs::new(COURSES, Access::Write)
}

/// Whether a course takes anybody.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum State {
    /// Being written. Nobody is on it and nobody can see it.
    Draft,
    /// Open. Whoever is on it can work through it.
    Open,
    /// Finished with. Whoever was on it keeps what they did and cannot open
    /// anything further — closing a course is not deleting it, and a
    /// certificate somebody earned in March still says so in December.
    Closed,
}

impl State {
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            State::Draft => "draft",
            State::Open => "open",
            State::Closed => "closed",
        }
    }

    #[must_use]
    pub const fn is_open(self) -> bool {
        matches!(self, State::Open)
    }
}

pub const BY_RECENT: Keyset = Keyset(&[
    Key::newest("created_at", Kind::Moment),
    Key::newest("id", Kind::Id),
]);

#[must_use]
pub fn endpoints() -> Vec<Endpoint> {
    let mut all = the_courses();
    all.extend(what_is_in_them());
    all.extend(who_is_on_it());
    all.extend(taking_a_course());
    all
}

/// The courses themselves.
fn the_courses() -> Vec<Endpoint> {
    vec![
        Endpoint {
            method: Method::Get,
            path: "/api/courses",
            named: "courses.list",
            about: "Every course this site has, newest first.",
            who: Who::AnAccount,
            parameters: vec![
                Parameter::query("state", Is::Text, "Only courses sitting here."),
                Parameter::query("after", Is::Text, "The cursor the last page ended with."),
                Parameter::query("limit", Is::Number, "How many, at most a hundred."),
            ],
            takes: None,
            answers: Answers::With("CoursePage"),
            refuses: &[],
            changes: false,
        },
        Endpoint {
            method: Method::Post,
            path: "/api/courses",
            named: "courses.make",
            about: "Starts one.",
            who: Who::AnAccount,
            parameters: Vec::new(),
            takes: Some("NewCourse"),
            answers: Answers::Made("Course"),
            refuses: &[Code::Conflict],
            changes: true,
        },
        Endpoint {
            method: Method::Get,
            path: "/api/courses/{id}",
            named: "courses.read",
            about: "One course, its modules and its lessons, in order.",
            who: Who::AnAccount,
            parameters: vec![Parameter::path("id", Is::Id, "Which course.")],
            takes: None,
            answers: Answers::With("Course"),
            refuses: &[Code::NotFound],
            changes: false,
        },
        Endpoint {
            method: Method::Patch,
            path: "/api/courses/{id}",
            named: "courses.change",
            about: "Renames one, or opens or closes it.",
            who: Who::AnAccount,
            parameters: vec![Parameter::path("id", Is::Id, "Which course.")],
            takes: Some("CourseChanges"),
            answers: Answers::With("Course"),
            refuses: &[Code::NotFound, Code::Conflict],
            changes: true,
        },
        Endpoint {
            method: Method::Put,
            path: "/api/courses/{id}/order",
            named: "courses.reorder",
            about: "Says what order the modules are in. The same ones, somewhere else.",
            who: Who::AnAccount,
            parameters: vec![Parameter::path("id", Is::Id, "Which course.")],
            // The whole order at once rather than "move this one up": one
            // statement, checked when it commits, instead of a dance of
            // temporary numbers that a crash leaves half done.
            takes: Some("TheOrder"),
            answers: Answers::With("Course"),
            refuses: &[Code::NotFound],
            changes: true,
        },
    ]
}

/// What is in them, and the order it is in.
fn what_is_in_them() -> Vec<Endpoint> {
    vec![
        Endpoint {
            method: Method::Post,
            path: "/api/courses/{id}/modules",
            named: "modules.make",
            about: "Adds a part to a course.",
            who: Who::AnAccount,
            parameters: vec![Parameter::path("id", Is::Id, "Which course.")],
            takes: Some("NewModule"),
            answers: Answers::Made("Module"),
            refuses: &[Code::NotFound],
            changes: true,
        },
        Endpoint {
            method: Method::Put,
            path: "/api/modules/{id}/order",
            named: "modules.reorder",
            about: "Says what order the lessons in one part are in.",
            who: Who::AnAccount,
            parameters: vec![Parameter::path("id", Is::Id, "Which module.")],
            takes: Some("TheOrder"),
            answers: Answers::With("Module"),
            refuses: &[Code::NotFound],
            changes: true,
        },
        Endpoint {
            method: Method::Post,
            path: "/api/modules/{id}/lessons",
            named: "lessons.make",
            about: "Adds a lesson to a part.",
            who: Who::AnAccount,
            parameters: vec![Parameter::path("id", Is::Id, "Which module.")],
            takes: Some("NewLesson"),
            answers: Answers::Made("Lesson"),
            refuses: &[Code::NotFound],
            changes: true,
        },
        Endpoint {
            method: Method::Patch,
            path: "/api/lessons/{id}",
            named: "lessons.change",
            about: "Changes what a lesson says.",
            who: Who::AnAccount,
            parameters: vec![Parameter::path("id", Is::Id, "Which lesson.")],
            takes: Some("LessonChanges"),
            answers: Answers::With("Lesson"),
            refuses: &[Code::NotFound],
            changes: true,
        },
    ]
}

/// Who is learning, and on what.
fn who_is_on_it() -> Vec<Endpoint> {
    vec![
        Endpoint {
            method: Method::Get,
            path: "/api/students",
            named: "students.list",
            about: "Everybody learning here, newest first.",
            who: Who::AnAccount,
            parameters: vec![
                Parameter::query("standing", Is::Text, "Only those standing like this."),
                Parameter::query("after", Is::Text, "The cursor the last page ended with."),
                Parameter::query("limit", Is::Number, "How many, at most a hundred."),
            ],
            takes: None,
            answers: Answers::With("StudentPage"),
            refuses: &[],
            changes: false,
        },
        Endpoint {
            method: Method::Post,
            path: "/api/students",
            named: "students.ask",
            about: "Writes to somebody, asking them to take up an account.",
            who: Who::AnAccount,
            parameters: Vec::new(),
            takes: Some("SomebodyToAsk"),
            answers: Answers::Made("Student"),
            refuses: &[Code::Conflict],
            changes: true,
        },
        Endpoint {
            method: Method::Post,
            path: "/api/courses/{id}/students",
            named: "enrolments.add",
            about: "Puts somebody on a course. Twice is once.",
            who: Who::AnAccount,
            parameters: vec![Parameter::path("id", Is::Id, "Which course.")],
            takes: Some("WhoToPutOn"),
            answers: Answers::Made("Enrolment"),
            refuses: &[Code::NotFound, Code::Conflict],
            changes: true,
        },
        Endpoint {
            method: Method::Delete,
            path: "/api/enrolments/{id}",
            named: "enrolments.remove",
            about: "Takes somebody off a course. What they did stays theirs.",
            who: Who::AnAccount,
            parameters: vec![Parameter::path("id", Is::Id, "Which enrolment.")],
            takes: None,
            answers: Answers::Nothing,
            refuses: &[Code::NotFound],
            changes: true,
        },
    ]
}

/// What somebody learning reaches, holding no grants at all.
fn taking_a_course() -> Vec<Endpoint> {
    vec![
        Endpoint {
            method: Method::Get,
            path: "/api/learning",
            named: "learning.mine",
            about: "The courses this student is on.",
            who: Who::AStudent,
            parameters: Vec::new(),
            takes: None,
            answers: Answers::With("LearningList"),
            refuses: &[],
            changes: false,
        },
        Endpoint {
            method: Method::Get,
            path: "/api/learning/lessons/{id}",
            named: "learning.lesson",
            about: "One lesson, if they are on the course and it is open.",
            who: Who::AStudent,
            parameters: vec![Parameter::path("id", Is::Id, "Which lesson.")],
            takes: None,
            answers: Answers::With("Lesson"),
            // Forbidden rather than not found: the lesson exists and is not
            // theirs, and saying it does not exist makes the real "no such
            // lesson" mean two things.
            refuses: &[Code::NotFound, Code::Forbidden],
            changes: false,
        },
        Endpoint {
            method: Method::Put,
            path: "/api/learning/lessons/{id}/done",
            named: "learning.done",
            about: "Says a lesson is done. Saying it twice is saying it once.",
            who: Who::AStudent,
            parameters: vec![Parameter::path("id", Is::Id, "Which lesson.")],
            takes: None,
            answers: Answers::With("Progress"),
            refuses: &[Code::NotFound, Code::Forbidden],
            changes: true,
        },
    ]
}

#[cfg(test)]
mod tests {
    use super::*;
    use mavi_api::Api;

    #[test]
    fn everything_this_domain_answers_is_described_completely() {
        let holes = Api::of(endpoints()).holes();

        assert!(holes.is_empty(), "{holes:#?}");
    }

    #[test]
    fn no_two_of_these_are_the_same_route() {
        let clashes = Api::of(endpoints()).clashes();

        assert!(clashes.is_empty(), "{clashes:#?}");
    }

    #[test]
    fn nothing_a_student_reaches_is_mixed_in_with_the_panel() {
        // Two audiences that are further apart than any other pair here, and
        // the paths say so: everything a student reaches is under
        // `/api/learning`, so "what can somebody holding no grants get to" is
        // answered by reading rather than by trusting ten declarations.
        for endpoint in endpoints() {
            assert_eq!(
                endpoint.who == Who::AStudent,
                endpoint.path.starts_with("/api/learning"),
                "{} is one thing in its path and another in its audience",
                endpoint.named
            );
        }
    }

    #[test]
    fn nothing_here_is_open_to_anybody_at_all() {
        // A course is somebody's work and somebody else has paid for it.
        // Where that changes, it will be an endpoint under `/api/open/` that
        // somebody wrote on purpose.
        assert!(endpoints().iter().all(|e| e.who != Who::Anybody));
    }

    #[test]
    fn what_this_domain_asks_for_is_a_capability_the_site_has() {
        assert!(mavi_people::is_a_capability(COURSES));
    }
}
