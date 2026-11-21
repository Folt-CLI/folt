use std::ffi::OsStr;
use std::fs;
use std::path::Path;

use crate::config::names::TEMP_PREFIX_COPY;

use super::{Problem, REQUIRED, problem};

/// Records every entry that is neither required nor a copy temporary file.
pub(super) fn check_unexpected_entries(path: &Path, name: &OsStr, problems: &mut Vec<Problem>) {
    match fs::read_dir(path) {
        Ok(entries) => {
            for entry in entries {
                match entry {
                    Ok(entry)
                        if !REQUIRED.iter().any(|(name, _)| entry.file_name() == *name)
                            && !(entry.file_name().to_str().is_some_and(copy_temporary_name)
                                && entry.file_type().is_ok_and(|kind| kind.is_file())) =>
                    {
                        problems.push(Problem {
                            entry: entry.file_name(),
                            message: "unexpected entry".into(),
                        });
                    }
                    Ok(_) => {}
                    Err(error) => problems.push(problem(name, error)),
                }
            }
        }
        Err(error) => problems.push(problem(name, error)),
    }
}

fn copy_temporary_name(name: &str) -> bool {
    let Some(rest) = name.strip_prefix(TEMP_PREFIX_COPY) else {
        return false;
    };
    let Some((pid, counter)) = rest.split_once('-') else {
        return false;
    };
    !pid.is_empty()
        && !counter.is_empty()
        && pid.bytes().all(|byte| byte.is_ascii_digit())
        && counter.bytes().all(|byte| byte.is_ascii_digit())
}
