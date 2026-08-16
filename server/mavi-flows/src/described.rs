//! What a site does by itself, described.

use mavi_api::{Field, Is, Of, Shape};

const A_TRIGGER: &[&str] = &[
    "something_was_published",
    "somebody_filled_in_a_form",
    "an_order_was_paid_for",
    "an_order_went_out",
    "somebody_was_put_on_a_course",
    "somebody_finished_a_course",
];

const A_DOES: &[&str] = &["send_a_letter", "call_an_address", "wait", "put_on_a_list"];

const WHAT_A_STEP_IS_TOLD: &str = "What this step needs, by name. Which names \
                                   depends on what it does, and `flows.triggers` \
                                   answers that list — an address to call, a \
                                   letter to send, how long to wait.";

#[must_use]
pub fn shapes() -> Vec<Shape> {
    vec![
        Shape::new(
            "Step",
            "One thing a flow does, as it is read back.",
            vec![
                Field::new("does", Of::OneOf(A_DOES), "Which of them."),
                Field::new("told", Of::Whatever, WHAT_A_STEP_IS_TOLD),
                Field::new("place", Of::One(Is::Number), "Where it comes."),
            ],
        ),
        Shape::new(
            "NewStep",
            "One thing for a flow to do.",
            vec![
                Field::new("does", Of::OneOf(A_DOES), "Which of them."),
                Field::new("told", Of::Whatever, WHAT_A_STEP_IS_TOLD).maybe(),
            ],
        ),
        a_flow(),
        Shape::page_of("FlowPage", "Flow", "What a site does by itself."),
        Shape::new(
            "NewFlow",
            "One to arrange.",
            vec![
                Field::new("name", Of::One(Is::Text), "What it is called."),
                Field::new("trigger", Of::OneOf(A_TRIGGER), "What sets it off."),
                Field::new("steps", Of::ManyOf("NewStep"), "What it does, in order."),
            ],
        ),
        Shape::new(
            "FlowChanges",
            "What may be changed. Not what sets it off: a flow arranged for one \
             thing and quietly moved to another is one nobody can reason about \
             from its own runs.",
            vec![
                Field::new("name", Of::One(Is::Text), "What it is called.").maybe(),
                Field::new("on", Of::One(Is::Bool), "Whether it runs at all.").maybe(),
                Field::new(
                    "steps",
                    Of::ManyOf("NewStep"),
                    "The whole list, replaced. A flow's steps are one thing \
                     rather than a collection to add to: what somebody is \
                     editing is the order and the settings together.",
                )
                .maybe()
                .or_null(),
            ],
        ),
        a_run(),
        Shape::page_of("RunPage", "Run", "What has run, newest first."),
        Shape::anything(
            "SomethingMadeUp",
            "Something invented, to try a flow against: whatever the thing that \
             sets this flow off would carry. Not described further, because \
             what that is depends on the trigger.",
        ),
        Shape::new(
            "WouldDo",
            "One step, as it would run.",
            vec![
                Field::new("does", Of::OneOf(A_DOES), "Which of them."),
                Field::new("told", Of::Whatever, "What it was told."),
                Field::new(
                    "about",
                    Of::Whatever,
                    "What it would be working with, once the values from the \
                     thing that set it off are put in.",
                ),
            ],
        ),
        Shape::list_of(
            "WhatItWouldDo",
            "WouldDo",
            "Every step, as it would run, and nothing sent. What this is for is \
             seeing what a flow would do before it does it to somebody.",
        ),
        Shape::new(
            "TriggerList",
            "What can set a flow off, and what a flow can do — with what each \
             one has to be told. Answered rather than written into a screen, so \
             a step this build does not have cannot be arranged.",
            vec![
                Field::new("triggers", Of::Whatever, "Each with a `name`."),
                Field::new(
                    "does",
                    Of::Whatever,
                    "Each with a `name` and the `needs` it has to be told.",
                ),
            ],
        ),
    ]
}

fn a_flow() -> Shape {
    Shape::new(
        "Flow",
        "Something a site does by itself when something happens.",
        vec![
            Field::new("id", Of::One(Is::Id), "Which one."),
            Field::new("name", Of::One(Is::Text), "What it is called."),
            Field::new("trigger", Of::OneOf(A_TRIGGER), "What sets it off."),
            Field::new("on", Of::One(Is::Bool), "Whether it runs at all."),
            Field::new("steps", Of::ManyOf("Step"), "What it does, in order."),
            Field::new("created_at", Of::One(Is::Moment), "When it was arranged."),
        ],
    )
}

