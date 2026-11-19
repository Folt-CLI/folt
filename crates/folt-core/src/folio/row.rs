use crate::folio::format::FolioFormat;
use crate::id::FolioId;

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct NewFolio {
    pub id: FolioId,
    pub format: FolioFormat,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct FolioRow {
    pub id: FolioId,
    pub format: FolioFormat,
}
