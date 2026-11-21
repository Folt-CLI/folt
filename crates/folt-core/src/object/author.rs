//! Validates and formats author identities; commit and tag bodies live elsewhere.

use std::fmt;
use std::str::FromStr;

use crate::config::names::ANONYMOUS_AUTHOR;

use super::ObjectError;
use super::fields::validate_value;

/// A validated identity, encoded as `username` or `username <email>`.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Author {
    username: String,
    email: Option<String>,
}

impl Author {
    pub fn new(username: &str, email: Option<&str>) -> Result<Self, ObjectError> {
        validate_value(username)?;
        if username.trim().is_empty() || username.contains(['<', '>']) {
            return Err(ObjectError::InvalidObject);
        }
        if let Some(email) = email {
            validate_value(email)?;
            if email.split('@').count() != 2
                || email.split('@').any(str::is_empty)
                || email.chars().any(char::is_whitespace)
                || email.contains(['<', '>'])
            {
                return Err(ObjectError::InvalidObject);
            }
        }
        Ok(Self {
            username: username.into(),
            email: email.map(String::from),
        })
    }

    pub fn anonymous() -> Self {
        Self {
            username: ANONYMOUS_AUTHOR.into(),
            email: None,
        }
    }

    pub fn username(&self) -> &str {
        &self.username
    }
    pub fn email(&self) -> Option<&str> {
        self.email.as_deref()
    }
}

impl fmt::Display for Author {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match &self.email {
            Some(email) => write!(f, "{} <{email}>", self.username),
            None => write!(f, "{}", self.username),
        }
    }
}

impl FromStr for Author {
    type Err = ObjectError;
    fn from_str(value: &str) -> Result<Self, Self::Err> {
        if let Some((username, email)) = value
            .strip_suffix('>')
            .and_then(|value| value.rsplit_once(" <"))
        {
            Self::new(username, Some(email))
        } else {
            Self::new(value, None)
        }
    }
}
