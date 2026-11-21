use crate::home::Home;
use crate::ops::register::{
    RegisterError, RegisterLinkOutcome, RegisterPlan, RegisterRequest, apply_register,
    apply_register_links, plan_register,
};

impl Home {
    /// Forwards to [`crate::ops::register::plan_register`] with this home, which owns the rules.
    pub fn plan_register(
        &self,
        paths: impl IntoIterator<Item = impl Into<RegisterRequest>>,
    ) -> Result<RegisterPlan, RegisterError> {
        plan_register(self, paths)
    }

    /// Forwards to [`crate::ops::register::apply_register`] with this home, which owns the rules.
    pub fn apply_register(
        &self,
        plan: &RegisterPlan,
        confirm_large: bool,
    ) -> Result<RegisterPlan, RegisterError> {
        apply_register(self, plan, confirm_large)
    }

    /// Forwards to [`crate::ops::register::apply_register_links`] with this home, which owns the rules.
    pub fn apply_register_links(
        &self,
        plan: &RegisterPlan,
        confirm_large: bool,
    ) -> Result<RegisterLinkOutcome, RegisterError> {
        apply_register_links(self, plan, confirm_large)
    }
}
