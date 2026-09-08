//! Identifiers regression coverage.

use super::*;

#[test]
fn slugify_normal() {
    assert_eq!(slugify("Deep Review"), "deep-review");
}

#[test]
fn slugify_special_chars() {
    assert_eq!(slugify("My Profile!@#$%"), "my-profile");
}

#[test]
fn slugify_consecutive_dashes() {
    assert_eq!(slugify("a---b"), "a-b");
}

#[test]
fn slugify_leading_trailing() {
    assert_eq!(slugify("  Hello World  "), "hello-world");
}

#[test]
fn slugify_empty() {
    assert_eq!(slugify(""), "");
}

#[test]
fn slugify_numbers() {
    assert_eq!(slugify("Profile 2.0"), "profile-2-0");
}

#[test]
fn validate_id_valid() {
    assert!(validate_profile_id("deep-review").is_ok());
    assert!(validate_profile_id("my_profile_1").is_ok());
}

#[test]
fn validate_id_empty() {
    assert!(validate_profile_id("").is_err());
}

#[test]
fn validate_id_invalid_chars() {
    assert!(validate_profile_id("has spaces").is_err());
    assert!(validate_profile_id("has.dots").is_err());
    assert!(validate_profile_id("path/traversal").is_err());
    assert!(validate_profile_id(&"a".repeat(65)).is_err());
}

#[test]
fn sanitize_normal_id() {
    assert_eq!(sanitize_step_id("contribution"), "contribution");
    assert_eq!(sanitize_step_id("my-step_1"), "my-step_1");
}

#[test]
fn sanitize_strips_slash() {
    // '/' is reserved for step_id/agent composite keys
    assert_eq!(sanitize_step_id("step/agent"), "step-agent");
    assert_eq!(sanitize_step_id("a/b/c"), "a-b-c");
}

#[test]
fn sanitize_strips_special_chars() {
    assert_eq!(sanitize_step_id("step with spaces"), "step-with-spaces");
    assert_eq!(sanitize_step_id("step@#$%!"), "step");
}

#[test]
fn sanitize_preserves_dots() {
    assert_eq!(sanitize_step_id("v2.1"), "v2.1");
}

#[test]
fn sanitize_collapses_dashes() {
    assert_eq!(sanitize_step_id("a///b"), "a-b");
    assert_eq!(sanitize_step_id("--leading--"), "leading");
}
