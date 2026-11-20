use crate::registry;

/// A placement has a stage when its staged field is set.
pub(crate) fn is_staged(row: &registry::Placement) -> bool {
    row.staged.is_some()
}

#[cfg(test)]
mod tests;
