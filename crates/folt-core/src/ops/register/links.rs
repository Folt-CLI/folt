use std::error::Error;
use std::io;
use std::path::{Path, PathBuf};

use crate::config::settings::Settings;
use crate::fsx::{FileSystem, RealFileSystem};
use crate::home::{Home, HomeLock};
use crate::ops::step_hook::{NoHook, StepHook};
use crate::placement::{Observation, PlacementKind};
use crate::placement_fs::PlacementFs;
use crate::placement_path::PlacementPath;
use crate::registry::Registry;

use super::apply::apply_locked;
use super::error::RegisterError;
use super::plan::{RegisterPlan, block_changed, blocked, plan_register};
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum RegisterLinkResult {
    Linked,
    LeftAsCopy { reason: String },
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct RegisterLinkOutcome {
    pub registration: RegisterPlan,
    pub paths: Vec<(PathBuf, RegisterLinkResult)>,
}

/// Registers the batch before converting each placement independently. The home
/// lock covers probing, registration and conversion; conversion errors keep rows.
pub(crate) fn apply_register_links(
    home: &Home,
    plan: &RegisterPlan,
    confirm_large: bool,
) -> Result<RegisterLinkOutcome, RegisterError> {
    apply_links_steps(
        &RealFileSystem,
        home,
        plan,
        confirm_large,
        |step: LinkStep<'_>| {
            if let LinkStep::Probe(path) = step {
                PlacementFs::new(home).probe_link(path)?;
            }
            Ok(())
        },
    )
}

#[cfg_attr(not(test), allow(dead_code))]
pub(super) enum LinkStep<'a> {
    Probe(&'a Path),
    Registered,
    Convert(&'a Path),
}

pub(super) fn apply_links_steps(
    fs: &dyn FileSystem,
    home: &Home,
    plan: &RegisterPlan,
    confirm_large: bool,
    mut hook: impl for<'a> StepHook<LinkStep<'a>, RegisterError>,
) -> Result<RegisterLinkOutcome, RegisterError> {
    let lock = home.lock()?;
    let settings = Settings::load(home)?;
    let current = block_changed(
        plan,
        plan_register(home, plan.requests.clone())?,
        settings.batch_total_bytes,
    );
    if blocked(&current, confirm_large) {
        return Ok(RegisterLinkOutcome {
            registration: current,
            paths: Vec::new(),
        });
    }
    for entry in &current.entries {
        hook.on_step(LinkStep::Probe(&entry.path))?;
    }
    let registration = apply_locked(fs, home, &current, confirm_large, &lock, NoHook)?;
    if blocked(&registration, confirm_large) {
        return Ok(RegisterLinkOutcome {
            registration,
            paths: Vec::new(),
        });
    }
    hook.on_step(LinkStep::Registered)?;
    let paths = registration
        .entries
        .iter()
        .map(|entry| {
            let result = hook
                .on_step(LinkStep::Convert(&entry.path))
                .map_err(|error| error.to_string())
                .and_then(|()| {
                    register_link(fs, home, &entry.path, &lock).map_err(|error| error.to_string())
                });
            let result = match result {
                Ok(()) => RegisterLinkResult::Linked,
                Err(reason) => RegisterLinkResult::LeftAsCopy { reason },
            };
            (entry.path.clone(), result)
        })
        .collect();
    Ok(RegisterLinkOutcome {
        registration,
        paths,
    })
}

fn register_link(
    fs: &dyn FileSystem,
    home: &Home,
    path: &Path,
    lock: &HomeLock,
) -> Result<(), Box<dyn Error>> {
    use crate::ops::facts::gather_convert;
    use crate::ops::placement::convert::{apply_convert, judge_convert};
    let mut registry = Registry::open_or_create(home, lock)?;
    let row = registry
        .placement_at(&PlacementPath::resolve(path)?)?
        .ok_or_else(|| io::Error::other("placement is missing; run repair (#38)"))?;
    let folio = registry
        .folio_row(row.id)?
        .ok_or_else(|| io::Error::other("Folio is missing"))?;
    let directory = crate::folio::dir::validate(home.path(), &folio)?;
    let objects = crate::store::ObjectStore::open(&directory, &folio.format)?;
    let refs = crate::history::RefStore::open(&directory, &folio.format)?;
    let placement_fs = PlacementFs::with_fs(home, fs);
    let facts = gather_convert(&row, &placement_fs, |_| Ok(&refs))?;
    if row.kind == PlacementKind::Link && facts.observation == Observation::IntactLink {
        return Ok(());
    }
    let plan = judge_convert(&facts, PlacementKind::Link, None, None)?;
    apply_convert(&plan, &placement_fs, &objects, &mut registry, lock)?;
    Ok(())
}

#[cfg(test)]
mod tests;
