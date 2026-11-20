use std::io;

use crate::config::settings::Settings;
use crate::folio_dir::Durability;
use crate::fsx::{FileSystem, RealFileSystem};
use crate::home::{Home, HomeLock};
use crate::ops::rollback::Rollback;
use crate::ops::step_hook::{NoHook, StepHook};
use crate::placement_path::RegistryError;

use super::error::RegisterError;
use super::plan::{RegisterPlan, RegisterStatus, block_changed, blocked, plan_register};

/// Replans under the exclusive home lock. Returns the plan actually applied,
/// or a blocked plan without writing. Does not create an initial history commit.
pub fn apply_register(
    home: &Home,
    plan: &RegisterPlan,
    confirm_large: bool,
) -> Result<RegisterPlan, RegisterError> {
    apply_steps(&RealFileSystem, home, plan, confirm_large, NoHook)
}

// Hook payloads are observed by the regression tests.
#[cfg_attr(not(test), allow(dead_code))]
pub(super) enum Step<'a> {
    Markers,
    Refreshed(&'a mut RegisterPlan),
    MarkerSynced,
    Folio(Durability),
    Transaction,
    Commit,
}

pub(super) fn apply_steps(
    fs: &dyn FileSystem,
    home: &Home,
    plan: &RegisterPlan,
    confirm_large: bool,
    hook: impl for<'a> StepHook<Step<'a>, RegistryError>,
) -> Result<RegisterPlan, RegisterError> {
    let lock = home.lock()?;
    apply_locked(fs, home, plan, confirm_large, &lock, hook)
}

pub(super) fn apply_locked(
    fs: &dyn FileSystem,
    home: &Home,
    plan: &RegisterPlan,
    confirm_large: bool,
    lock: &HomeLock,
    mut hook: impl for<'a> StepHook<Step<'a>, RegistryError>,
) -> Result<RegisterPlan, RegisterError> {
    let settings = Settings::load(home)?;
    let mut current = block_changed(
        plan,
        plan_register(home, plan.requests.clone())?,
        settings.batch_total_bytes,
    );
    if blocked(&current, confirm_large) {
        return Ok(current);
    }
    let mut state = Rollback::new(fs);
    let result: Result<bool, RegisterError> = (|| {
        markers::create_markers(fs, &current, &mut state, &mut hook)?;
        current = markers::refresh(home, plan, &current, &settings, &mut hook)?;
        if blocked(&current, confirm_large) {
            return Ok(false);
        }
        let mut rows = Vec::new();
        // Each index identifies a request in the refreshed plan. Display paths
        // may change spelling during replan, so never use them as identities.
        for (index, entry) in current.entries.clone().into_iter().enumerate() {
            if entry.status == RegisterStatus::AlreadyRegistered {
                continue;
            }
            let Some(content) = capture::capture_entry(
                fs,
                &mut current,
                index,
                &entry,
                &settings,
                confirm_large,
                &mut state,
            )?
            else {
                return Ok(false);
            };
            rows.push(publish::publish_entry(
                fs, home, lock, &entry, content, &mut state, &mut hook,
            )?);
        }
        publish::publish_rows(fs, home, lock, &rows, &mut hook)?;
        Ok(true)
    })();
    if matches!(result, Ok(true)) {
        // The empty file used to create an owned marker was only an
        // intermediate state, not an input snapshot for the returned plan.
        for entry in &mut current.entries {
            if state.owns_marker(&entry.path) {
                entry.signature = None;
            }
        }
        state.commit();
    } else {
        let failures = state.fail();
        if !failures.is_empty() {
            let cause = match &result {
                Err(error) => error.to_string(),
                _ => "registration blocked".into(),
            };
            return Err(io::Error::other(format!("{cause}; {}", failures.join("; "))).into());
        }
    }
    result.map(|_| current)
}

mod capture;
mod markers;
mod publish;

#[cfg(test)]
mod tests;
