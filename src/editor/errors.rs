#![allow(clippy::arithmetic_side_effects)]

use std::io::{Error, ErrorKind};

pub fn reason(err: &Error) -> String {
    match err.kind() {
        ErrorKind::InvalidData if err.get_ref().is_none() => {
            "not valid UTF-8 text (binary file or another encoding)".to_string()
        }
        _ => err.to_string(),
    }
}

pub fn describe(action: &str, target: &str, err: &Error) -> String {
    format!("Error: {action}: {} [{target}]", reason(err))
}

pub fn toml_detail(text: &str, err: &::toml::de::Error) -> String {
    let message = err.message();
    match err.span() {
        Some(span) => {
            let before = text.get(..span.start).unwrap_or(text);
            let line = before.matches('\n').count() + 1;
            let col = before.rsplit('\n').next().map_or(0, |l| l.chars().count()) + 1;
            format!("line {line}, col {col}: {message}")
        }
        None => message.to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn invalid_data_without_a_cause_means_not_utf8() {
        let err = Error::from(ErrorKind::InvalidData);
        assert!(reason(&err).contains("not valid UTF-8"));
    }

    #[test]
    fn other_errors_keep_their_message() {
        let err = Error::new(ErrorKind::Other, "boom");
        assert_eq!(reason(&err), "boom");
        assert_eq!(describe("save failed", "a.txt", &err), "Error: save failed: boom [a.txt]");
    }

    #[test]
    fn toml_errors_name_the_line() {
        let text = "a = 1\nb = \n";
        let err = ::toml::from_str::<::toml::Value>(text).unwrap_err();
        assert!(toml_detail(text, &err).starts_with("line "));
    }
}