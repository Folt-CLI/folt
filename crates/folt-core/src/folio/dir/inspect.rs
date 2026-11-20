use std::ffi::{OsStr, OsString};
use std::fs;
use std::io;
use std::path::Path;

use crate::config::names::{BLOBS_DIR, CONTENT_FILE, FOLIOS_DIR, REFS_DIR, TEMP_PREFIX_COPY};
use crate::folio::format::{FormatError, check_format};
use crate::folio::refs_dir::inspect_refs;
use crate::folio::row::FolioRow;
use crate::id::FolioId;
use crate::layout::LayoutVersion;

use super::{FolioDir, FolioDirError};

/// A repair that can safely recreate a missing empty directory.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum Fix {
    CreateBlobs,
    CreateRefs,
}

/// An entry that prevents validation.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Problem {
    pub entry: OsString,
    pub message: String,
}

/// Classified result of inspecting an entry in `folios/`.
#[derive(Clone, Debug)]
pub enum Inspection {
    Valid(FolioDir),
    Repairable(FolioDir, Vec<Fix>),
    Newer {
        found: LayoutVersion,
        supported: LayoutVersion,
    },
    Unregistered(FolioDir, Vec<Fix>),
    Damaged(Vec<Problem>),
    NotAFolio,
}

/// Entries every Folio directory holds, with whether each is a directory.
const REQUIRED: [(&str, bool); 3] = [(CONTENT_FILE, false), (BLOBS_DIR, true), (REFS_DIR, true)];

/// Inspects a directory entry named by `name` under `folios/`.
pub fn inspect(home: &Path, name: &OsStr, row: Option<&FolioRow>) -> Inspection {
    let Some(text) = name.to_str() else {
        return Inspection::NotAFolio;
    };
    let Ok(id) = FolioId::parse(text) else {
        return Inspection::NotAFolio;
    };
    if let Some(row) = row
        && row.id != id
    {
        return Inspection::Damaged(vec![Problem {
            entry: name.to_os_string(),
            message: "registry row id does not match Folio directory name".into(),
        }]);
    }
    let path = home.join(FOLIOS_DIR).join(name);
    let mut problems = Vec::new();
    let mut fixes = Vec::new();
    let metadata = match fs::symlink_metadata(&path) {
        Ok(value) => value,
        Err(e) => return Inspection::Damaged(vec![problem(name, e)]),
    };
    if !metadata.file_type().is_dir() {
        return Inspection::NotAFolio;
    }
    check_required_entries(&path, &mut problems, &mut fixes);
    if let Err(error) = inspect_refs(&path) {
        problems.push(Problem {
            entry: REFS_DIR.into(),
            message: error.to_string(),
        });
    }
    check_unexpected_entries(&path, name, &mut problems);
    if let Some(row) = row
        && let Some(result) = check_row_format(row)
    {
        return result;
    }
    classify(FolioDir { id, path }, row.is_some(), problems, fixes)
}

/// Records wrong types and missing entries among the required entries.
fn check_required_entries(path: &Path, problems: &mut Vec<Problem>, fixes: &mut Vec<Fix>) {
    for (entry, directory) in REQUIRED {
        let child = path.join(entry);
        match fs::symlink_metadata(&child) {
            Ok(meta)
                if meta.file_type().is_symlink()
                    || (directory && !meta.is_dir())
                    || (!directory && !meta.is_file()) =>
            {
                problems.push(Problem {
                    entry: entry.into(),
                    message: "wrong entry type".into(),
                })
            }
            Ok(_) => {}
            Err(e) if e.kind() == io::ErrorKind::NotFound && entry == BLOBS_DIR => {
                fixes.push(Fix::CreateBlobs)
            }
            Err(e) if e.kind() == io::ErrorKind::NotFound && entry == REFS_DIR => {
                fixes.push(Fix::CreateRefs)
            }
            Err(e) => problems.push(problem(OsStr::new(entry), e)),
        }
    }
}

/// Records every entry that is neither required nor a copy temporary file.
fn check_unexpected_entries(path: &Path, name: &OsStr, problems: &mut Vec<Problem>) {
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

/// Returns a final inspection when the registry row's format blocks the Folio.
fn check_row_format(row: &FolioRow) -> Option<Inspection> {
    match check_format(&row.format) {
        Ok(()) => None,
        Err(FormatError::Newer { found, supported }) => {
            Some(Inspection::Newer { found, supported })
        }
        Err(error) => Some(Inspection::Damaged(vec![Problem {
            // This labels a format problem, not a file name.
            entry: "format".into(),
            message: error.to_string(),
        }])),
    }
}

/// Classifies the collected problems and fixes into the final inspection.
fn classify(
    directory: FolioDir,
    registered: bool,
    problems: Vec<Problem>,
    fixes: Vec<Fix>,
) -> Inspection {
    if !problems.is_empty() {
        Inspection::Damaged(problems)
    } else if registered {
        if !fixes.is_empty() {
            Inspection::Repairable(directory, fixes)
        } else {
            Inspection::Valid(directory)
        }
    } else {
        Inspection::Unregistered(directory, fixes)
    }
}

/// Validates a Folio, accepting only a complete directory.
pub fn validate(home: &Path, row: &FolioRow) -> Result<FolioDir, FolioDirError> {
    match inspect(home, OsStr::new(&row.id.to_string()), Some(row)) {
        Inspection::Valid(directory) => Ok(directory),
        Inspection::Newer { found, supported } => Err(FolioDirError::Newer { found, supported }),
        _ => Err(FolioDirError::InvalidDirectory(
            home.join(FOLIOS_DIR).join(row.id.to_string()),
        )),
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

fn problem(entry: &OsStr, error: io::Error) -> Problem {
    Problem {
        entry: entry.to_os_string(),
        message: error.to_string(),
    }
}