fn a_run() -> Shape {
    Shape::new(
        "Run",
        "One journey through a flow.",
        vec![
            Field::new("id", Of::One(Is::Id), "Which one."),
            Field::new("flow_id", Of::One(Is::Id), "Which flow."),
            Field::new("state", Of::One(Is::Text), "Where it has got to."),
            Field::new(
                "about",
                Of::Whatever,
                "What set it off, as it was at the moment it did.",
            ),
            Field::new("at_step", Of::One(Is::Number), "Which step it is on."),
            Field::new(
                "went_wrong",
                Of::One(Is::Text),
                "What stopped it, where something did.",
            )
            .or_null(),
            Field::new("started_at", Of::One(Is::Moment), "When it began."),
            Field::new("finished_at", Of::One(Is::Moment), "When it ended.").or_null(),
        ],
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::step::{Does, TRIGGERS, Trigger};
    use crate::store::{Flow, NewFlow, NewStep, Run, Told, WouldDo};
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

    fn one_of(named: &str) -> Vec<&'static str> {
        shapes()
            .iter()
            .find(|shape| shape.named == named)
            .expect("a shape")
            .fields()
            .iter()
            .find_map(|field| match field.of {
                Of::OneOf(these) => Some(these.to_vec()),
                _ => None,
            })
            .expect("a closed list")
    }

    #[test]
    fn what_can_set_a_flow_off_is_described_as_what_this_build_has() {
        // Written out above and compared here rather than derived, so that a
        // trigger added to the code and not to the description fails rather
        // than quietly not being offered.
        let described = one_of("Flow");
        let there: Vec<&str> = TRIGGERS.iter().map(|one| one.as_str()).collect();

        assert_eq!(described, there);

        let described = one_of("Step");
        let there: Vec<&str> = [
            Does::SendALetter,
            Does::CallAnAddress,
            Does::Wait,
            Does::PutOnAList,
        ]
        .iter()
        .map(|one| one.as_str())
        .collect();

        assert_eq!(described, there);
    }

    #[test]
    fn what_is_described_is_what_is_sent() {
        let step = Told {
            does: Does::Wait.as_str().to_owned(),
            told: serde_json::json!({}),
            place: 1,
        };

        assert_eq!(
            keys(&serde_json::to_value(&step).expect("a step")),
            fields_of("Step")
        );

        let flow = Flow {
            id: uuid::Uuid::nil(),
            name: "A Flow".to_owned(),
            trigger: Trigger::SomebodyFilledInAForm.as_str().to_owned(),
            on: true,
            steps: vec![step],
            created_at: chrono::Utc::now(),
        };

        assert_eq!(
            keys(&serde_json::to_value(&flow).expect("a flow")),
            fields_of("Flow")
        );

        let run = Run {
            id: uuid::Uuid::nil(),
            flow_id: uuid::Uuid::nil(),
            state: "running".to_owned(),
            about: serde_json::json!({}),
            at_step: 0,
            went_wrong: None,
            started_at: chrono::Utc::now(),
            finished_at: None,
        };

        assert_eq!(
            keys(&serde_json::to_value(&run).expect("a run")),
            fields_of("Run")
        );

        let would = WouldDo {
            does: Does::Wait.as_str().to_owned(),
            told: serde_json::json!({}),
            about: serde_json::json!({}),
        };

        assert_eq!(
            keys(&serde_json::to_value(&would).expect("what it would do")),
            fields_of("WouldDo")
        );
    }

    #[test]
    fn what_is_described_is_what_is_taken() {
        let step = NewStep {
            does: Does::Wait.as_str().to_owned(),
            told: serde_json::json!({}),
        };

        assert_eq!(
            keys(&serde_json::to_value(&step).expect("a step")),
            fields_of("NewStep")
        );

        let flow = serde_json::to_value(NewFlow {
            name: "A Flow".to_owned(),
            trigger: Trigger::SomebodyFilledInAForm.as_str().to_owned(),
            steps: vec![step],
        })
        .expect("a new flow");

        assert_eq!(keys(&flow), fields_of("NewFlow"));
    }
}
