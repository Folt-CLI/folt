//! Storage names and fixed text identifiers.

/// Registry database filename (UTF-8 text).
pub(crate) const REGISTRY_FILE: &str = "registry.db";

/// Home lock filename (UTF-8 text).
pub(crate) const LOCK_FILE: &str = "lock";

/// Home temporary directory name (UTF-8 text).
pub(crate) const TMP_DIR: &str = "tmp";

/// Configuration filename (UTF-8 text).
pub(crate) const CONFIG_FILE: &str = "config.toml";

/// Default home directory name beneath the OS home (UTF-8 text).
pub(crate) const HOME_DIR_NAME: &str = ".folt";

/// Parent directory name for Folio directories (UTF-8 text).
pub(crate) const FOLIOS_DIR: &str = "folios";

/// Staging directory name for new Folio directories (UTF-8 text).
pub(crate) const NEW_DIR: &str = ".new";

/// Object store directory name (UTF-8 text).
pub(crate) const BLOBS_DIR: &str = "blobs";

/// History refs directory name (UTF-8 text).
pub(crate) const REFS_DIR: &str = "refs";

/// Branch refs directory name (UTF-8 text).
pub(crate) const HEADS_DIR: &str = "heads";

/// Tag refs directory name (UTF-8 text).
pub(crate) const TAGS_DIR: &str = "tags";

/// Retention pin refs directory name (UTF-8 text).
pub(crate) const PINS_DIR: &str = "pins";

/// Directory Folio marker filename (UTF-8 text).
pub const MARKER_FILE: &str = "__folio__.md";

/// Object filename extension, including the dot (UTF-8 text).
pub(crate) const BLOB_EXTENSION: &str = ".blob";

/// Temporary ref filename prefix (UTF-8 text).
pub(crate) const TEMP_PREFIX_REF: &str = ".ref.";

/// Object input spool filename prefix (UTF-8 text).
pub(crate) const TEMP_PREFIX_INPUT: &str = ".input.";

/// Temporary stored object filename prefix (UTF-8 text).
pub(crate) const TEMP_PREFIX_OBJECT: &str = ".object.";

/// Temporary copy filename prefix recognized during inspection (UTF-8 text).
pub(crate) const TEMP_PREFIX_COPY: &str = ".folt-copy-";

/// Temporary copy filename tag (UTF-8 text).
pub(crate) const TEMP_TAG_COPY: &str = "folt-copy";

/// Placement conversion temporary filename tag (UTF-8 text).
pub(crate) const TEMP_TAG_CONVERT: &str = "folt-convert";

/// Front matter rewrite temporary filename tag (UTF-8 text).
pub(crate) const TEMP_TAG_NAME: &str = "folt-name";

/// Link probe temporary filename tag (UTF-8 text).
pub(crate) const TEMP_TAG_LINK_PROBE: &str = "folt-link-probe";

/// Default author username (UTF-8 text).
pub(crate) const ANONYMOUS_AUTHOR: &str = "anonymous";

/// Home override environment variable name (UTF-8 text).
pub(crate) const ENV_FOLT_HOME: &str = "FOLT_HOME";

/// FOLT_LARGE_FILE_BYTES environment variable name (UTF-8 text).
pub(crate) const ENV_FOLT_LARGE_FILE_BYTES: &str = "FOLT_LARGE_FILE_BYTES";

/// FOLT_BATCH_TOTAL_BYTES environment variable name (UTF-8 text).
pub(crate) const ENV_FOLT_BATCH_TOTAL_BYTES: &str = "FOLT_BATCH_TOTAL_BYTES";

/// FOLT_BUSY_TIMEOUT_MS environment variable name (UTF-8 text).
pub(crate) const ENV_FOLT_BUSY_TIMEOUT_MS: &str = "FOLT_BUSY_TIMEOUT_MS";

/// Fixed default Folio branch name (UTF-8 text).
pub(crate) const DEFAULT_BRANCH: &str = "main";

/// Working content filename and link placement target (UTF-8 text).
pub const CONTENT_FILE: &str = "folio.main";

/// Registry metadata key prefix for pending renames (UTF-8 text).
pub(crate) const RENAME_PENDING_PREFIX: &str = "rename-pending:";

/// Owned front matter key for the Folio id (UTF-8 text).
pub(crate) const OWNED_ID: &str = "folio-id";

/// Owned front matter key for the folio-name (UTF-8 text).
pub(crate) const OWNED_NAME: &str = "folio-name";

/// Owned front matter key for declared references (UTF-8 text).
pub(crate) const OWNED_REFERENCES: &str = "folio-references";

/// Markdown filename extensions without dots (ASCII, case insensitive).
pub(crate) const MARKDOWN_EXTENSIONS: [&str; 2] = ["md", "markdown"];

/// Owned front matter keys (UTF-8 text).
pub(crate) const OWNED: [&str; 3] = [OWNED_ID, OWNED_NAME, OWNED_REFERENCES];

/// Retired home format file name tolerated inside a home (UTF-8 text).
pub(crate) const RETIRED_FORMAT_ENTRY: &str = "format";

/// Allowed home entry names, including the retired format file (UTF-8 text).
pub(crate) const HOME_ENTRIES: [&str; 6] = [
    LOCK_FILE,
    TMP_DIR,
    FOLIOS_DIR,
    REGISTRY_FILE,
    RETIRED_FORMAT_ENTRY,
    CONFIG_FILE,
];
