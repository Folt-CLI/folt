use super::super::error::Result;
use super::super::reference::flow::mapping;
use super::super::reference::{Reference, ReferenceNode};
use super::super::scan::punctuation;
use super::super::tree::Node;
use super::references::render_reference;
use super::scalar::quoted;
use super::{Edit, checked_edit, edit};

// A single entry inside a flow collection cannot contain a block mapping.
fn render_flow_reference(r: &Reference<'_>) -> Result<String> {
    render_reference(r, "\n")?;
    let mut fields = Vec::new();
    if let Some(name) = &r.name {
        fields.push(format!("name: {}", quoted(name)?));
    }
    if let Some(id) = r.id {
        fields.push(format!("id: \"{id}\""));
    }
    if let Some(pin) = &r.pin {
        fields.push(format!("pin: {}", quoted(pin)?));
    }
    Ok(format!("{}: {{{}}}", r.alias, fields.join(", ")))
}

pub(super) fn set_flow_reference(
    bytes: &[u8],
    n: &Node,
    entries: &[ReferenceNode<'_>],
    selected: Option<usize>,
    value: Option<&Reference<'_>>,
    collection: &str,
) -> Result<Option<Edit>> {
    // Flow collections must stay flow collections. Only the selected
    // key/value and, on removal, one separator belong to the edit.
    let parts = mapping(collection)?;
    if let Some(index) = selected {
        let start = entries[index].key_range.start;
        let end = entries[index].value.end;
        if let Some(value) = value {
            return checked_edit(bytes, edit(start..end, render_flow_reference(value)?));
        }
        let commas: Vec<_> = punctuation(&collection[1..collection.len() - 1], ',')
            .into_iter()
            .map(|n| n + 1)
            .collect();
        let (range, replacement) = if index < commas.len() {
            let comma = n.value.start + commas[index];
            (start..comma + 1, bytes[end..comma].to_vec())
        } else if index > 0 {
            let comma = n.value.start + commas[index - 1];
            (comma..end, bytes[comma + 1..start].to_vec())
        } else {
            (start..end, Vec::new())
        };
        return checked_edit(bytes, edit(range, replacement));
    }
    if let Some(value) = value {
        let position = n.value.end - 1;
        let has_separator = parts.last().is_some_and(|(_, old)| {
            let end = old.as_ptr() as usize - collection.as_ptr() as usize + old.len();
            !punctuation(&collection[end..collection.len() - 1], ',').is_empty()
        });
        let separator = if entries.is_empty() || has_separator {
            ""
        } else {
            ", "
        };
        return checked_edit(
            bytes,
            edit(
                position..position,
                format!("{separator}{}", render_flow_reference(value)?),
            ),
        );
    }
    Ok(None)
}
