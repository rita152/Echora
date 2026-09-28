//! Long goal objectives, as the reference stores them. An objective longer
//! than 4000 characters (code points, after trimming) is written to
//! `$CODEX_HOME/attachments/<uuid>/goal-objective.md`, and the goal carries a
//! one-line pointer to that file instead. The Edit goal tab reads the file
//! back only when the pointer names such a file under the Codex home.

use std::{
    hash::{BuildHasher, Hasher},
    io,
    path::{Path, PathBuf},
};

/// The reference's limit, in code points of the trimmed objective.
pub const GOAL_OBJECTIVE_LIMIT: usize = 4000;
const FILE_NAME: &str = "goal-objective.md";
const PREFIX: &str = "Read the Codex goal objective file at ";
const SUFFIX: &str = " before continuing.";

/// `$CODEX_HOME`, or `~/.codex` when it is unset.
pub fn codex_home() -> Option<PathBuf> {
    std::env::var_os("CODEX_HOME")
        .map(PathBuf::from)
        .or_else(|| std::env::var_os("HOME").map(|home| PathBuf::from(home).join(".codex")))
}

/// A random version-4 UUID for the attachment directory.
fn uuid_v4() -> String {
    let random = || {
        std::collections::hash_map::RandomState::new()
            .build_hasher()
            .finish()
    };
    let (high, low) = (random(), random());
    let high = (high & !0xf000) | 0x4000;
    let low = (low & !(0xc000 << 48)) | (0x8000 << 48);
    format!(
        "{:08x}-{:04x}-{:04x}-{:04x}-{:012x}",
        high >> 32,
        (high >> 16) & 0xffff,
        high & 0xffff,
        low >> 48,
        low & 0xffff_ffff_ffff
    )
}

/// The objective to send: the trimmed text itself, or, above the limit, a
/// pointer to the file it was written to.
pub fn prepare_objective(objective: &str, codex_home: &Path) -> io::Result<String> {
    let objective = objective.trim();
    if objective.chars().count() <= GOAL_OBJECTIVE_LIMIT {
        return Ok(objective.to_owned());
    }
    let directory = codex_home.join("attachments").join(uuid_v4());
    std::fs::create_dir_all(&directory)?;
    let path = directory.join(FILE_NAME);
    std::fs::write(&path, objective)?;
    Ok(format!("{PREFIX}{}{SUFFIX}", path.display()))
}

/// The file a pointer objective names, if it is the reference's exact
/// sentence and the file sits in an attachment directory of `codex_home`.
pub fn objective_file(objective: &str, codex_home: &Path) -> Option<PathBuf> {
    let path = PathBuf::from(objective.strip_prefix(PREFIX)?.strip_suffix(SUFFIX)?);
    let directory = path.parent()?;
    (path.file_name()? == FILE_NAME
        && directory.parent()? == codex_home.join("attachments")
        && directory.file_name().is_some())
    .then_some(path)
}

/// The objective as the user wrote it: the file's text for a pointer, the
/// objective itself otherwise.
pub fn load_objective(objective: &str, codex_home: &Path) -> io::Result<String> {
    match objective_file(objective, codex_home) {
        Some(path) => std::fs::read_to_string(path),
        None => Ok(objective.to_owned()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_long_objective_is_written_to_an_attachment_and_read_back() {
        let home = std::env::temp_dir().join(format!("gpui-goal-objective-{}", std::process::id()));
        let short = format!("  {}  ", "a".repeat(GOAL_OBJECTIVE_LIMIT));
        assert_eq!(prepare_objective(&short, &home).unwrap(), short.trim());
        // Code points, not bytes: 4000 CJK characters still fit.
        let wide = "目".repeat(GOAL_OBJECTIVE_LIMIT);
        assert_eq!(prepare_objective(&wide, &home).unwrap(), wide);
        let long = "b".repeat(GOAL_OBJECTIVE_LIMIT + 1);
        let pointer = prepare_objective(&long, &home).unwrap();
        assert!(pointer.starts_with("Read the Codex goal objective file at "));
        assert!(pointer.ends_with("/goal-objective.md before continuing."));
        let path = objective_file(&pointer, &home).expect("a pointer under the home");
        assert!(path.starts_with(home.join("attachments")));
        assert_eq!(load_objective(&pointer, &home).unwrap(), long);
        assert_eq!(load_objective("plain", &home).unwrap(), "plain");
        std::fs::remove_dir_all(&home).unwrap();
    }

    #[test]
    fn only_the_exact_sentence_under_the_home_is_a_pointer() {
        let home = Path::new("/home/codex");
        let inside = "Read the Codex goal objective file at /home/codex/attachments/x/goal-objective.md before continuing.";
        assert!(objective_file(inside, home).is_some());
        for outside in [
            "Read the Codex goal objective file at /etc/attachments/x/goal-objective.md before continuing.",
            "Read the Codex goal objective file at /home/codex/attachments/goal-objective.md before continuing.",
            "Read the Codex goal objective file at /home/codex/attachments/x/other.md before continuing.",
            "Please read the Codex goal objective file at /home/codex/attachments/x/goal-objective.md before continuing.",
        ] {
            assert!(objective_file(outside, home).is_none(), "{outside}");
        }
        let id = uuid_v4();
        assert_eq!(id.len(), 36);
        assert_eq!(&id[14..15], "4");
        assert!(matches!(&id[19..20], "8" | "9" | "a" | "b"));
    }
}
