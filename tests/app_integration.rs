use lora_chat::app::{App, AppEvent};
use lora_chat::config::Config;
use lora_chat::crypto::keystore::KeyStore;
use lora_chat::device::mock::MockDevice;
use lora_chat::device::types::DeviceEvent;
use lora_chat::gps::MockProvider;
use lora_chat::protocol::{Frame, FrameType, FLAG_ENCRYPTED};
use std::time::Duration;

fn meta() -> lora_chat::device::types::RxMeta {
    lora_chat::device::types::RxMeta {
        rssi: Some(-90),
        snr: Some(5.0),
        received_at: std::time::SystemTime::now(),
    }
}

fn quiet_mock() -> lora_chat::device::mock_data::MockConfig {
    lora_chat::device::mock_data::MockConfig {
        hello_interval_ms: 500,
        ..Default::default()
    }
}

fn test_app(name: &str) -> App {
    let mut cfg = Config::default();
    cfg.identity.name = name.into();
    cfg.identity.node_id = 0x00A0_0001;
    let dir = std::env::temp_dir().join(format!("lora-chat-test-{name}-{}", std::process::id()));
    let _ = std::fs::create_dir_all(&dir);
    let store = lora_chat::storage::MessageStore::open(dir.join("history.jsonl")).expect("store");
    let keys = KeyStore::default();
    App::new(cfg, store, keys, Box::new(MockProvider::default()))
}

#[allow(clippy::field_reassign_with_default)]
#[tokio::test]
async fn hello_frame_registers_node() {
    let mut app = test_app("ALPHA");
    let mut f = Frame::new(FrameType::Hello, 42, 0xBEEF01, 10, b"BETA|BETA".to_vec());
    f.header.timestamp = lora_chat::protocol::now_secs();

    let ev = app.on_frame(meta(), f.to_wire()).await;
    assert!(ev.is_some());
    let node = app.nodes.get(0xBEEF01).expect("node registered");
    assert_eq!(node.name, "BETA");
    assert_eq!(node.short_name, "BETA");
}

#[tokio::test]
async fn duplicate_frames_are_dropped() {
    let mut app = test_app("ALPHA");
    let f = Frame::new(FrameType::Data, 7, 0x111111, 5, b"once".to_vec());
    let wire = f.to_wire();
    let first = app.on_frame(meta(), wire.clone()).await;
    assert!(matches!(first, Some(AppEvent::Message(_))));
    let second = app.on_frame(meta(), wire).await;
    assert!(second.is_none(), "duplicate must be dropped");
}

#[tokio::test]
async fn out_of_order_sequences_accepted() {
    let mut app = test_app("ALPHA");
    let f2 = Frame::new(FrameType::Data, 9, 0x222222, 101, b"b".to_vec());
    let f1 = Frame::new(FrameType::Data, 8, 0x222222, 100, b"a".to_vec());

    assert!(matches!(
        app.on_frame(meta(), f2.to_wire()).await,
        Some(AppEvent::Message(_))
    ));
    assert!(
        matches!(
            app.on_frame(meta(), f1.to_wire()).await,
            Some(AppEvent::Message(_))
        ),
        "in-window older seq accepted"
    );
}

#[tokio::test]
async fn exact_replay_of_whole_frame_is_deduped() {
    let mut app = test_app("ALPHA");
    let f = Frame::new(FrameType::Data, 8, 0x333333, 200, b"dup".to_vec());
    let wire = f.to_wire();
    assert!(matches!(
        app.on_frame(meta(), wire.clone()).await,
        Some(AppEvent::Message(_))
    ));
    assert!(app.on_frame(meta(), wire).await.is_none());
}

