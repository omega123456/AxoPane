use super::provider::{NativeMenuProvider, ProviderInvocation, ProviderNativeMenuItem};
use super::shell_executor::ShellExecutor;
use super::types::{
    LoadNativeMenuRequest, NativeMenuCanonicalActionKind, NativeMenuIcon, NativeMenuIconKind,
    NativeMenuTargetKind,
};
use crate::ipc::types::MenuActionStatus;

#[derive(Default)]
pub struct FakeNativeMenuProvider;

impl NativeMenuProvider for FakeNativeMenuProvider {
    fn load_menu(
        &self,
        request: &LoadNativeMenuRequest,
        executor: &ShellExecutor,
    ) -> Vec<ProviderNativeMenuItem> {
        let target_kind = request.target_kind.clone();
        executor.execute(move || fixture_menu(&target_kind))
    }

    fn invoke(
        &self,
        invocation: &ProviderInvocation,
        executor: &ShellExecutor,
    ) -> MenuActionStatus {
        let invocation = invocation.clone();
        executor.execute(move || match invocation {
            ProviderInvocation::Fake { action_id } => {
                MenuActionStatus::handled_with_message(format!("invoked:{action_id}"))
            }
            ProviderInvocation::Windows { .. } => {
                MenuActionStatus::unsupported("fake-provider-does-not-run-windows-shell")
            }
            ProviderInvocation::WindowsModern { .. } => {
                MenuActionStatus::unsupported("fake-provider-does-not-run-windows-shell")
            }
        })
    }
}

fn target_suffix(kind: &NativeMenuTargetKind) -> &'static str {
    match kind {
        NativeMenuTargetKind::File => "file",
        NativeMenuTargetKind::Folder => "folder",
        NativeMenuTargetKind::Multi => "multi",
        NativeMenuTargetKind::Mixed => "mixed",
        NativeMenuTargetKind::DriveRoot => "drive-root",
        NativeMenuTargetKind::Background => "background",
        NativeMenuTargetKind::Tree => "tree",
        NativeMenuTargetKind::Tab => "tab",
    }
}

fn target_label(kind: &NativeMenuTargetKind) -> &'static str {
    match kind {
        NativeMenuTargetKind::File => "file action",
        NativeMenuTargetKind::Folder => "folder action",
        NativeMenuTargetKind::Multi => "multi action",
        NativeMenuTargetKind::Mixed => "mixed action",
        NativeMenuTargetKind::DriveRoot => "drive action",
        NativeMenuTargetKind::Background => "background action",
        NativeMenuTargetKind::Tree => "tree action",
        NativeMenuTargetKind::Tab => "tab action",
    }
}

fn fixture_menu(target_kind: &NativeMenuTargetKind) -> Vec<ProviderNativeMenuItem> {
    if matches!(target_kind, NativeMenuTargetKind::Tab) {
        return Vec::new();
    }
    let suffix = target_suffix(target_kind);
    vec![
        fake_action(
            &format!("fixture-target-{suffix}"),
            &format!("Fixture {}", target_label(target_kind)),
            &format!("fixture{suffix}"),
            &format!("fake.target.{suffix}"),
        ),
        canonical_fake_action(
            NativeMenuCanonicalActionKind::OpenWith,
            fake_action(
                "fixture-open-with",
                "Open with Fixture",
                "OpenWith",
                "fake.openWith",
            ),
        ),
        fake_submenu(
            "fixture-archive-tools",
            "Fixture archive tools",
            archive_tool_items(),
        ),
        ProviderNativeMenuItem {
            icon: Some(NativeMenuIcon {
                kind: NativeMenuIconKind::DataUrl,
                data_url: "data:image/png;base64,RkFLRQ==".to_string(),
                alt: Some("Fixture icon".to_string()),
            }),
            ..fake_action(
                "fixture-open-terminal",
                "Open in Fixture Terminal",
                "openinfixtureterminal",
                "fake.openTerminal",
            )
        },
        canonical_fake_action(
            NativeMenuCanonicalActionKind::Copy,
            fake_action("fixture-copy-duplicate", "Copy", "copy", "fake.copy"),
        ),
        fake_action(
            "fixture-properties-duplicate",
            "Properties",
            "properties",
            "fake.properties",
        ),
    ]
}

fn archive_tool_items() -> Vec<ProviderNativeMenuItem> {
    vec![
        canonical_fake_action(
            NativeMenuCanonicalActionKind::Compress,
            fake_action(
                "fixture-compress",
                "Add to fixture.zip",
                "compress",
                "fake.compress",
            ),
        ),
        canonical_fake_action(
            NativeMenuCanonicalActionKind::Extract,
            fake_action(
                "fixture-extract",
                "Extract with Fixture",
                "extract",
                "fake.extract",
            ),
        ),
        fake_action(
            "fixture-share-with-team",
            "Share with team",
            "sharewithteam",
            "fake.shareWithTeam",
        ),
    ]
}

fn fake_action(id: &str, label: &str, verb: &str, action_id: &str) -> ProviderNativeMenuItem {
    ProviderNativeMenuItem {
        id: id.to_string(),
        label: label.to_string(),
        enabled: true,
        danger: false,
        canonical_action_kind: None,
        normalized_verb: Some(verb.to_string()),
        icon: None,
        invocation: Some(ProviderInvocation::Fake {
            action_id: action_id.to_string(),
        }),
        children: Vec::new(),
    }
}

fn canonical_fake_action(
    kind: NativeMenuCanonicalActionKind,
    item: ProviderNativeMenuItem,
) -> ProviderNativeMenuItem {
    ProviderNativeMenuItem {
        canonical_action_kind: Some(kind),
        ..item
    }
}

fn fake_submenu(
    id: &str,
    label: &str,
    children: Vec<ProviderNativeMenuItem>,
) -> ProviderNativeMenuItem {
    ProviderNativeMenuItem {
        id: id.to_string(),
        label: label.to_string(),
        enabled: true,
        danger: false,
        canonical_action_kind: None,
        normalized_verb: None,
        icon: None,
        invocation: None,
        children,
    }
}
