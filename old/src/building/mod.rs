//! Running the thing that turns a site's files into its pages.
//!
//! The old machine ran every build as one user, in folders every other build
//! could read and write, with no limit in the code on how many ran at once —
//! so one customer's `postinstall` could read every other customer's source and
//! change their live site, and three builds at once took the machine off the
//! air. Both of those are decisions this file makes rather than a manifest.

use std::path::{Path, PathBuf};
use std::process::Stdio;
use std::time::Duration;

use crate::kernel::error::{AppError, Result};

/// What is run and where, which is configuration rather than anything this
/// module decides — so it is read where the rest of an installation's outside
/// programs are, and used here.
pub use crate::kernel::builder::Generator;

/// How long a build may take before it is killed.
///
/// Killed, not dropped: a child that is only dropped keeps its cores, and a
/// `bun` that has stopped making progress will hold them until somebody
/// notices — which on the old machine was hours.
const AT_MOST: Duration = Duration::from_mins(20);

/// What a build left behind: what it said, and the files it produced.
#[derive(Debug)]
pub struct Made {
    pub log: String,
    pub files: Vec<(String, Vec<u8>)>,
}

/// Runs the generator over one site's files.
///
/// The workspace is this build's alone: a fresh directory named for the site
/// and the publish, made by this process, readable by nobody else, and removed
/// afterwards however it went. Two customers' builds cannot see each other,
/// because neither is ever inside a directory the other has a path to.
pub async fn run(
    generator: &Generator,
    publish: uuid::Uuid,
    files: Vec<(String, String)>,
) -> Result<Made> {
    // Held for the whole build, so "one at a time" is a fact about the machine
    // rather than a hope about the manifest.
    let _turn = generator
        .at_once
        .acquire()
        .await
        .map_err(|_| AppError::Bug("nothing is taking builds any more"))?;

    let workspace = generator.workspaces.join(publish.simple().to_string());

    // Refused rather than reused: a workspace that is already there is one
    // another build is in, and writing into it is how a build reads the half
    // of another that has not finished.
    if tokio::fs::try_exists(&workspace).await.unwrap_or(false) {
        return Err(AppError::Bug("that workspace is already somebody's"));
    }

    let outcome = build_in(&workspace, generator, files).await;

    // However it went: a workspace left behind is a customer's source left on
    // a shared disk.
    let _ = tokio::fs::remove_dir_all(&workspace).await;

    outcome
}

async fn build_in(
    workspace: &Path,
    generator: &Generator,
    files: Vec<(String, String)>,
) -> Result<Made> {
    make_ours(workspace).await?;

    for (path, body) in files {
        let at = within(workspace, &path)?;

        if let Some(parent) = at.parent() {
            tokio::fs::create_dir_all(parent)
                .await
                .map_err(|_| AppError::Bug("somewhere to put a site's files"))?;
        }

        tokio::fs::write(&at, body)
            .await
            .map_err(|_| AppError::Bug("writing a site's files"))?;
    }

    let child = tokio::process::Command::new(&generator.program)
        .args(&generator.arguments)
        .current_dir(workspace)
        // Nothing of this process's environment: it holds the database
        // password, the keyring and every provider's key, and what runs here is
        // a command a customer can change.
        .env_clear()
        .env("PATH", std::env::var("PATH").unwrap_or_default())
        .env("HOME", workspace)
        .env("CI", "1")
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        // Killed when this future is dropped, so a cancelled build does not
        // leave a compiler running.
        .kill_on_drop(true)
        .spawn()
        .map_err(|_| AppError::Bug("that generator could not be started"))?;

    let finished = tokio::time::timeout(AT_MOST, child.wait_with_output()).await;

    let output = match finished {
        Ok(Ok(output)) => output,
        Ok(Err(_)) => return Err(AppError::Bug("that generator could not be waited for")),
        Err(_) => {
            return Ok(Made {
                log: format!("the generator was still going after {AT_MOST:?} and was stopped"),
                files: Vec::new(),
            });
        }
    };

    let mut log = String::from_utf8_lossy(&output.stdout).into_owned();
    log.push_str(&String::from_utf8_lossy(&output.stderr));

    // What a person reads when it did not work, and not a megabyte of it.
    let log: String = log.chars().take(20_000).collect();

    if !output.status.success() {
        return Ok(Made {
            log,
            files: Vec::new(),
        });
    }

    let made = gather(&workspace.join(&generator.output)).await?;

    Ok(Made { log, files: made })
}

/// A directory this build owns and nobody else can read.
async fn make_ours(workspace: &Path) -> Result<()> {
    tokio::fs::create_dir_all(workspace)
        .await
        .map_err(|_| AppError::Bug("somewhere to build"))?;

    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt as _;

        // Nobody else, not even the group: the old machine's checkout roots
        // were group-writable and every build ran in the same group.
        tokio::fs::set_permissions(workspace, std::fs::Permissions::from_mode(0o700))
            .await
            .map_err(|_| AppError::Bug("a workspace only this build can read"))?;
    }

    Ok(())
}

