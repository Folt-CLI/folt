use std::fs;
use std::io;

use crate::fsx::FileSystem;

use super::super::error::RegisterError;
use super::ApplyState;

pub(super) fn restore_apply(
    fs: &dyn FileSystem,
    mut state: ApplyState,
    result: &Result<bool, RegisterError>,
) -> Result<(), RegisterError> {
    let mut failures = Vec::new();
    restore_files(fs, &mut state, &mut failures);
    restore_artifacts(state, &mut failures);
    if !failures.is_empty() {
        let cause = match result {
            Err(error) => error.to_string(),
            _ => "registration blocked".into(),
        };
        return Err(io::Error::other(format!("{cause}; {}", failures.join("; "))).into());
    }
    Ok(())
}

fn restore_files(fs: &dyn FileSystem, state: &mut ApplyState, failures: &mut Vec<String>) {
    for (path, original, signature) in std::mem::take(&mut state.rewritten).into_iter().rev() {
        if !fs::symlink_metadata(&path)
            .is_ok_and(|meta| meta.is_file() && crate::fsx::signature(&meta) == signature)
        {
            failures.push(format!(
                "{} keeps the name: file changed after our write",
                path.display()
            ));
            continue;
        }
        let mut restoration_published = false;
        if let Err(error) =
            crate::fsx::atomic_rewrite(fs, &path, &original, &signature, |restored| {
                restoration_published = true;
                if let Some((_, marker_signature)) =
                    state.markers.iter_mut().find(|(marker, _)| marker == &path)
                {
                    *marker_signature = restored;
                }
            })
        {
            if restoration_published {
                failures.push(format!(
                        "{}: restoration completed but the directory sync failed; original bytes are back, durability of the restore is not confirmed: {error}",
                        path.display()
                    ));
            } else {
                failures.push(format!(
                    "{} keeps the name: restore failed: {error}",
                    path.display()
                ));
            }
        }
    }
}

fn restore_artifacts(state: ApplyState, failures: &mut Vec<String>) {
    for directory in state.directories {
        if let Err(error) = crate::fsx::remove_dir_all(&directory) {
            failures.push(format!("{}: {error}", directory.display()));
        }
    }
    for (marker, signature) in state.markers {
        if fs::symlink_metadata(&marker)
            .is_ok_and(|meta| meta.is_file() && crate::fsx::signature(&meta) == signature)
            && let Err(error) = crate::fsx::remove_file(&marker)
        {
            failures.push(format!("{}: {error}", marker.display()));
        }
    }
}
