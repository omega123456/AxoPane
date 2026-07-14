use file_explorer_lib::ipc::{events, types::*};

#[test]
fn thumbnail_contract_serializes_context_and_event_name() {
    let request = RequestThumbnailsRequest {
        pane_id: "left".into(),
        tab_id: "tab".into(),
        path: "/folder".into(),
        generation: 4,
        revision: 2,
        candidates: vec![ThumbnailCandidateRequest {
            path: "/folder/image.png".into(),
            modified_unix_seconds: 2,
            size_bytes: 3,
            is_directory: false,
            priority: ThumbnailPriority::Visible,
            order: 0,
        }],
    };
    let value = serde_json::to_value(request).expect("serialize");
    assert_eq!(value["generation"], 4);
    assert_eq!(value["candidates"][0]["modifiedUnixSeconds"], 2);
    assert_eq!(events::THUMBNAIL_STATE, "thumbnail://state");
    let cancel = CancelThumbnailsRequest {
        pane_id: "left".into(),
        tab_id: "tab".into(),
        path: "/folder".into(),
        generation: 5,
    };
    assert_eq!(
        serde_json::to_value(cancel).expect("serialize")["generation"],
        5
    );
}
