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
        let request = request.clone();
        executor.execute(move || {
            if matches!(request.target_kind, NativeMenuTargetKind::Tab) {
                return Vec::new();
            }

            vec![
                ProviderNativeMenuItem {
                    id: format!("fixture-target-{}", target_suffix(&request.target_kind)),
                    label: format!("Fixture {}", target_label(&request.target_kind)),
                    enabled: true,
                    danger: false,
                    canonical_action_kind: None,
                    normalized_verb: Some(format!(
                        "fixture{}",
                        target_suffix(&request.target_kind)
                    )),
                    icon: None,
                    invocation: Some(ProviderInvocation::Fake {
                        action_id: format!("fake.target.{}", target_suffix(&request.target_kind)),
                    }),
                    children: Vec::new(),
                },
                ProviderNativeMenuItem {
                    id: "fixture-open-with".to_string(),
                    label: "Open with Fixture".to_string(),
                    enabled: true,
                    danger: false,
                    canonical_action_kind: Some(NativeMenuCanonicalActionKind::OpenWith),
                    normalized_verb: Some("OpenWith".to_string()),
                    icon: None,
                    invocation: Some(ProviderInvocation::Fake {
                        action_id: "fake.openWith".to_string(),
                    }),
                    children: Vec::new(),
                },
                ProviderNativeMenuItem {
                    id: "fixture-archive-tools".to_string(),
                    label: "Fixture archive tools".to_string(),
                    enabled: true,
                    danger: false,
                    canonical_action_kind: None,
                    normalized_verb: None,
                    icon: None,
                    invocation: None,
                    children: vec![
                        ProviderNativeMenuItem {
                            id: "fixture-compress".to_string(),
                            label: "Add to fixture.zip".to_string(),
                            enabled: true,
                            danger: false,
                            canonical_action_kind: Some(NativeMenuCanonicalActionKind::Compress),
                            normalized_verb: Some("compress".to_string()),
                            icon: None,
                            invocation: Some(ProviderInvocation::Fake {
                                action_id: "fake.compress".to_string(),
                            }),
                            children: Vec::new(),
                        },
                        ProviderNativeMenuItem {
                            id: "fixture-extract".to_string(),
                            label: "Extract with Fixture".to_string(),
                            enabled: true,
                            danger: false,
                            canonical_action_kind: Some(NativeMenuCanonicalActionKind::Extract),
                            normalized_verb: Some("extract".to_string()),
                            icon: None,
                            invocation: Some(ProviderInvocation::Fake {
                                action_id: "fake.extract".to_string(),
                            }),
                            children: Vec::new(),
                        },
                        ProviderNativeMenuItem {
                            id: "fixture-share-with-team".to_string(),
                            label: "Share with team".to_string(),
                            enabled: true,
                            danger: false,
                            canonical_action_kind: None,
                            normalized_verb: Some("sharewithteam".to_string()),
                            icon: None,
                            invocation: Some(ProviderInvocation::Fake {
                                action_id: "fake.shareWithTeam".to_string(),
                            }),
                            children: Vec::new(),
                        },
                    ],
                },
                ProviderNativeMenuItem {
                    id: "fixture-open-terminal".to_string(),
                    label: "Open in Fixture Terminal".to_string(),
                    enabled: true,
                    danger: false,
                    canonical_action_kind: None,
                    normalized_verb: Some("openinfixtureterminal".to_string()),
                    icon: Some(NativeMenuIcon {
                        kind: NativeMenuIconKind::DataUrl,
                        data_url: "data:image/png;base64,RkFLRQ==".to_string(),
                        alt: Some("Fixture icon".to_string()),
                    }),
                    invocation: Some(ProviderInvocation::Fake {
                        action_id: "fake.openTerminal".to_string(),
                    }),
                    children: Vec::new(),
                },
                ProviderNativeMenuItem {
                    id: "fixture-copy-duplicate".to_string(),
                    label: "Copy".to_string(),
                    enabled: true,
                    danger: false,
                    canonical_action_kind: Some(NativeMenuCanonicalActionKind::Copy),
                    normalized_verb: Some("copy".to_string()),
                    icon: None,
                    invocation: Some(ProviderInvocation::Fake {
                        action_id: "fake.copy".to_string(),
                    }),
                    children: Vec::new(),
                },
                ProviderNativeMenuItem {
                    id: "fixture-properties-duplicate".to_string(),
                    label: "Properties".to_string(),
                    enabled: true,
                    danger: false,
                    canonical_action_kind: None,
                    normalized_verb: Some("properties".to_string()),
                    icon: None,
                    invocation: Some(ProviderInvocation::Fake {
                        action_id: "fake.properties".to_string(),
                    }),
                    children: Vec::new(),
                },
            ]
        })
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
