use crate::native_menu::types::NativeMenuCanonicalActionKind;

struct CanonicalActionNames {
    action: NativeMenuCanonicalActionKind,
    verbs: &'static [&'static str],
    recognised_by_label: bool,
}

const CANONICAL_ACTIONS: [CanonicalActionNames; 15] = [
    CanonicalActionNames {
        action: NativeMenuCanonicalActionKind::Open,
        verbs: &["open"],
        recognised_by_label: false,
    },
    CanonicalActionNames {
        action: NativeMenuCanonicalActionKind::OpenWith,
        verbs: &["openwith"],
        recognised_by_label: false,
    },
    CanonicalActionNames {
        action: NativeMenuCanonicalActionKind::Copy,
        verbs: &["copy"],
        recognised_by_label: true,
    },
    CanonicalActionNames {
        action: NativeMenuCanonicalActionKind::CopyAsPath,
        verbs: &["copyaspath"],
        recognised_by_label: true,
    },
    CanonicalActionNames {
        action: NativeMenuCanonicalActionKind::Cut,
        verbs: &["cut"],
        recognised_by_label: true,
    },
    CanonicalActionNames {
        action: NativeMenuCanonicalActionKind::Paste,
        verbs: &["paste"],
        recognised_by_label: true,
    },
    CanonicalActionNames {
        action: NativeMenuCanonicalActionKind::Rename,
        verbs: &["rename"],
        recognised_by_label: true,
    },
    CanonicalActionNames {
        action: NativeMenuCanonicalActionKind::Delete,
        verbs: &["delete"],
        recognised_by_label: true,
    },
    CanonicalActionNames {
        action: NativeMenuCanonicalActionKind::Properties,
        verbs: &["properties"],
        recognised_by_label: true,
    },
    CanonicalActionNames {
        action: NativeMenuCanonicalActionKind::Share,
        verbs: &["share"],
        recognised_by_label: false,
    },
    CanonicalActionNames {
        action: NativeMenuCanonicalActionKind::Compress,
        verbs: &["compress", "windowscompresszipfile"],
        recognised_by_label: false,
    },
    CanonicalActionNames {
        action: NativeMenuCanonicalActionKind::Extract,
        verbs: &["extract"],
        recognised_by_label: false,
    },
    CanonicalActionNames {
        action: NativeMenuCanonicalActionKind::Refresh,
        verbs: &["refresh"],
        recognised_by_label: true,
    },
    CanonicalActionNames {
        action: NativeMenuCanonicalActionKind::NewFolder,
        verbs: &["new", "newfolder"],
        recognised_by_label: false,
    },
    CanonicalActionNames {
        action: NativeMenuCanonicalActionKind::SelectAll,
        verbs: &["selectall"],
        recognised_by_label: true,
    },
];

const OPEN_WITH_LABEL_PREFIX: &str = "openwith";

pub fn normalize_verb(value: &str) -> String {
    value
        .chars()
        .filter(|character| character.is_ascii_alphanumeric())
        .flat_map(char::to_lowercase)
        .collect()
}

fn action_for_verb(verb: &str) -> Option<NativeMenuCanonicalActionKind> {
    CANONICAL_ACTIONS
        .iter()
        .find(|names| names.verbs.contains(&verb))
        .map(|names| names.action.clone())
}

fn action_for_label(label: &str) -> Option<NativeMenuCanonicalActionKind> {
    let normalized_label = normalize_verb(label);
    if normalized_label.starts_with(OPEN_WITH_LABEL_PREFIX) {
        return Some(NativeMenuCanonicalActionKind::OpenWith);
    }
    CANONICAL_ACTIONS
        .iter()
        .filter(|names| names.recognised_by_label)
        .find(|names| names.verbs.contains(&normalized_label.as_str()))
        .map(|names| names.action.clone())
}

pub fn classify_canonical_action(
    normalized_verb: Option<&str>,
    label: &str,
) -> Option<NativeMenuCanonicalActionKind> {
    action_for_verb(normalized_verb.unwrap_or_default()).or_else(|| action_for_label(label))
}
