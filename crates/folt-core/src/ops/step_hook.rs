pub(crate) trait StepHook<S, E> {
    fn on_step(&mut self, step: S) -> Result<(), E>;
}

pub(crate) struct NoHook;

impl<S, E> StepHook<S, E> for NoHook {
    fn on_step(&mut self, _step: S) -> Result<(), E> {
        Ok(())
    }
}

impl<S, E, F: FnMut(S) -> Result<(), E>> StepHook<S, E> for F {
    fn on_step(&mut self, step: S) -> Result<(), E> {
        self(step)
    }
}

#[cfg(test)]
mod tests;
