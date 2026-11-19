//! Validates shared object fields and extracts header values; body parsing lives elsewhere.

use super::ObjectError;

pub(super) fn field<'a>(line: Option<&'a str>, prefix: &str) -> Result<&'a str, ObjectError> {
    line.and_then(|line| line.strip_prefix(prefix))
        .ok_or(ObjectError::InvalidObject)
}

pub(super) fn validate_message(value: &str) -> Result<(), ObjectError> {
    validate_value(value)?;
    if value.is_empty() {
        return Err(ObjectError::InvalidObject);
    }
    Ok(())
}

pub(super) fn validate_value(value: &str) -> Result<(), ObjectError> {
    if value.contains('\n') || value.contains('\r') || value.as_bytes().contains(&0) {
        Err(ObjectError::InvalidObject)
    } else {
        Ok(())
    }
}
