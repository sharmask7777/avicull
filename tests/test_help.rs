use avicull::help::{list_topics, render_help, HelpTopic};
use std::str::FromStr;

#[test]
fn test_help_topics_list_not_empty() {
    let topics = list_topics();
    assert!(!topics.is_empty());
    assert!(topics.iter().any(|(name, _)| *name == "quickstart"));
    assert!(topics.iter().any(|(name, _)| *name == "organize"));
    assert!(topics.iter().any(|(name, _)| *name == "invariants"));
    assert!(topics.iter().any(|(name, _)| *name == "models"));
    assert!(topics.iter().any(|(name, _)| *name == "update"));
    assert!(topics.iter().any(|(name, _)| *name == "scan"));
}

#[test]
fn test_help_topics_each_has_non_empty_description() {
    for (name, desc) in list_topics() {
        assert!(!name.is_empty(), "Topic name must not be empty");
        assert!(
            !desc.is_empty(),
            "Topic '{}' must have a non-empty description",
            name
        );
    }
}

#[test]
fn test_render_help_for_each_topic() {
    for topic in [
        HelpTopic::Quickstart,
        HelpTopic::Organize,
        HelpTopic::Invariants,
        HelpTopic::Models,
        HelpTopic::Update,
        HelpTopic::Scan,
        HelpTopic::All,
    ] {
        let content = render_help(Some(topic));
        assert!(
            !content.is_empty(),
            "Topic {:?} should render non-empty content",
            topic
        );
        assert!(content.contains("avicull"), "Help should mention avicull");
        assert!(
            content.contains("avicull Help —"),
            "Header must mention avicull Help"
        );
    }
}

#[test]
fn test_render_help_non_all_topics_show_available_topics_footer() {
    // Every topic except All must show the "Available topics" footer guiding the user
    for topic in [
        HelpTopic::Quickstart,
        HelpTopic::Organize,
        HelpTopic::Invariants,
        HelpTopic::Models,
        HelpTopic::Update,
        HelpTopic::Scan,
    ] {
        let content = render_help(Some(topic));
        assert!(
            content.contains("Available topics:"),
            "Topic {:?} must show available topics footer",
            topic
        );
        assert!(
            content.contains("avicull help <TOPIC>"),
            "Topic {:?} must reference 'avicull help <TOPIC>'",
            topic
        );
    }
}

#[test]
fn test_render_help_all_topic_does_not_show_footer() {
    // The 'all' topic renders the full manual and must NOT show the footer (it is the manual)
    let content = render_help(Some(HelpTopic::All));
    // The All topic shows COMMANDS section instead
    assert!(
        content.contains("COMMANDS:"),
        "All topic must list COMMANDS"
    );
}

#[test]
fn test_render_help_topic_key_content() {
    // Verify each topic surfaces its canonical section header
    assert!(render_help(Some(HelpTopic::Quickstart)).contains("QUICKSTART & BASIC USAGE"));
    assert!(render_help(Some(HelpTopic::Organize)).contains("THE ORGANIZE PIPELINE"));
    assert!(render_help(Some(HelpTopic::Scan)).contains("PRE-FLIGHT ARCHIVE SCANNING"));
    assert!(render_help(Some(HelpTopic::Invariants)).contains("FILESYSTEM SAFETY"));
    assert!(render_help(Some(HelpTopic::Models)).contains("NEURAL MODELS"));
    assert!(render_help(Some(HelpTopic::Update)).contains("SELF-UPDATE"));
}

#[test]
fn test_render_help_default_shows_overview() {
    let content = render_help(None);
    assert!(!content.is_empty());
    assert!(content.contains("User Manual") || content.contains("Quickstart"));
    assert!(content.contains("avicull help <TOPIC>"));
}

#[test]
fn test_parse_help_topic_case_insensitive() {
    assert_eq!(
        HelpTopic::from_str("quickstart").unwrap(),
        HelpTopic::Quickstart
    );
    assert_eq!(
        HelpTopic::from_str("QUICKSTART").unwrap(),
        HelpTopic::Quickstart
    );
    assert_eq!(
        HelpTopic::from_str("organize").unwrap(),
        HelpTopic::Organize
    );
    assert_eq!(
        HelpTopic::from_str("invariants").unwrap(),
        HelpTopic::Invariants
    );
    assert_eq!(HelpTopic::from_str("models").unwrap(), HelpTopic::Models);
    assert_eq!(HelpTopic::from_str("update").unwrap(), HelpTopic::Update);
    assert_eq!(HelpTopic::from_str("scan").unwrap(), HelpTopic::Scan);
    assert_eq!(HelpTopic::from_str("all").unwrap(), HelpTopic::All);
    assert!(HelpTopic::from_str("unknown_topic").is_err());
}

#[test]
fn test_parse_help_topic_aliases() {
    // Each topic supports multiple idiomatic aliases
    assert_eq!(HelpTopic::from_str("start").unwrap(), HelpTopic::Quickstart);
    assert_eq!(HelpTopic::from_str("sort").unwrap(), HelpTopic::Organize);
    assert_eq!(
        HelpTopic::from_str("safety").unwrap(),
        HelpTopic::Invariants
    );
    assert_eq!(HelpTopic::from_str("apfs").unwrap(), HelpTopic::Invariants);
    assert_eq!(HelpTopic::from_str("weights").unwrap(), HelpTopic::Models);
    assert_eq!(HelpTopic::from_str("ai").unwrap(), HelpTopic::Models);
    assert_eq!(HelpTopic::from_str("upgrade").unwrap(), HelpTopic::Update);
    assert_eq!(HelpTopic::from_str("check").unwrap(), HelpTopic::Scan);
    assert_eq!(HelpTopic::from_str("full").unwrap(), HelpTopic::All);
    assert_eq!(HelpTopic::from_str("manual").unwrap(), HelpTopic::All);
}

#[test]
fn test_parse_help_topic_unknown_returns_error_with_hint() {
    // Error message must guide the user toward valid topics
    let err = HelpTopic::from_str("doesnotexist").unwrap_err();
    let msg = err.to_string();
    assert!(
        msg.contains("Available topics:"),
        "Error must list available topics: {}",
        msg
    );
}

#[test]
fn test_help_topic_display() {
    // Display impl must round-trip through FromStr
    for (expected_str, topic) in [
        ("quickstart", HelpTopic::Quickstart),
        ("organize", HelpTopic::Organize),
        ("scan", HelpTopic::Scan),
        ("invariants", HelpTopic::Invariants),
        ("models", HelpTopic::Models),
        ("update", HelpTopic::Update),
        ("all", HelpTopic::All),
    ] {
        assert_eq!(topic.to_string(), expected_str);
        assert_eq!(HelpTopic::from_str(expected_str).unwrap(), topic);
    }
}
