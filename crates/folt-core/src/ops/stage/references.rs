use crate::placement::PlacementKind;
use crate::registry::Placement;

/// Inspect exactly the prefix staging uses, including internal Markdown detection.
pub(crate) fn stage_references<'a>(
    selected: &Placement,
    prefix: &'a [u8],
) -> Result<Vec<crate::front_matter::Reference<'a>>, crate::front_matter::FrontMatterError> {
    let prefix = crate::front_matter::scan_window(prefix);
    let markdown = crate::front_matter::applies_to(&selected.path.to_path_buf());
    // Internal working files have no Markdown extension. Only their own named
    // front matter enables detection; copies and links use the selected path.
    let markdown = if !markdown && selected.kind == PlacementKind::Internal {
        match crate::front_matter::read_name(prefix) {
            Ok(name) => name.is_some(),
            Err(crate::front_matter::FrontMatterError::Binary) => false,
            Err(error) => return Err(error),
        }
    } else {
        markdown
    };
    if !markdown {
        return Ok(Vec::new());
    }
    match crate::front_matter::read_references(prefix) {
        Ok(refs) => Ok(refs),
        Err(crate::front_matter::FrontMatterError::Binary) => Ok(Vec::new()),
        Err(error) => Err(error),
    }
}