#[tokio::test]
async fn encrypted_roundtrip_with_shared_channel_key() {
    use chacha20poly1305::aead::{Aead, KeyInit, Payload};
    let mut alice = test_app("ALICE");
    let mut bob_cfg = Config::default();
    bob_cfg.identity.name = "BOB".into();
    bob_cfg.identity.node_id = 0x00B0_0002;
    let dir = std::env::temp_dir().join(format!("lora-chat-test-bob-{}", std::process::id()));
    let _ = std::fs::create_dir_all(&dir);
    let bob_store = lora_chat::storage::MessageStore::open(dir.join("h.jsonl")).expect("store");
    let mut bob_keys = KeyStore::default();
    let shared = [42u8; 32];
    bob_keys.set_channel_key("General", shared);
    let mut bob = App::new(
        bob_cfg,
        bob_store,
        bob_keys,
        Box::new(MockProvider::default()),
    );

    let mut alice_keys = KeyStore::default();
    alice_keys.set_channel_key("General", shared);
    alice.keys = alice_keys;
    alice.attach_connected_for_test();

    let id = alice
        .send_text(lora_chat::app::SendRequest {
            channel: "General",
            text: "secret hello",
            direct_to: None,
            emergency: false,
            want_ack: true,
        })
        .await
        .expect("send ok");

    let stored = alice.store.all().last().expect("stored");
    assert_eq!(stored.security_label, "Encrypted");
    assert_eq!(stored.id, id);

    let wires = alice.all_sent_wires_for_test(id);
    assert!(!wires.is_empty(), "captured wires");

    for w in &wires {
        let frame = Frame::from_wire(w).expect("parse");
        assert!(frame.header.is_encrypted());
    }

    let mut final_ev = None;
    for w in &wires {
        if let Some(ev) = bob.on_frame(meta(), w.clone()).await {
            final_ev = Some(ev);
        }
    }
    match final_ev.expect("bob should emit message") {
        AppEvent::Message(m) => {
            assert_eq!(m.text, "secret hello");
            assert_eq!(m.security_label, "Encrypted");
        }
        other => panic!("unexpected event {other:?}"),
    }

    let _ = chacha20poly1305::XChaCha20Poly1305::new((&shared).into()).encrypt(
        chacha20poly1305::XNonce::from_slice(&[0u8; 24]),
        Payload {
            msg: b"x",
            aad: b"y",
        },
    );
}

#[tokio::test]
async fn dm_without_key_is_refused() {
    let mut app = test_app("ALPHA");
    app.attach_connected_for_test();
    let err = app
        .send_text(lora_chat::app::SendRequest {
            channel: "General",
            text: "hi",
            direct_to: Some(0x77),
            emergency: false,
            want_ack: false,
        })
        .await
        .expect_err("must refuse");
    assert!(matches!(err, lora_chat::error::Error::PrivacyBlocked(_)));
}

#[tokio::test]
async fn ack_marks_message_delivered() {
    let mut app = test_app("ALPHA");
    app.attach_connected_for_test();
    app.keys.set_channel_key("General", [7u8; 32]);

    let id = app
        .send_text(lora_chat::app::SendRequest {
            channel: "General",
            text: "ack me",
            direct_to: None,
            emergency: false,
            want_ack: true,
        })
        .await
        .expect("send");

    let ev = app.on_device_event(DeviceEvent::Acked(id)).await;
    assert!(matches!(ev, Some(AppEvent::Delivery { status, .. }) if status == "Delivered"));
    let stored = app
        .store
        .all()
        .iter()
        .rev()
        .find(|m| m.id == id)
        .expect("msg");
    assert_eq!(stored.delivery, "Delivered");
    assert!(!app.has_pending_acks());
}

#[tokio::test]
async fn truncated_wire_counts_crc_error() {
    let mut app = test_app("ALPHA");
    let wire = Frame::new(FrameType::Data, 3, 0x55, 1, b"data".to_vec()).to_wire();
    let ev = app.on_frame(meta(), wire[..wire.len() - 1].to_vec()).await;
    assert!(ev.is_some(), "should emit drop notice");
    assert_eq!(app.stats.crc_errors, 1);
}

#[tokio::test]
async fn flipped_header_byte_counts_crc_error() {
    let mut app = test_app("ALPHA");
    let mut wire = Frame::new(FrameType::Data, 3, 0x55, 1, b"data".to_vec()).to_wire();
    wire[1] ^= 0xff;
    let _ = app.on_frame(meta(), wire).await;
    assert!(app.stats.crc_errors + app.stats.dropped >= 1);
}

#[tokio::test]
async fn mock_device_end_to_end_beacons() {
    let (_handle, mut rx) = MockDevice::spawn(crate::quiet_mock());
    tokio::time::timeout(Duration::from_secs(30), async {
        loop {
            match rx.recv().await {
                Some(DeviceEvent::Frame(_, bytes)) => {
                    if let Ok(f) = Frame::from_wire(&bytes) {
                        if f.header.frame_type == FrameType::Hello {
                            return;
                        }
                    }
                }
                Some(_) => {}
                None => panic!("device died"),
            }
        }
    })
    .await
    .expect("hello beacon within timeout");
}

#[test]
fn encrypted_flag_never_set_without_key() {
    let f = Frame::new(FrameType::Data, 1, 2, 3, vec![9]);
    assert_eq!(f.header.flags & FLAG_ENCRYPTED, 0);
}
