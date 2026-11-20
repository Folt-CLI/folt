use std::io;

use crate::ops::register::RegisterRefusalReason;

use super::io_refusal;

#[test]
fn permission_error_maps_to_unreadable() {
    assert_eq!(
        io_refusal(&io::Error::from(io::ErrorKind::PermissionDenied)),
        RegisterRefusalReason::Unreadable
    );
}