/// A path inside the workspace, whatever was asked for.
fn within(workspace: &Path, path: &str) -> Result<PathBuf> {
    let safe = !path.is_empty()
        && path
            .split('/')
            .all(|part| !part.is_empty() && part != "." && part != ".." && !part.contains('\\'));

    if !safe {
        return Err(AppError::Invalid(crate::kernel::say::NOT_PLACE.into()));
    }

    Ok(workspace.join(path))
}

/// Everything a build produced, by name.
async fn gather(from: &Path) -> Result<Vec<(String, Vec<u8>)>> {
    let mut found = Vec::new();
    let mut looking = vec![from.to_path_buf()];

    while let Some(here) = looking.pop() {
        // Nothing produced is a build that made nothing, which is said by the
        // empty list rather than by an error.
        let Ok(mut entries) = tokio::fs::read_dir(&here).await else {
            continue;
        };

        while let Ok(Some(entry)) = entries.next_entry().await {
            let path = entry.path();

            if path.is_dir() {
                looking.push(path);
                continue;
            }

            let Ok(name) = path.strip_prefix(from) else {
                continue;
            };

            let Some(name) = name.to_str().map(str::to_owned) else {
                continue;
            };

            let bytes = tokio::fs::read(&path)
                .await
                .map_err(|_| AppError::Bug("reading what a build produced"))?;

            found.push((name, bytes));
        }
    }

    Ok(found)
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use tokio::sync::Semaphore;

    use super::*;

    fn somewhere() -> PathBuf {
        std::env::temp_dir().join(format!("mavi-build-{}", uuid::Uuid::now_v7().simple()))
    }

    fn a_generator(program: &str, arguments: &[&str], at: PathBuf) -> Generator {
        Generator {
            program: program.to_owned(),
            arguments: arguments.iter().map(|word| (*word).to_owned()).collect(),
            output: "dist".to_owned(),
            workspaces: at,
            at_once: Arc::new(Semaphore::new(1)),
        }
    }

    #[tokio::test]
    async fn what_a_generator_produced_is_what_comes_back() {
        let at = somewhere();

        // Something every machine has, doing what a generator does: reading the
        // site's files and writing pages.
        let generator = a_generator(
            "sh",
            &["-c", "mkdir -p dist && cp src/page.html dist/index.html"],
            at.clone(),
        );

        let made = run(
            &generator,
            uuid::Uuid::now_v7(),
            vec![("src/page.html".to_owned(), "<h1>A page</h1>".to_owned())],
        )
        .await
        .expect("a build");

        assert_eq!(made.files.len(), 1);
        assert_eq!(made.files[0].0, "index.html");
        assert_eq!(made.files[0].1, b"<h1>A page</h1>");

        let _ = tokio::fs::remove_dir_all(&at).await;
    }

    #[tokio::test]
    async fn a_workspace_does_not_outlive_the_build() {
        let at = somewhere();
        let generator = a_generator("sh", &["-c", "mkdir -p dist"], at.clone());
        let publish = uuid::Uuid::now_v7();

        run(&generator, publish, Vec::new()).await.expect("a build");

        let left = at.join(publish.simple().to_string());

        assert!(
            !tokio::fs::try_exists(&left).await.unwrap_or(false),
            "the source was left on the disk after the build"
        );

        let _ = tokio::fs::remove_dir_all(&at).await;
    }

    #[tokio::test]
    async fn a_generator_that_fails_says_so_and_produces_nothing() {
        let at = somewhere();
        let generator = a_generator("sh", &["-c", "echo it went wrong >&2; exit 1"], at.clone());

        let made = run(&generator, uuid::Uuid::now_v7(), Vec::new())
            .await
            .expect("an answer");

        assert!(made.files.is_empty());
        assert!(made.log.contains("it went wrong"), "{}", made.log);

        let _ = tokio::fs::remove_dir_all(&at).await;
    }

    #[tokio::test]
    async fn a_site_s_file_cannot_be_written_outside_its_workspace() {
        let at = somewhere();

        assert!(within(&at, "../../etc/passwd").is_err());
        assert!(within(&at, "/etc/passwd").is_err());
        assert!(within(&at, "src/page.html").is_ok());
    }

    #[tokio::test]
    async fn what_this_process_holds_is_not_in_the_build_s_environment() {
        let at = somewhere();

        // Whatever runs this has a pile of `CARGO_*` in its environment, and a
        // deployed one has `DATABASE_URL` and the keyring. None of it belongs
        // to a command a customer can change.
        assert!(
            std::env::var("CARGO").is_ok(),
            "this test is about what a build inherits, and it inherits nothing here"
        );

        let generator = a_generator(
            "sh",
            &["-c", "mkdir -p dist && env > dist/seen.txt"],
            at.clone(),
        );

        let made = run(&generator, uuid::Uuid::now_v7(), Vec::new())
            .await
            .expect("a build");

        let seen = String::from_utf8_lossy(&made.files[0].1).into_owned();

        assert!(
            !seen.contains("CARGO"),
            "a build was handed this process's environment: {seen}"
        );

        let _ = tokio::fs::remove_dir_all(&at).await;
    }
}
