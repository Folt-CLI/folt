use crate::id::ObjectId;

use super::RefError;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Expect {
    Absent,
    Is(ObjectId),
    Any,
}
impl Expect {
    pub(super) fn check(self, actual: Option<ObjectId>) -> Result<(), RefError> {
        if match self {
            Self::Absent => actual.is_none(),
            Self::Is(old) => actual == Some(old),
            Self::Any => true,
        } {
            Ok(())
        } else {
            Err(RefError::Stale { actual })
        }
    }
}
