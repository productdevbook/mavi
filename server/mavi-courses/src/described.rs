//! What a site teaches, and what somebody learning it sees.

use mavi_api::{Field, Is, Of, Shape};

/// Where a course has got to. Closed, and this software's: a draft is not
/// bought, an open one is, and a closed one keeps whoever is already on it.
const A_STATE: &[&str] = &["draft", "open", "closed"];

#[must_use]
pub fn shapes() -> Vec<Shape> {
    let mut all = vec![a_lesson(), a_module(), a_course()];

    all.extend([
        Shape::page_of("CoursePage", "Course", "What a site teaches."),
        Shape::list_of(
            "LearningList",
            "Course",
            "What one student is on. Theirs, and nobody else's.",
        ),
    ]);
    all.extend(what_is_written());
    all.extend(the_students());

    all
}

fn a_course() -> Shape {
    Shape::new(
        "Course",
        "Something a site teaches.",
        vec![
            Field::new("id", Of::One(Is::Id), "Which one."),
            Field::new("slug", Of::One(Is::Text), "Where it answers."),
            Field::new("title", Of::One(Is::Text), "What it is called."),
            Field::new("about", Of::One(Is::Text), "What it teaches.").or_null(),
            Field::new(
                "state",
                Of::OneOf(A_STATE),
                "Where it has got to. A draft cannot be bought; a closed one \
                 keeps whoever is already on it.",
            ),
            Field::new(
                "modules",
                Of::ManyOf("Module"),
                "What it is made of. **Empty in a listing** — what a listing is \
                 for is choosing a course, and carrying every lesson of every \
                 one of them is a page that grows with the site rather than \
                 with the screen.",
            ),
            Field::new("created_at", Of::One(Is::Moment), "When it was started."),
        ],
    )
}

fn a_module() -> Shape {
    Shape::new(
        "Module",
        "One part of a course.",
        vec![
            Field::new("id", Of::One(Is::Id), "Which one."),
            Field::new("title", Of::One(Is::Text), "What it is called."),
            Field::new(
                "place",
                Of::One(Is::Number),
                "Where it comes in the course.",
            ),
            Field::new("lessons", Of::ManyOf("Lesson"), "What is in it."),
        ],
    )
}

fn a_lesson() -> Shape {
    Shape::new(
        "Lesson",
        "One lesson.",
        vec![
            Field::new("id", Of::One(Is::Id), "Which one."),
            Field::new("module_id", Of::One(Is::Id), "Which part it is in."),
            Field::new("title", Of::One(Is::Text), "What it is called."),
            Field::new("body", Of::One(Is::Text), "What it says."),
            Field::new("place", Of::One(Is::Number), "Where it comes in the part."),
        ],
    )
}

fn what_is_written() -> Vec<Shape> {
    vec![
        Shape::new(
            "NewCourse",
            "One to start.",
            vec![
                Field::new("slug", Of::One(Is::Text), "Where it should answer."),
                Field::new("title", Of::One(Is::Text), "What it is called."),
                Field::new("about", Of::One(Is::Text), "What it teaches.")
                    .maybe()
                    .or_null(),
            ],
        ),
        Shape::new(
            "CourseChanges",
            "What may be changed. Its address is not among them: it is what \
             every link to the course points at.",
            vec![
                Field::new("title", Of::One(Is::Text), "What it is called.").maybe(),
                Field::new("about", Of::One(Is::Text), "What it teaches.").maybe(),
                Field::new("state", Of::OneOf(A_STATE), "Where it has got to.").maybe(),
            ],
        ),
        Shape::new(
            "NewModule",
            "A part to add. It goes on the end; moving it is `TheOrder`.",
            vec![Field::new("title", Of::One(Is::Text), "What it is called.")],
        ),
        Shape::new(
            "NewLesson",
            "A lesson to add. It goes on the end of its part.",
            vec![
                Field::new("title", Of::One(Is::Text), "What it is called."),
                Field::new("body", Of::One(Is::Text), "What it says."),
            ],
        ),
        Shape::new(
            "LessonChanges",
            "What may be changed about one. Where it comes is not among them: \
             moving it is `TheOrder`, so that one lesson cannot be dragged \
             somewhere without the rest being told.",
            vec![
                Field::new("title", Of::One(Is::Text), "What it is called.").maybe(),
                Field::new("body", Of::One(Is::Text), "What it says.").maybe(),
            ],
        ),
        Shape::new(
            "TheOrder",
            "The same things in a new order. Refused if it is not exactly that: \
             a list with something missing is a lesson quietly dropped out of a \
             course, and one with something extra is a lesson from somebody \
             else's course being pulled into this one.",
            vec![Field::new(
                "order",
                Of::Many(Is::Id),
                "Every one of them, in the order they should come.",
            )],
        ),
    ]
}

