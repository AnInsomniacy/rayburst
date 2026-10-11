use super::*;
use crate::database::Database;
use serde_json::json;
use std::sync::Arc;

#[test]
fn filename_input_cannot_escape_the_selected_directory() {
    for name in [
        "../file",
        "/Upload/file",
        "C:\\file",
        "C:file",
        "file:stream",
        "..",
        "CON.txt",
        "NUL",
        "trailing.",
        "bad\nname",
    ] {
        assert!(validate_filename(name).is_err(), "{name:?}");
    }
    for name in ["", "report final.zip", "文件.zip", "COM10.txt"] {
        assert!(validate_filename(name).is_ok(), "{name:?}");
    }
}

fn request(mime: Option<&str>, size: Option<i64>) -> AddRequest {
    serde_json::from_value(json!({"url":"https://example.test/file.zip","verify":true,
        "mime":mime,"size":size}))
    .unwrap()
}

fn probe(status: &str, content_type: &str, length: Option<&str>) -> Aria2Probe {
    serde_json::from_value(
        json!({"status":status,"finalUrl":"https://example.test/file.zip",
        "contentType":content_type,"totalLength":length,"resumable":"true"}),
    )
    .unwrap()
}

#[test]
fn the_browser_keeps_downloads_the_engine_would_not_receive_identically() {
    let zip = request(Some("application/zip"), Some(1000));
    assert_eq!(
        refusal(&probe("206", "application/zip", Some("1000")), &zip),
        None
    );
    assert_eq!(refusal(&probe("200", "", None), &zip), None);
    assert_eq!(
        refusal(&probe("0", "", None), &zip),
        Some(FallbackReason::Unreachable)
    );
    assert_eq!(
        refusal(&probe("403", "text/html", None), &zip),
        Some(FallbackReason::Rejected)
    );
    let mut failed = probe("206", "", Some("1000"));
    failed.error = Some("transfer closed".into());
    assert_eq!(refusal(&failed, &zip), Some(FallbackReason::Rejected));
    assert_eq!(
        refusal(
            &probe("200", "text/html; charset=utf-8", Some("1000")),
            &zip
        ),
        Some(FallbackReason::WebPage)
    );
    assert_eq!(
        refusal(&probe("206", "application/zip", Some("999")), &zip),
        Some(FallbackReason::SizeMismatch)
    );
    // A page the browser was downloading as a page is still that page.
    let page = request(Some("text/html"), None);
    assert_eq!(refusal(&probe("200", "TEXT/HTML", None), &page), None);
    // Unknown sizes never mismatch.
    let unknown = request(None, Some(-1));
    assert_eq!(
        refusal(&probe("206", "application/zip", Some("5")), &unknown),
        None
    );
}

fn recorder() -> (Arc<std::sync::Mutex<Vec<String>>>, handoff::OnWithdraw) {
    let withdrawn = Arc::new(std::sync::Mutex::new(Vec::new()));
    let sink = withdrawn.clone();
    (
        withdrawn,
        Box::new(move |id: &str| sink.lock().unwrap().push(id.to_owned())),
    )
}

#[tokio::test]
async fn the_dialog_answers_a_held_download_exactly_once() {
    let handoffs = Handoffs::default();

    let (withdrawn, on_withdraw) = recorder();
    let pending = handoffs.open(on_withdraw);
    let id = pending.id().to_owned();
    let claim = handoffs.claim(&id).unwrap();
    assert!(matches!(handoffs.claim(&id), Err(AppError::Conflict(_))));
    // A claimed request ignores cancellation; its task decides.
    handoffs.cancel(&id);
    claim.finish("gid".into());
    assert!(matches!(
        pending.wait(Duration::from_secs(1)).await,
        Some(handoff::Outcome::Added(gid)) if gid == "gid"
    ));
    assert!(matches!(handoffs.claim(&id), Err(AppError::NotFound(_))));
    assert!(withdrawn.lock().unwrap().is_empty());

    let (_, on_withdraw) = recorder();
    let pending = handoffs.open(on_withdraw);
    let id = pending.id().to_owned();
    // A failed creation returns the request to the dialog.
    drop(handoffs.claim(&id).unwrap());
    handoffs.cancel(&id);
    assert!(matches!(
        pending.wait(Duration::from_secs(1)).await,
        Some(handoff::Outcome::Cancelled)
    ));
}

#[tokio::test]
async fn unanswered_or_abandoned_requests_go_back_to_the_browser() {
    let handoffs = Handoffs::default();

    let (withdrawn, on_withdraw) = recorder();
    let pending = handoffs.open(on_withdraw);
    let id = pending.id().to_owned();
    assert!(pending.wait(Duration::from_millis(10)).await.is_none());
    assert_eq!(*withdrawn.lock().unwrap(), std::slice::from_ref(&id));
    assert!(matches!(handoffs.claim(&id), Err(AppError::NotFound(_))));

    // The extension disconnected: dropping the request withdraws it.
    let (withdrawn, on_withdraw) = recorder();
    let id = handoffs.open(on_withdraw).id().to_owned();
    assert_eq!(*withdrawn.lock().unwrap(), std::slice::from_ref(&id));
    assert!(handoffs.claim(&id).is_err());

    // A task being created when time runs out still answers.
    let (withdrawn, on_withdraw) = recorder();
    let pending = handoffs.open(on_withdraw);
    let id = pending.id().to_owned();
    let claim = handoffs.claim(&id).unwrap();
    let finish = async {
        tokio::time::sleep(Duration::from_millis(50)).await;
        claim.finish("late".into());
    };
    let (outcome, ()) = tokio::join!(pending.wait(Duration::from_millis(10)), finish);
    assert!(matches!(outcome, Some(handoff::Outcome::Added(gid)) if gid == "late"));
    assert!(withdrawn.lock().unwrap().is_empty());
}

#[tokio::test]
async fn saved_credentials_apply_only_to_one_origin_and_preserve_explicit_auth() {
    let db = Database::open_in_memory().unwrap();
    db.save_credential("https://example.test", "saved", "literal%20")
        .await
        .unwrap();
    let mut options = json!({});
    native::apply_saved_credentials(&db, &["https://example.test/file".into()], &mut options)
        .await
        .unwrap();
    assert_eq!(options["http-passwd"], "literal%20");
    let mut explicit = json!({"http-user":"manual"});
    native::apply_saved_credentials(&db, &["https://example.test/file".into()], &mut explicit)
        .await
        .unwrap();
    assert_eq!(explicit, json!({"http-user":"manual"}));
    let mut mirrors = json!({});
    native::apply_saved_credentials(
        &db,
        &[
            "https://example.test/file".into(),
            "https://other.test/file".into(),
        ],
        &mut mirrors,
    )
    .await
    .unwrap();
    assert_eq!(mirrors, json!({}));
}
