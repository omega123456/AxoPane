use file_explorer_lib::native_menu::canonical::{classify_canonical_action, normalize_verb};
use file_explorer_lib::native_menu::types::NativeMenuCanonicalActionKind;

#[test]
fn normalize_verb_keeps_lowercase_alphanumerics_only() {
    assert_eq!(normalize_verb("Open &With…"), "openwith");
    assert_eq!(
        normalize_verb("Windows.CompressZipFile"),
        "windowscompresszipfile"
    );
    assert_eq!(normalize_verb(""), "");
}

#[test]
fn every_canonical_verb_maps_to_its_action() {
    let cases = [
        ("open", NativeMenuCanonicalActionKind::Open),
        ("openwith", NativeMenuCanonicalActionKind::OpenWith),
        ("copy", NativeMenuCanonicalActionKind::Copy),
        ("copyaspath", NativeMenuCanonicalActionKind::CopyAsPath),
        ("cut", NativeMenuCanonicalActionKind::Cut),
        ("paste", NativeMenuCanonicalActionKind::Paste),
        ("rename", NativeMenuCanonicalActionKind::Rename),
        ("delete", NativeMenuCanonicalActionKind::Delete),
        ("properties", NativeMenuCanonicalActionKind::Properties),
        ("share", NativeMenuCanonicalActionKind::Share),
        ("compress", NativeMenuCanonicalActionKind::Compress),
        (
            "windowscompresszipfile",
            NativeMenuCanonicalActionKind::Compress,
        ),
        ("extract", NativeMenuCanonicalActionKind::Extract),
        ("refresh", NativeMenuCanonicalActionKind::Refresh),
        ("new", NativeMenuCanonicalActionKind::NewFolder),
        ("newfolder", NativeMenuCanonicalActionKind::NewFolder),
        ("selectall", NativeMenuCanonicalActionKind::SelectAll),
    ];
    for (verb, action) in cases {
        assert_eq!(
            classify_canonical_action(Some(verb), "Unrelated"),
            Some(action),
            "verb {verb}"
        );
    }
}

#[test]
fn the_verb_wins_over_the_label() {
    assert_eq!(
        classify_canonical_action(Some("cut"), "Delete"),
        Some(NativeMenuCanonicalActionKind::Cut)
    );
}

#[test]
fn labels_identify_only_the_label_recognised_actions() {
    assert_eq!(
        classify_canonical_action(None, "Open with Visual Studio Code"),
        Some(NativeMenuCanonicalActionKind::OpenWith)
    );
    assert_eq!(
        classify_canonical_action(Some("customverb"), "&Delete"),
        Some(NativeMenuCanonicalActionKind::Delete)
    );
    assert_eq!(
        classify_canonical_action(None, "Select all"),
        Some(NativeMenuCanonicalActionKind::SelectAll)
    );
    assert_eq!(classify_canonical_action(None, "Share"), None);
    assert_eq!(classify_canonical_action(None, "Open"), None);
    assert_eq!(classify_canonical_action(None, "Extract All…"), None);
}
