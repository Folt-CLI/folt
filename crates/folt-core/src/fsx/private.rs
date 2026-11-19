use std::fs::{DirBuilder, OpenOptions};
#[cfg(unix)]
use std::os::unix::fs::{DirBuilderExt, OpenOptionsExt};

pub(crate) fn private_directory_builder() -> DirBuilder {
    let builder = DirBuilder::new();
    #[cfg(unix)]
    let builder = {
        let mut builder = builder;
        builder.mode(0o700);
        builder
    };
    builder
}

pub(crate) fn private_file_options() -> OpenOptions {
    let options = OpenOptions::new();
    #[cfg(unix)]
    let options = {
        let mut options = options;
        options.mode(0o600);
        options
    };
    options
}

#[cfg(test)]
mod tests;
