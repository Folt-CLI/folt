use std::fmt;

use crate::config::limits::SUMMARY_LARGEST;
use crate::placement_path::PlacementPath;

use super::plan::{
    NameWrite, RegisterEntry, RegisterNote, RegisterPlan, RegisterStatus, RegisterSummary,
};
use super::request::RegisterRequest;

pub(super) fn report(
    entries: Vec<RegisterEntry>,
    requests: Vec<RegisterRequest>,
    destinations: Vec<PlacementPath>,
    batch_total_bytes: u64,
) -> RegisterPlan {
    let mut summary = RegisterSummary::default();
    for entry in &entries {
        if entry.name_write != NameWrite::None {
            summary.files_to_write += 1;
        }
        match entry.status {
            RegisterStatus::New => summary.new += 1,
            RegisterStatus::AlreadyRegistered => summary.already_registered += 1,
            RegisterStatus::Refused(_) => summary.refused += 1,
            RegisterStatus::NeedsConfirmation { .. } => summary.needs_confirmation += 1,
        }
        if matches!(
            entry.status,
            RegisterStatus::New | RegisterStatus::NeedsConfirmation { .. }
        ) {
            summary.total_bytes = summary.total_bytes.saturating_add(entry.size);
            summary.largest.push((entry.path.clone(), entry.size));
        }
    }
    summary
        .largest
        .sort_by(|a, b| b.1.cmp(&a.1).then_with(|| a.0.cmp(&b.0)));
    summary.largest.truncate(SUMMARY_LARGEST);
    let notes = if summary.total_bytes > batch_total_bytes {
        vec![RegisterNote::NeedsConfirmation {
            size: summary.total_bytes,
        }]
    } else {
        vec![]
    };
    RegisterPlan {
        entries,
        notes,
        summary,
        requests,
        destinations,
    }
}

impl fmt::Display for RegisterPlan {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        for entry in &self.entries {
            if let RegisterStatus::Refused(reason) = &entry.status {
                writeln!(f, "{}: {reason}", entry.path.display())?;
            } else {
                writeln!(f, "{}: {:?}", entry.path.display(), entry.status)?;
            }
            if let Some(name) = &entry.name {
                writeln!(f, "  name: {name}")?;
                if entry.name_write != NameWrite::None {
                    writeln!(
                        f,
                        "will write folio-name: \"{name}\" into {}",
                        entry.path.display()
                    )?;
                }
            }
        }
        writeln!(f, "{} files to write", self.summary.files_to_write)
    }
}
