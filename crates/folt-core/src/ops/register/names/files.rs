use std::collections::HashMap;
use std::fs::File;
use std::io::Read;

use crate::config::limits::SCAN_WINDOW;
use crate::folio::name::FolioName;
use crate::front_matter;

use super::super::plan::{NameSource, RegisterEntry, RegisterStatus};
use super::super::refusal::{RegisterRefusalReason, io_refusal, refuse_name};
use super::super::request::RegisterRequest;

pub(super) fn names_from_files(
    entries: &mut [RegisterEntry],
    requests: &[RegisterRequest],
) -> HashMap<usize, Vec<u8>> {
    let mut prefixes = HashMap::new();
    for (index, (entry, request)) in entries.iter_mut().zip(requests).enumerate() {
        if entry.status == RegisterStatus::AlreadyRegistered && request.name.is_some() {
            refuse_name(entry, RegisterRefusalReason::NameOnRegisteredPath);
        }
        if !matches!(
            entry.status,
            RegisterStatus::New | RegisterStatus::NeedsConfirmation { .. }
        ) {
            continue;
        }
        let existing = if front_matter::applies_to(&entry.path) {
            let mut bytes = Vec::new();
            if !entry.creates_marker {
                let read = File::open(&entry.path)
                    .and_then(|file| file.take(SCAN_WINDOW as u64).read_to_end(&mut bytes));
                if let Err(error) = read {
                    refuse_name(entry, io_refusal(&error));
                    continue;
                }
            }
            let name = match front_matter::read_name(&bytes) {
                Ok(name) => name.map(|name| name.into_owned()),
                Err(error) => {
                    refuse_name(entry, RegisterRefusalReason::FrontMatter(error));
                    continue;
                }
            };
            entry.prefix = Some(bytes.clone());
            prefixes.insert(index, bytes);
            name
        } else {
            None
        };
        if let Some(value) = existing {
            let name = match FolioName::parse(&value) {
                Ok(name) => name,
                Err(error) => {
                    refuse_name(entry, RegisterRefusalReason::InvalidFrontMatterName(error));
                    continue;
                }
            };
            if request.name.is_some() {
                refuse_name(entry, RegisterRefusalReason::NameOptionWithFrontMatter);
                continue;
            }
            entry.name = Some(name);
            entry.name_source = Some(NameSource::FrontMatter);
        } else if let Some(name) = &request.name {
            entry.name = Some(name.clone());
            entry.name_source = Some(NameSource::Option);
        }
    }
    prefixes
}
