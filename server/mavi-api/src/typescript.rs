//! The description, as the panel reads it.
//!
//! Written from the shapes rather than typed out again, and written by a
//! **test** rather than by a build step: the file is committed, and the test
//! fails when it and the code disagree. So a shape that changes and a panel
//! that has not caught up is a red build rather than a screen that breaks in
//! somebody's browser.
//!
//! Nothing here is a client. What a call looks like, what carries the token,
//! how a refusal is shown — those are the panel's, and a generator that
//! decided them would be deciding them for everybody. What is here is the
//! vocabulary: the shape of every body, and which operation lives where.

use std::fmt::Write;

use crate::{Api, Is, Of, Shape, What};

/// What is written at the top, so nobody edits the file by hand.
const SAID_FIRST: &str = "// Written from the description of the API. Do not edit by hand —\n\
                          // `cargo test -p mavi-everything --test described` writes it, and\n\
                          // fails when what was here is not what it wrote.\n";

/// Every shape and every operation, as TypeScript.
#[must_use]
pub fn typescript(api: &Api) -> String {
    let mut out = String::from(SAID_FIRST);

    out.push_str(REFUSAL);

    let mut shapes: Vec<&Shape> = api.shapes.iter().collect();
    shapes.sort_by_key(|shape| shape.named);

    for shape in shapes {
        out.push('\n');
        out.push_str(&declared(shape));
    }

    out.push('\n');
    out.push_str(&operations(api));

    out
}

/// The one shape no domain owns: what every operation can answer with.
const REFUSAL: &str = "
/** What every operation can answer with instead. */
export interface Refusal {
  /** Which refusal, exactly. Stable, and what a panel words in somebody's own language. */
  key: string;
  /** What the sentence needs: a name, a count, a limit. */
  named: Record<string, string>;
  /** The English, for anything with no wording of its own. */
  said: string;
}
";

fn declared(shape: &Shape) -> String {
    let mut out = String::new();

    out.push_str(&comment(shape.about, ""));

    match &shape.what {
        What::Every(of) => {
            let _ = writeln!(out, "export type {} = {of}[];", shape.named);
        }
        // No fields, and not `{}` — an object whose shape this software does
        // not decide is one a panel may read whatever it likes out of.
        What::Anything => {
            let _ = writeln!(
                out,
                "export type {} = Record<string, unknown>;",
                shape.named
            );
        }
        What::Fields(fields) => {
            let _ = writeln!(out, "export interface {} {{", shape.named);

            for field in fields {
                out.push_str(&comment(field.about, "  "));

                let _ = writeln!(
                    out,
                    "  {}{}: {};",
                    field.name,
                    if field.required { "" } else { "?" },
                    holds(field)
                );
            }

            out.push_str("}\n");
        }
    }

    out
}

fn holds(field: &crate::Field) -> String {
    let said = match &field.of {
        Of::One(is) => named(*is).to_owned(),
        Of::Many(is) => format!("{}[]", named(*is)),
        Of::Another(shape) => (*shape).to_owned(),
        Of::ManyOf(shape) => format!("{shape}[]"),
        Of::Whatever => "unknown".to_owned(),
        Of::OneOf(these) => these
            .iter()
            .map(|one| format!("\"{one}\""))
            .collect::<Vec<_>>()
            .join(" | "),
    };

    if field.null {
        format!("{said} | null")
    } else {
        said
    }
}

const fn named(is: Is) -> &'static str {
    match is {
        Is::Text | Is::Id | Is::Moment => "string",
        Is::Number => "number",
        Is::Bool => "boolean",
    }
}

