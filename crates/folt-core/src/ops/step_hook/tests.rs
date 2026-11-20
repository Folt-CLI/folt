use super::{NoHook, StepHook};

#[test]
fn no_hook_accepts_different_payload_and_error_types() {
    let mut hook = NoHook;
    let unit: Result<(), String> = hook.on_step(());
    let owned: Result<(), std::io::Error> = hook.on_step(String::from("step"));
    let mut value = 42;
    let borrowed: Result<(), ()> = hook.on_step(("step", &mut value));
    assert_eq!(unit, Ok(()));
    assert!(owned.is_ok());
    assert_eq!(borrowed, Ok(()));
    assert_eq!(value, 42);
}

#[test]
fn closure_observes_steps_in_order() {
    let mut steps = Vec::new();
    let mut hook = |step| {
        steps.push(step);
        Ok::<(), ()>(())
    };
    hook.on_step(3).unwrap();
    hook.on_step(1).unwrap();
    hook.on_step(2).unwrap();
    assert_eq!(steps, [3, 1, 2]);
}

#[test]
fn closure_error_propagates() {
    let mut hook = |step| Err::<(), _>(step);
    assert_eq!(hook.on_step("step failed"), Err("step failed"));
}
