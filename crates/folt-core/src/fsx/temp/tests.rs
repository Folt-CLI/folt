use super::*;

#[test]
fn names_have_the_requested_shape_and_are_unique() {
    let first = temp_name(".ref.", '.');
    let second = temp_name(".ref.", '.');
    let prefix = format!(".ref.{}.", std::process::id());
    assert!(first.strip_prefix(&prefix).unwrap().parse::<u64>().is_ok());
    assert!(second.strip_prefix(&prefix).unwrap().parse::<u64>().is_ok());
    assert_ne!(first, second);
}