fn the_students() -> Vec<Shape> {
    vec![
        Shape::new(
            "Student",
            "Somebody learning here. **Not a panel account** — what a student \
             may reach is their own lessons, and nothing about the site.",
            vec![
                Field::new("id", Of::One(Is::Id), "Which one."),
                Field::new("email", Of::One(Is::Text), "Where they are reached."),
                Field::new("name", Of::One(Is::Text), "What they are called."),
                Field::new(
                    "standing",
                    Of::One(Is::Text),
                    "Whether they may still get in.",
                ),
                Field::new("created_at", Of::One(Is::Moment), "When they were asked."),
            ],
        ),
        Shape::page_of("StudentPage", "Student", "Everybody learning here."),
        Shape::new(
            "SomebodyToAsk",
            "Somebody to invite to learn here.",
            vec![
                Field::new("email", Of::One(Is::Text), "Where to reach them."),
                Field::new("name", Of::One(Is::Text), "What to call them."),
            ],
        ),
        Shape::new(
            "WhoToPutOn",
            "Which student to put on this course.",
            vec![Field::new("student", Of::One(Is::Id), "Which one.")],
        ),
        Shape::new(
            "Enrolment",
            "They are on it.",
            vec![
                Field::new("id", Of::One(Is::Id), "Which enrolment."),
                Field::new("course", Of::One(Is::Id), "Which course."),
                Field::new("student", Of::One(Is::Id), "Which student."),
            ],
        ),
        Shape::new(
            "Progress",
            "One lesson marked as done.",
            vec![
                Field::new("lesson", Of::One(Is::Id), "Which lesson."),
                Field::new("at", Of::One(Is::Moment), "When they finished it."),
            ],
        ),
    ]
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::State;
    use crate::store::{Course, CourseChanges, Lesson, LessonChanges, Module, NewCourse, Student};
    use std::collections::BTreeSet;

    fn fields_of(named: &str) -> BTreeSet<&'static str> {
        shapes()
            .iter()
            .find(|shape| shape.named == named)
            .expect("a shape")
            .fields()
            .iter()
            .map(|field| field.name)
            .collect()
    }

    fn keys(what: &serde_json::Value) -> BTreeSet<&str> {
        what.as_object()
            .expect("an object")
            .keys()
            .map(String::as_str)
            .collect()
    }

    #[test]
    fn what_is_described_is_what_is_sent() {
        let lesson = Lesson {
            id: uuid::Uuid::nil(),
            module_id: uuid::Uuid::nil(),
            title: "A Lesson".to_owned(),
            body: "Something taught.".to_owned(),
            place: 1,
        };

        assert_eq!(
            keys(&serde_json::to_value(&lesson).expect("a lesson")),
            fields_of("Lesson")
        );

        let module = Module {
            id: uuid::Uuid::nil(),
            title: "A Part".to_owned(),
            place: 1,
            lessons: vec![lesson],
        };

        assert_eq!(
            keys(&serde_json::to_value(&module).expect("a module")),
            fields_of("Module")
        );

        let course = Course {
            id: uuid::Uuid::nil(),
            slug: "a-course".to_owned(),
            title: "A Course".to_owned(),
            about: None,
            state: State::Draft,
            modules: vec![module],
            created_at: chrono::Utc::now(),
        };

        assert_eq!(
            keys(&serde_json::to_value(&course).expect("a course")),
            fields_of("Course")
        );

        let student = Student {
            id: uuid::Uuid::nil(),
            email: "somebody@example.test".to_owned(),
            name: "Somebody".to_owned(),
            standing: "here".to_owned(),
            created_at: chrono::Utc::now(),
        };

        assert_eq!(
            keys(&serde_json::to_value(&student).expect("a student")),
            fields_of("Student")
        );
    }

    #[test]
    fn what_is_described_is_what_is_taken() {
        let new = serde_json::to_value(NewCourse {
            slug: "a-course".to_owned(),
            title: "A Course".to_owned(),
            about: None,
        })
        .expect("a new course");

        assert_eq!(keys(&new), fields_of("NewCourse"));

        assert_eq!(
            keys(&serde_json::to_value(CourseChanges::default()).expect("changes")),
            fields_of("CourseChanges")
        );

        assert_eq!(
            keys(&serde_json::to_value(LessonChanges::default()).expect("changes")),
            fields_of("LessonChanges")
        );
    }
}
