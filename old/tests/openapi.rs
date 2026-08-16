//! The description of the API, written down. A change to what is served shows
//! up here before it shows up in somebody's client.

const SNAPSHOT: &str = "tests/snapshots/openapi.json";

// Written down for the build that carries everything. A build with something
// mounted on this crate serves more, and comparing that to this would say the
// description had changed when only the build had.
#[test]
fn the_description_is_what_it_was() {
    let now = mavi::openapi().to_pretty_json().expect("a description");

    if std::env::var("UPDATE_SNAPSHOTS").is_ok() {
        std::fs::create_dir_all("tests/snapshots").expect("a place to write");
        std::fs::write(SNAPSHOT, format!("{now}\n")).expect("write");
        return;
    }

    let before = std::fs::read_to_string(SNAPSHOT).unwrap_or_else(|_| {
        panic!("{SNAPSHOT} is missing; run the tests with UPDATE_SNAPSHOTS=1 to write it")
    });

    assert_eq!(
        before.trim(),
        now.trim(),
        "the API is not the one described. Read the difference, and if it is \
         what was meant, run with UPDATE_SNAPSHOTS=1."
    );
}

#[test]
fn everything_served_is_described() {
    let description = mavi::openapi();

    for endpoint in mavi::endpoints() {
        assert!(
            description.paths.paths.contains_key(endpoint.path()),
            "{} is served and not described",
            endpoint.path()
        );
    }
}