/// Every operation, by the name a client calls it.
///
/// The path with its holes still in it, because filling them is the caller's
/// and a generator that filled them would have to decide how. What this stops
/// is a panel with an address typed into it — which is how a listing comes to
/// be pointed at the wrong endpoint and nothing says so.
fn operations(api: &Api) -> String {
    let mut out = String::from(
        "
export interface Operation {
  method: \"get\" | \"post\" | \"put\" | \"patch\" | \"delete\";
  /** With its `{holes}` still in it. Filling them is the caller's. */
  path: string;
  /** The shape it takes, where it takes one. */
  takes: string | null;
  /** The shape it answers with, where it answers with one. */
  answers: string | null;
  /** What it answers when nothing went wrong. */
  status: number;
}

/** Every operation this installation describes. */
export const operations = {
",
    );

    let mut endpoints: Vec<_> = api.endpoints.iter().collect();
    endpoints.sort_by_key(|endpoint| endpoint.named);

    for endpoint in endpoints {
        let _ = writeln!(
            out,
            "  \"{}\": {{ method: \"{}\", path: \"{}\", takes: {}, answers: {}, status: {} }},",
            endpoint.named,
            endpoint.method.lower(),
            endpoint.path,
            quoted(endpoint.takes),
            quoted(endpoint.answers.body()),
            endpoint.answers.status(),
        );
    }

    out.push_str("} as const;\n\nexport type Named = keyof typeof operations;\n");

    out
}

fn quoted(what: Option<&'static str>) -> String {
    what.map_or_else(|| "null".to_owned(), |what| format!("\"{what}\""))
}

/// A sentence, as a doc comment, wrapped so nothing is a line nobody reads.
fn comment(about: &str, indent: &str) -> String {
    if about.is_empty() {
        return String::new();
    }

    let mut lines = Vec::new();
    let mut line = String::new();

    for word in about.split_whitespace() {
        if !line.is_empty() && line.len() + word.len() + 1 > 76 - indent.len() {
            lines.push(std::mem::take(&mut line));
        }

        if !line.is_empty() {
            line.push(' ');
        }

        line.push_str(word);
    }

    if !line.is_empty() {
        lines.push(line);
    }

    if lines.len() == 1 {
        return format!("{indent}/** {} */\n", lines[0]);
    }

    let mut out = format!("{indent}/**\n");

    for line in lines {
        let _ = writeln!(out, "{indent} * {line}");
    }

    let _ = writeln!(out, "{indent} */");

    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{Answers, Endpoint, Field, Method, Parameter, Who};

    fn an_api() -> Api {
        Api::of(vec![Endpoint {
            method: Method::Patch,
            path: "/api/writings/{id}",
            named: "writings.change",
            about: "Changes one.",
            who: Who::AnAccount,
            parameters: vec![Parameter::path("id", Is::Id, "Which one.")],
            takes: Some("WritingChanges"),
            answers: Answers::With("Writing"),
            refuses: &[],
            changes: true,
        }])
        .and(vec![
            Shape::new(
                "Writing",
                "Something a site wrote.",
                vec![
                    Field::new("id", Of::One(Is::Id), "Which one."),
                    Field::new("excerpt", Of::One(Is::Text), "A line about it.").or_null(),
                    Field::new("fields", Of::Whatever, "Whatever a site added."),
                    Field::new(
                        "state",
                        Of::OneOf(&["draft", "published"]),
                        "Whether it is out.",
                    ),
                    Field::new("tags", Of::ManyOf("Term"), "What it is under.").maybe(),
                ],
            ),
            Shape::list_of("TermList", "Term", "All of them."),
            Shape::anything("Whatever", "Nobody decides this."),
        ])
    }

    #[test]
    fn a_shape_becomes_a_type_a_panel_can_hold_somebody_to() {
        let written = typescript(&an_api());

        assert!(written.contains("export interface Writing {"), "{written}");
        assert!(written.contains("  id: string;"), "{written}");
        // What may be nothing and what may be left out are different, and a
        // panel that cannot tell them apart crashes on one of them.
        assert!(written.contains("  excerpt: string | null;"), "{written}");
        assert!(written.contains("  tags?: Term[];"), "{written}");
        // What a site decided its own fields are is not invented here.
        assert!(written.contains("  fields: unknown;"), "{written}");
        assert!(
            written.contains("  state: \"draft\" | \"published\";"),
            "{written}"
        );
    }

    #[test]
    fn a_bare_list_and_an_object_nobody_decides_are_not_interfaces() {
        let written = typescript(&an_api());

        assert!(
            written.contains("export type TermList = Term[];"),
            "{written}"
        );
        assert!(
            written.contains("export type Whatever = Record<string, unknown>;"),
            "{written}"
        );
    }

    #[test]
    fn an_operation_keeps_the_holes_in_its_path() {
        // Filling them is the caller's. What this stops is a panel with an
        // address typed into it, which is how a listing comes to be pointed at
        // the wrong endpoint and nothing says so.
        let written = typescript(&an_api());

        assert!(
            written.contains(
                "\"writings.change\": { method: \"patch\", path: \"/api/writings/{id}\", \
                 takes: \"WritingChanges\", answers: \"Writing\", status: 200 },"
            ),
            "{written}"
        );
    }

    #[test]
    fn what_it_says_it_is_is_at_the_top() {
        assert!(typescript(&an_api()).starts_with("// Written from the description"));
    }
}
