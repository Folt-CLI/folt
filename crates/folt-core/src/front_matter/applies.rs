use std::path::Path;

use crate::config::names::MARKDOWN_EXTENSIONS;

/// Whether the path has a supported Markdown extension.
pub fn applies_to(path: &Path) -> bool {
    path.extension()
        .and_then(|ext| ext.to_str())
        .is_some_and(|ext| {
            MARKDOWN_EXTENSIONS
                .iter()
                .any(|extension| ext.eq_ignore_ascii_case(extension))
        })
}
