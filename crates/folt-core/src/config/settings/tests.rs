use super::*;

#[test]
fn defaults_are_the_default_constants() {
    let settings = Settings::resolve(&Environment::default(), FileSettings::default()).unwrap();
    assert_eq!(settings, Settings::default());
    assert_eq!(settings.large_file_bytes, DEFAULT_LARGE_FILE_BYTES);
    assert_eq!(settings.batch_total_bytes, DEFAULT_BATCH_TOTAL_BYTES);
    assert_eq!(settings.busy_timeout, DEFAULT_BUSY_TIMEOUT);
}

#[test]
fn file_value_beats_default_and_environment_beats_file() {
    let file = FileSettings {
        large_file_bytes: Some(9),
        ..FileSettings::default()
    };
    assert_eq!(
        Settings::resolve(&Environment::default(), file.clone())
            .unwrap()
            .large_file_bytes,
        9
    );
    let environment = Environment {
        large_file_bytes: Some("4".into()),
        ..Environment::default()
    };
    assert_eq!(
        Settings::resolve(&environment, file.clone())
            .unwrap()
            .large_file_bytes,
        4
    );
    let environment = Environment {
        large_file_bytes: Some("0".into()),
        ..Environment::default()
    };
    assert_eq!(
        Settings::resolve(&environment, file)
            .unwrap()
            .large_file_bytes,
        0
    );
}

#[test]
fn environment_overrides_batch_total_and_busy_timeout() {
    for bytes in ["0", "10", "18446744073709551615"] {
        let environment = Environment {
            batch_total_bytes: Some(bytes.into()),
            busy_timeout_ms: Some("250".into()),
            ..Environment::default()
        };
        let settings = Settings::resolve(&environment, FileSettings::default()).unwrap();
        assert_eq!(settings.batch_total_bytes, bytes.parse::<u64>().unwrap());
        assert_eq!(settings.busy_timeout, Duration::from_millis(250));
    }
}

#[test]
fn empty_variable_counts_as_unset() {
    let environment = Environment {
        large_file_bytes: Some("".into()),
        batch_total_bytes: Some("".into()),
        busy_timeout_ms: Some("".into()),
        ..Environment::default()
    };
    assert_eq!(
        Settings::resolve(&environment, FileSettings::default()).unwrap(),
        Settings::default()
    );
    let file = FileSettings {
        large_file_bytes: Some(9),
        ..FileSettings::default()
    };
    assert_eq!(
        Settings::resolve(&environment, file)
            .unwrap()
            .large_file_bytes,
        9
    );
}

#[test]
fn invalid_values_name_the_variable() {
    let values: Vec<OsString> = ["abc", "-1", "+1", " 1", "1 ", "18446744073709551616"]
        .into_iter()
        .map(OsString::from)
        .collect();
    #[cfg(unix)]
    let values = {
        use std::os::unix::ffi::OsStringExt;
        let mut values = values;
        values.push(OsString::from_vec(vec![0xff]));
        values
    };
    for value in values {
        for variable in [
            ENV_FOLT_LARGE_FILE_BYTES,
            ENV_FOLT_BATCH_TOTAL_BYTES,
            ENV_FOLT_BUSY_TIMEOUT_MS,
        ] {
            let mut environment = Environment::default();
            match variable {
                ENV_FOLT_LARGE_FILE_BYTES => environment.large_file_bytes = Some(value.clone()),
                ENV_FOLT_BATCH_TOTAL_BYTES => environment.batch_total_bytes = Some(value.clone()),
                _ => environment.busy_timeout_ms = Some(value.clone()),
            }
            let error = Settings::resolve(&environment, FileSettings::default()).unwrap_err();
            assert!(
                matches!(&error, SettingsError::Invalid { variable: actual, value: actual_value, .. } if *actual == variable && actual_value == &value)
            );
            assert!(error.to_string().starts_with(variable));
        }
    }
}

#[test]
fn busy_timeout_bounds() {
    for millis in [0_u64, 2147483647] {
        let environment = Environment {
            busy_timeout_ms: Some(millis.to_string().into()),
            ..Environment::default()
        };
        assert_eq!(
            Settings::resolve(&environment, FileSettings::default())
                .unwrap()
                .busy_timeout,
            Duration::from_millis(millis)
        );
    }
    let environment = Environment {
        busy_timeout_ms: Some("2147483648".into()),
        ..Environment::default()
    };
    assert!(matches!(
        Settings::resolve(&environment, FileSettings::default()),
        Err(SettingsError::Invalid {
            variable: ENV_FOLT_BUSY_TIMEOUT_MS,
            ..
        })
    ));
}
