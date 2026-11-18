# Front matter in folt-core

Terms are defined in [GLOSSARY.md](../../GLOSSARY.md).

`front_matter` implements a bounded YAML subset. Foreign `Node` values stay opaque; edits are byte-span replacements valid only for the bytes they were planned on. Reading is more permissive than editing.

Scenario numbers below follow the implementation scope: scenario 1 introduces the numbered rules; scenarios 2 to 36 describe the grammar and edits. Function names refer to [front_matter.rs](../../crates/folt-core/src/front_matter.rs); test names are qualified by their module under [unit tests](../../crates/folt-core/src/front_matter/tests/) or [validation tests](../../tests/validation/). This guide covers the parser, not [stage](../../GLOSSARY.md#stage) or [register](../../GLOSSARY.md#register).

## Block location

**Scenario 2 - No block.** `locate` returns `Ok(None)` when the first line, after an optional UTF-8 BOM, is not `---` after trimming trailing spaces and tabs. Leading spaces, a vertical tab after `---`, and lone carriage return line endings do not open a `Block`. Test: `document::enforces_scan_boundary_and_delimiters`.

**Scenario 3 - Delimiters.** The opening line accepts trailing spaces and tabs. A closing line must contain exactly `---` or `...`, without trailing whitespace; `locate` compares the entire line content. An indented `  ---`, including one in a foreign block scalar, does not close the `Block`. Delimiter recognition precedes tokenization and does not track YAML scalar context. Tests: `document::enforces_scan_boundary_and_delimiters`, `document::preserves_bytes_and_owned_line_trivia`.

**Scenario 4 - Missing close.** An opening line without a closing line gives `Unterminated`, including input consisting only of the opening line. Creating a second `Block` is not a recovery mechanism. Functions: `locate`, `insert`. Tests: `document::enforces_scan_boundary_and_delimiters`, validation `front_matter::unterminated_note_cannot_receive_a_second_block`.

**Scenario 5 - Scan limit.** The closing delimiter must start before byte 65536: byte 65535 succeeds and byte 65536 gives `Unterminated`. Offsets include any BOM. In [config/limits.rs](../../crates/folt-core/src/config/limits.rs), `SCAN_LIMIT` is 65536, `SCAN_SLACK` is eight bytes and `SCAN_WINDOW` is their sum; `locate` can inspect those extra bytes to finish a delimiter. Tests: `document::enforces_scan_boundary_and_delimiters`, `document::edits_keep_the_closing_delimiter_within_the_scan_limit`, validation `declared_references::closing_delimiter_crossing_scan_boundary_is_read_from_the_stored_blob`.

**Scenario 6 - Binary input.** A NUL anywhere in the first 65536 bytes gives `Binary`, even after a valid closing delimiter or in input with no `Block`. `locate` checks this before looking for an opening line. A later NUL is outside this check. Tests: `document::preserves_bytes_and_owned_line_trivia`, validation `front_matter_awkward::structured_awkward_inputs`.

## Top-level nodes

**Scenario 7 - Line endings.** LF, CRLF and a BOM are preserved. `locate` takes `Block::line_ending` from the opening line alone; inserted lines use that ending, while existing lines retain their individual endings even in mixed input. Tests: `document::preserves_bytes_and_owned_line_trivia`, `document::creates_inserts_and_removes`.

**Scenario 8 - Keys.** `tokenize` considers a non-empty line at the top level if it does not start with a space, tab, `#` or `- `. An ordinary `Node` needs a key separator: `colon` chooses the first colon outside quotes and flow brackets followed by ASCII whitespace, the line end, `{` or `[`. Explicit keys are recognized separately. Tab-indented keys are not top-level keys. Tests: `owned_keys::owned_key_audit_counts_unicode_whitespace_consistently`, `owned_keys::owned_key_audit_ignores_tab_indented_keys`.

**Scenario 9 - Comments and spans.** `scan_punctuation` recognizes `#` at the start or after ASCII whitespace outside quotes. `Node::value` excludes spacing after the colon and the trailing header comment. `tokenize` retains original spans, allowing standalone comments and blank lines to survive edits. Tests: `scalars::scalar_rewrites_preserve_header_comment_spacing`, `comments::selected_alias_inline_comments_survive_set_and_remove`, `comments::nested_field_inline_comments_survive_set_and_remove`, validation `front_matter_awkward::name_edits_preserve_preamble_and_standalone_lines`.

**Scenario 10 - Wrapped flow values.** Values starting with `{` or `[` can span lines, including lines at column zero. `flow_end` uses the closing character at depth zero in the text after the opening character to end the value; `tokenize` resumes after that closing line. This is a permissive punctuation counter, not full YAML validation. Tests: `owned_keys::owned_key_audit_accepts_ordinary_and_clean_wrapped_flows`, `references::wrapped_flow_comments_and_indentation_are_understood`.

## Owned keys and hidden keys

**Scenario 11 - Foreign content.** Foreign values are not decoded or rewritten. Their order and bytes, as well as the body after front matter, survive owned-key edits; key spellings still undergo the safety checks below. Functions: `tokenize`, `unresolved_key_checks`, `edit_checks`. Tests: validation `front_matter::edits_note_bytes_without_touching_foreign_yaml_or_body`, `front_matter_properties::composed_documents_preserve_and_round_trip`.

**Scenario 12 - Owned spellings.** `OWNED_ID`, `OWNED_NAME`, `OWNED_REFERENCES` and `OWNED` in [config/names.rs](../../crates/folt-core/src/config/names.rs) define `folio-id`, `folio-name` and `folio-references`. Matching trims Unicode whitespace around a key; ASCII leading spaces and tabs still make the line indented under scenario 8. Functions: `owned_spelling`, `tokenize`, `audit`. Test: `owned_keys::owned_key_audit_counts_unicode_whitespace_consistently`.

**Scenario 13 - Duplicate owned keys.** `node` gives `Duplicate` when the requested owned key occurs more than once. Reads check the requested key; edits call `edit_checks` and check all owned keys, so a duplicate of any owned key blocks an edit. Test: `ambiguous_keys::refuses_unsafe_owned_nodes`.

**Scenario 14 - Hidden owned keys.** `audit` compares top-level owned-key spellings in the body with tokenized keys. An owned line swallowed into another `Node`, such as a malformed foreign flow with a later closer, gives `Hidden` on every read and edit. `checked_edit` rejects an edit that newly swallows an owned line. Plain scalars such as `folio-name:manual` inside foreign flows are ignored because their colon is not a key separator. Functions: `locate`, `audit`, `checked_edit`. Tests: `owned_keys::hidden_owned_keys_refuse_reads_and_duplicate_creating_edits`, `owned_keys::owned_key_audit_ignores_plain_scalars_in_foreign_flows`, `owned_keys::checked_edits_reject_newly_swallowed_owned_lines`, validation `front_matter_hidden::malformed_foreign_flows_cannot_hide_owned_keys`.

**Scenario 15 - Ambiguous keys.** A quoted owned key gives `Ambiguous` when that key is read, and blocks edits to all owned keys. Explicit owned keys, keys starting with the YAML property characters `!`, `&` or `*`, undecodable quoted keys, and explicit keys whose full text cannot be decoded to a single plain or quoted scalar give `Ambiguous` on all owned-key reads and edits. Explicit-key decoding can fold multiple plain lines into one scalar; quoted text must be completely consumed. Functions: `node`, `unresolved_key_checks`, `resolved_explicit_key`, `edit_checks`. Tests: `ambiguous_keys::refuses_unsafe_owned_nodes`, `ambiguous_keys::owned_or_unresolved_null_explicit_keys_remain_ambiguous`, `ambiguous_keys::refuses_owned_or_unresolved_multiline_explicit_keys`, `ambiguous_keys::explicit_keys_require_fully_decoded_foreign_scalars`, `ambiguous_keys::refuses_key_properties_before_owned_key_lookup`, `ambiguous_keys::reads_refuse_unresolved_owned_keys`, validation `front_matter_awkward::structured_awkward_inputs`.

## Scalars

**Scenario 16 - Foreign scalar keys.** Fully decodable foreign explicit and quoted keys remain readable and untouched. `edit_checks` additionally rejects implicit quoted foreign keys containing a backslash with `Ambiguous`; fully decoded explicit foreign keys are exempt from that additional check. Tests: `explicit_keys::reads_allow_decodable_foreign_explicit_and_quoted_keys`, `explicit_keys::implicit_escaped_foreign_keys_remain_opaque_for_edits`, `explicit_keys::foreign_multiline_explicit_keys_remain_readable_and_untouched`, `explicit_keys::foreign_null_explicit_keys_remain_readable_and_untouched`.

**Scenario 17 - Scalar decoding.** `read_name` and `read_id` use `read_scalar` and `scalar` for plain, single-quoted and double-quoted values. Single quotes decode `''` to `'`; double quotes support only `\"`, `\\`, `\n`, `\t` and `\r`, with other escapes giving `InvalidValue`. A value starting with `&`, `*`, `!`, `|`, `>`, `[` or `{` gives `Unsupported`. Plain text containing quotes or line breaks, starting with `?` or `:`, or consisting of `-` alone or `-` followed by whitespace gives `InvalidValue`. Text such as `null`, `true` and `1234` stays text. The quoted decoder is permissive about interior quotes and is not a full YAML validator. Tests: `scalars::written_plain_scalars_read_back_and_repeat_identically`, `scalars::names_are_double_quoted_in_existing_and_new_blocks`, `ambiguous_keys::refuses_unsafe_owned_nodes`; the complete escape and prefix rules are in `scalar`.

**Scenario 18 - Empty scalars.** Empty `folio-name:` and `folio-id:` values, including empty quoted strings, give `InvalidValue` when read or edited as that scalar. A missing key or missing `Block` gives `Ok(None)`. An empty scalar does not by itself prevent editing a different key: `edit_checks` runs `supported`, not `read_scalar`, on other owned values. Tests: `scalars::scalar_replacements_preserve_spacing_and_refuse_empty_values`, `owned_keys::owned_key_audit_counts_tab_and_empty_values_consistently`.

**Scenario 19 - YAML properties and tabs.** `supported` rejects unquoted `&`, `*` and `!` found by `punctuation` at depth zero in an owned `Node`, and continuation lines beginning with a tab after any spaces, with `Unsupported`. Characters inside quotes or comments are ignored; the punctuation scan does not inspect these characters at nested flow depth. A selected reference's nested scalars also pass through `scalar`, which rejects property prefixes. Test: `ambiguous_keys::refuses_unsafe_owned_nodes`.

**Scenario 20 - Scalar continuation.** A meaningful continuation line after an owned scalar gives `Unsupported`; meaningful means neither blank nor a comment after trimming. Multi-line scalars are not read. Function: `read_scalar`. Test: `ambiguous_keys::refuses_unsafe_owned_nodes` includes `folio-name: a` followed by `  continued`.

## Declared references

**Scenario 21 - Id values.** `read_id` requires `FolioId::parse` to accept a canonical lowercase hyphenated Folio id; otherwise it gives `InvalidValue`. Reference `id` fields use the same parsing. Test: `document::creates_inserts_and_removes`.

**Scenario 22 - Layouts.** `references` accepts a flow mapping of flow mappings, a block of aliases with flow mappings, and a block of aliases with nested block fields. Test: `references::reads_all_reference_forms`.

```yaml
folio-references: {logo: {name: image}}
```

```yaml
folio-references:
  logo: {name: image}
```

```yaml
folio-references:
  logo:
    name: image
    pin: opaque
```

**Scenario 23 - Target fields.** Each alias must have exactly one of `name` or `id`; `pin` is optional opaque text. Missing both targets or supplying both gives `InvalidValue`. Unknown fields give `Unsupported`, repeated fields give `Duplicate`, and empty or empty-quoted values give `InvalidValue`. An `id` must pass scenario 21. Nested block field keys permit spaces before colons. Flow field keys are compared as their trimmed spelling rather than decoded as quoted keys. `mapping` gives `InvalidValue` for missing mapping delimiters or separators, and `flow_token` gives it for meaningful text after a token comment. Functions: `reference`, `references`, `mapping`. Tests: `references::reference_target_mapping_rules_apply_to_block_and_flow`, `references::empty_reference_targets_and_pins_are_invalid`, `references::nested_block_field_keys_allow_spaces_before_colons`.

**Scenario 24 - Short references.** An alias mapped to a plain scalar, such as `logo: logo-image` or `logo: id:12345678-1234-4234-8234-123456789abc`, gives `InvalidValue`; the target must be a mapping. Function: `reference`. Tests: `references::reads_all_reference_forms`, validation `front_matter::short_references_are_refused`.

**Scenario 25 - Alias validity.** An empty alias (`''` or `""`) gives `InvalidValue`, including when `set_references` or `set_reference` parses an existing collection. Empty alias arguments also give `InvalidValue`. Repeated decoded aliases give `Duplicate`; `set_references` also rejects repeated supplied aliases. Functions: `reference`, `references`, `plain`, `set_references`, `set_reference`. Tests: `references::nested_block_entries_reject_empty_aliases`, `references::malformed_references_are_refused_without_partial_edits`.

**Scenario 26 - Block indentation.** The first meaningful block entry fixes the space indent; a different indent, zero indent within the owned continuation, or a tab gives `Unsupported`. Each nested block entry fixes its own consistent field indent, deeper than its alias. A column-zero key normally starts a new top-level `Node` under scenario 8; a swallowed owned line can instead give `Hidden` under scenario 14. Function: `references`. Tests: `references::malformed_references_are_refused_without_partial_edits`, `references::reference_target_mapping_rules_apply_to_block_and_flow`, `owned_keys::hidden_owned_keys_refuse_reads_and_duplicate_creating_edits`.

**Scenario 27 - Reference headers.** A non-empty header that is not a supported flow mapping gives `Unsupported`; so does meaningful text following a flow value, including `{} garbage` or meaningful continuation lines. An empty header with no entries gives an empty list, unlike the empty owned scalars in scenario 18. A missing key or `Block` also gives an empty list. Functions: `references`, `read_references`. Tests: `references::malformed_references_are_refused_without_partial_edits`, `owned_keys::owned_key_audit_counts_tab_and_empty_values_consistently`.

## Edits

**Scenario 28 - Flow comments.** Comments in flow collections count as whitespace for reads and edits, with LF and CRLF, including comments around field separators and nested mappings. Functions: `mapping`, `skip_flow_space`, `flow_token`. Tests: `references::nested_flow_mapping_comments_are_understood`, `references::block_entries_read_whole_wrapped_flow_values`, `comments::flow_comments_are_whitespace_in_reads_and_edits`.

**Scenario 29 - Setting a name.** `set_name` uses `set_scalar` to write a double-quoted string regardless of previous quoting. An existing equal value still produces an `Edit`. `None` removes the scalar header line, retaining standalone continuation comments and blank lines. Supplied text must pass `plain`: non-empty ASCII letters, digits or `-._/`, also accepted by `scalar`; `-` alone and `- x` are invalid. Full folio-name validation belongs to the caller. Tests: `scalars::names_are_double_quoted_in_existing_and_new_blocks`, `scalars::name_replacements_quote_plain_and_quoted_values_and_keep_endings`, `scalars::written_plain_scalars_read_back_and_repeat_identically`, validation `front_matter_awkward::name_edits_preserve_preamble_and_standalone_lines`.

**Scenario 30 - Setting an id.** `set_id` writes the supplied `FolioId` unquoted, including when replacing a quoted value; `None` removes its scalar header line. Function: `set_scalar`. Test: `scalars::ids_remain_unquoted_when_inserted_or_replaced`.

**Scenario 31 - Insertion and creation.** `insert` adds a missing owned key after the last existing owned `Node`, otherwise at the end of the body, using `Block::line_ending`. Without a `Block`, `create_block: false` gives no edit; `true` inserts `---`, the key line and `---`, each with LF, after any BOM. All original bytes follow unchanged, including later YAML documents. Tests: `document::creates_inserts_and_removes`, validation `front_matter_awkward::creation_keeps_later_documents_exactly`.

**Scenario 32 - Value spacing.** Scalar replacement preserves the original spaces or tabs after the colon and spacing before a header comment. `value_edit` adds one space if the replaced span begins immediately after the colon, and separates a following `#`. Empty owned scalar headers cannot be replaced under scenario 18; empty reference headers can be populated. Tests: `scalars::scalar_replacements_preserve_spacing_and_refuse_empty_values`, `scalars::scalar_rewrites_preserve_header_comment_spacing`, `comments::reference_rewrites_separate_preserved_header_comments`.

**Scenario 33 - Replacing references.** `set_references` replaces all entries with the canonical block form: two spaces before `alias:`, then four spaces before `name: "x"` or `id: "uuid"` and optional `pin: "p"`. Exactly one target is required. Aliases must pass `plain` and be unique. `quoted` rejects empty name or pin text, line breaks and NUL; it escapes double quotes and backslashes. An empty list removes the key while retaining standalone comments and blank lines. The header comment and trailing standalone comments survive replacement; comments inside replaced entries need not survive. Functions: `render_reference`, `remove_references`. Tests: `references::reference_target_mapping_rules_apply_to_block_and_flow`, `references::wrapped_flow_comments_and_indentation_are_understood`, `reference_edits::wrapped_and_commented_reference_edits_preserve_surrounding_bytes`, `reference_edits::nested_reference_replacement_keeps_trailing_comments_and_blank_lines`.

**Scenario 34 - Editing one alias.** `set_reference` sets or removes one alias; its alias argument must pass `plain` and equal the supplied `Reference::alias`, otherwise `InvalidValue`. A flow collection stays flow: insertion adds a separator when needed and removal drops one separator. A block collection retains its alias indent and writes nested fields two spaces deeper. Comments on selected block entries and fields survive, including as standalone comments when their field or entry is removed. Other entries remain byte-identical. Removing the last block entry leaves an empty `folio-references:` header; removing the last flow entry leaves `{}`. Removing an absent alias gives no edit. Tests: `reference_edits::single_alias_edits_preserve_other_document_bytes`, `reference_edits::single_alias_flow_edits_preserve_trailing_separators`, `reference_edits::wrapped_flow_alias_edits_preserve_all_comments`, `comments::selected_alias_inline_comments_survive_set_and_remove`, `comments::nested_field_inline_comments_survive_set_and_remove`, validation `front_matter::creates_then_removes_a_reference_in_a_note`.

**Scenario 35 - Edit validity.** An `Edit` is valid only for the exact bytes it was planned on. Plan again after applying another `Edit`. `apply_edit` replaces its byte range without I/O and gives `InvalidValue` for reversed or out-of-bounds ranges; it does not detect a different input with an in-bounds range. Test: `document::creates_inserts_and_removes` checks an out-of-bounds range; the reversed-range check and input-validity contract are in `apply_edit` and the module documentation.

**Scenario 36 - Checking the result.** `checked_edit` runs `locate` on the virtual result, bounded by `SCAN_WINDOW`. Pushing the closing delimiter to byte 65536 or later gives `Unterminated`; swallowing an owned line gives `Hidden`. No `Edit` is returned on error. Tests: `document::edits_keep_the_closing_delimiter_within_the_scan_limit`, `owned_keys::checked_edits_reject_newly_swallowed_owned_lines`.

Reads accept shapes that edits reject. All reads run `unresolved_key_checks`, but look up and validate only their requested owned value. Edits additionally run `edit_checks`: implicit foreign quoted keys containing backslashes are `Ambiguous`, every owned key is checked for duplicates and ambiguous spelling (scenario 15), and `supported` runs on every owned `Node`. Thus reading an absent `folio-id` beside `"folio-name": old` succeeds, while setting that id gives `Ambiguous`; reading an id beside a tagged name can succeed, while editing the id gives `Unsupported`. This does not mean every other value is decoded: an empty `folio-name:` (scenario 18) gives `InvalidValue` when edited itself, but `set_references` can still edit an empty reference collection beside it. Tests: validation `front_matter_awkward::structured_awkward_inputs`, `explicit_keys::implicit_escaped_foreign_keys_remain_opaque_for_edits`, `owned_keys::owned_key_audit_counts_tab_and_empty_values_consistently`.

## Errors

`FrontMatterError::fmt` prefixes every message with `front matter: `. The table gives the complete displayed messages and the triggering scenarios; scalar and reference syntax errors follow the checks described above, rather than general YAML validation. Invalid UTF-8 in inspected text also gives `InvalidValue` through `text` (scenarios 8, 11 and 17); foreign value bytes are opaque semantically but inspected front matter text must still be UTF-8.

| Variant | Message | Triggers |
| --- | --- | --- |
| `Unterminated` | `front matter: no closing delimiter within 64 KiB` | Missing close, including opening-only input (scenario 4); closing delimiter starting at or beyond the scan limit (scenario 5); edits or creation moving the close outside that limit (scenarios 31 and 36). |
| `Binary` | `front matter: NUL in input prefix` | NUL in the first 65536 bytes, before or after a valid block and even with no block (scenario 6); the virtual-result check applies the same rule (scenario 36). |
| `Duplicate` | `front matter: duplicate key` | Repeated requested owned key on read or any owned key on edit (scenario 13); repeated reference field (scenario 23); repeated decoded alias or repeated supplied alias in `set_references` (scenarios 25 and 33). |
| `Ambiguous` | `front matter: quoted or complex owned key` | Quoted requested owned key on read or any quoted owned key on edit; explicit owned key, property-prefixed key, undecodable quoted key or incompletely decoded explicit key (scenario 15); implicit quoted foreign key with a backslash on edit (scenario 16). |
| `Hidden` | `front matter: an owned key is hidden inside another node` | The raw owned-key audit differs from tokenized owned keys (scenario 14), including swallowed column-zero owned lines (scenario 26); an edit introduces that discrepancy (scenario 36). |
| `Unsupported` | `front matter: unsupported YAML construct` | Unsupported scalar prefixes (scenario 17); unquoted property characters detected by `supported` or tab-indented continuation (scenario 19); meaningful scalar continuation (scenario 20); unsupported reference field (scenario 23); inconsistent or zero block indentation within a continuation (scenario 26); unsupported reference header, meaningful flow suffix or continuation (scenario 27). These checks also apply during edits as described in scenarios 29, 30, 33 and 34. |
| `InvalidValue` | `front matter: invalid value` | Invalid UTF-8 in inspected text (scenarios 8, 11 and 17); unsupported double-quote escape or invalid plain scalar syntax (scenario 17); empty owned scalar (scenario 18); invalid Folio id (scenarios 21 and 23); missing or competing reference targets, empty field values, malformed mapping separators, incomplete flow mappings or meaningful text after a flow-token comment (scenarios 22 and 23); short reference (scenario 24); empty alias (scenario 25); missing block-entry or field colon (scenario 26); invalid supplied plain name or alias (scenarios 29, 33 and 34); invalid supplied target combination, empty name or pin, line break or NUL in rendered name or pin (scenario 33); mismatched alias argument (scenario 34); reversed or out-of-bounds edit range (scenario 35). |
