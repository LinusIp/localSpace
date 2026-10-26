//! Conversations through Core (architecture v2 §8): several, switchable,
//! persisted across restarts, with the transcript always the current one.

use localspace_core::{Config, Core};
use localspace_proto as proto;
use std::path::Path;

fn core_in(dir: &Path) -> Core {
    let mut cfg = Config::personal("tester");
    cfg.data_dir = Some(dir.to_path_buf());
    Core::new(cfg).expect("creating Core")
}

fn conversations(core: &mut Core) -> (Vec<proto::ConversationSummary>, String) {
    match core.handle(proto::Request::ListConversations) {
        proto::Response::Conversations { list, current } => (list, current),
        other => panic!("expected the conversations, got {other:?}"),
    }
}

fn transcript(core: &mut Core) -> Vec<proto::ChatMessage> {
    match core.handle(proto::Request::GetTranscript) {
        proto::Response::Transcript { messages, .. } => messages,
        other => panic!("expected the transcript, got {other:?}"),
    }
}

#[test]
fn a_message_lands_in_the_current_conversation_and_names_it() {
    let dir = tempfile::tempdir().unwrap();
    let mut core = core_in(dir.path());
    let (list, current) = conversations(&mut core);
    assert!(
        list.is_empty(),
        "a fresh environment has a chat to type in, listed once its first message is sent"
    );
    assert!(!current.is_empty());

    // No model is loaded: the turn still records the message and the reply.
    core.handle(proto::Request::SendMessage {
        text: "What is on the board right now?".into(),
        conversation: None,
        attachments: Vec::new(),
    });
    let (list, current_now) = conversations(&mut core);
    assert_eq!(current_now, current);
    assert_eq!(list[0].id, current);
    assert_eq!(list[0].title, "What is on the board right now?");
    assert_eq!(list[0].messages, 2, "the question and the answer");
}

#[test]
fn switching_conversations_switches_the_transcript_and_survives_a_restart() {
    let dir = tempfile::tempdir().unwrap();
    let first;
    let second;
    {
        let mut core = core_in(dir.path());
        core.handle(proto::Request::SendMessage {
            text: "first conversation".into(),
            conversation: None,
            attachments: Vec::new(),
        });
        first = conversations(&mut core).1;

        match core.handle(proto::Request::NewConversation) {
            proto::Response::Conversations { current, list } => {
                second = current;
                assert_eq!(
                    list.len(),
                    1,
                    "the new chat is listed once its first message is sent"
                );
            }
            other => panic!("{other:?}"),
        }
        assert!(
            transcript(&mut core).is_empty(),
            "a new conversation starts empty"
        );
        core.handle(proto::Request::SendMessage {
            text: "second conversation".into(),
            conversation: None,
            attachments: Vec::new(),
        });
        assert_eq!(transcript(&mut core)[0].content, "second conversation");

        assert!(matches!(
            core.handle(proto::Request::SelectConversation { id: first.clone() }),
            proto::Response::Conversations { .. }
        ));
        assert_eq!(transcript(&mut core)[0].content, "first conversation");
    }

    // A new process, the same data directory.
    let mut core = core_in(dir.path());
    let (list, current) = conversations(&mut core);
    assert_eq!(list.len(), 2);
    assert_eq!(current, first, "the selection persists too");
    assert_eq!(transcript(&mut core)[0].content, "first conversation");
    assert!(
        list.iter()
            .any(|c| c.id == second && c.title == "second conversation")
    );

    // Deleting the current one moves to the other.
    match core.handle(proto::Request::DeleteConversation { id: first }) {
        proto::Response::Conversations { current, list } => {
            assert_eq!(current, second);
            assert_eq!(list.len(), 1);
        }
        other => panic!("{other:?}"),
    }
    assert_eq!(transcript(&mut core)[0].content, "second conversation");

    match core.handle(proto::Request::RenameConversation {
        id: second.clone(),
        title: "Renamed".into(),
    }) {
        proto::Response::Conversations { list, .. } => assert_eq!(list[0].title, "Renamed"),
        other => panic!("{other:?}"),
    }
}

/// A text file attached to a message is kept with its chat as UTF-8, can be
/// fetched by its person as plain text, and is deleted with the chat
/// (docs/DECISIONS.md, 2026-10-26).
#[test]
fn an_attached_text_file_is_kept_with_its_chat_and_deleted_with_it() {
    let dir = tempfile::tempdir().unwrap();
    let mut core = core_in(dir.path());
    let utf16: Vec<u8> = [0xFF, 0xFE]
        .into_iter()
        .chain(
            "Oʻzbek tili: sanalar, raqamlar\n"
                .encode_utf16()
                .flat_map(u16::to_le_bytes),
        )
        .collect();
    let attachment = match core.handle(proto::Request::AddAttachment {
        name: "notes.md".into(),
        bytes: utf16,
    }) {
        proto::Response::Attached { attachment } => attachment,
        other => panic!("expected the file taken, got {other:?}"),
    };
    assert_eq!(attachment.kind, proto::AttachmentKind::Text);
    core.handle(proto::Request::SendMessage {
        text: "Summarise this.".into(),
        conversation: None,
        attachments: vec![attachment.id.clone()],
    });
    assert_eq!(transcript(&mut core)[0].attachments[0].id, attachment.id);
    match core.handle(proto::Request::GetAttachment {
        id: attachment.id.clone(),
    }) {
        proto::Response::DocBlob { mime, bytes, .. } => {
            assert_eq!(mime, "text/plain");
            assert_eq!(
                String::from_utf8(bytes).unwrap(),
                "Oʻzbek tili: sanalar, raqamlar\n"
            );
        }
        other => panic!("expected the file, got {other:?}"),
    }
    let chat = conversations(&mut core).1;
    core.handle(proto::Request::DeleteConversation { id: chat });
    assert!(matches!(
        core.handle(proto::Request::GetAttachment { id: attachment.id }),
        proto::Response::AttachmentRefused {
            refusal: proto::AttachmentRefusal::NotFound
        }
    ));
}

/// A picture attached to a message is kept with its chat, survives a
/// restart, can be fetched by its person, and is deleted with the chat
/// (docs/DECISIONS.md, 2026-10-18).
#[test]
fn an_attached_picture_is_kept_with_its_chat_and_deleted_with_it() {
    let dir = tempfile::tempdir().unwrap();
    let picture = include_bytes!("../../localspace-cli/assets/measure-picture.png").to_vec();
    let (id, chat) = {
        let mut core = core_in(dir.path());
        let attachment = match core.handle(proto::Request::AddAttachment {
            name: "circle.png".into(),
            bytes: picture,
        }) {
            proto::Response::Attached { attachment } => attachment,
            other => panic!("expected the picture taken, got {other:?}"),
        };
        assert_eq!(attachment.kind, proto::AttachmentKind::Picture);
        assert_eq!(attachment.name, "circle.png");
        core.handle(proto::Request::SendMessage {
            text: "What is in this picture?".into(),
            conversation: None,
            attachments: vec![attachment.id.clone()],
        });
        let said = transcript(&mut core);
        assert_eq!(said[0].attachments, vec![attachment.clone()]);
        (attachment.id, conversations(&mut core).1)
    };

    // After a restart the message still carries it, and its bytes are there.
    let mut core = core_in(dir.path());
    assert_eq!(transcript(&mut core)[0].attachments[0].id, id);
    match core.handle(proto::Request::GetAttachment { id: id.clone() }) {
        proto::Response::DocBlob { mime, bytes, .. } => {
            assert_eq!(mime, "image/png");
            assert!(bytes.starts_with(b"\x89PNG"));
        }
        other => panic!("expected the picture, got {other:?}"),
    }

    // Deleting the chat deletes the picture.
    core.handle(proto::Request::DeleteConversation { id: chat });
    assert!(matches!(
        core.handle(proto::Request::GetAttachment { id }),
        proto::Response::AttachmentRefused {
            refusal: proto::AttachmentRefusal::NotFound
        }
    ));
}

/// A file that is neither a picture nor text is refused, in parts the
/// client can say in the person's language; an id that was never added
/// cannot be sent.
#[test]
fn a_file_that_is_neither_a_picture_nor_text_and_an_unknown_id_are_refused() {
    let dir = tempfile::tempdir().unwrap();
    let mut core = core_in(dir.path());
    let binary: Vec<u8> = (0..600u32).map(|i| (i * 7 % 256) as u8).collect();
    assert!(matches!(
        core.handle(proto::Request::AddAttachment {
            name: "archive.zip".into(),
            bytes: binary,
        }),
        proto::Response::AttachmentRefused {
            refusal: proto::AttachmentRefusal::NotText
        }
    ));
    assert!(matches!(
        core.handle(proto::Request::SendMessage {
            text: "and this?".into(),
            conversation: None,
            attachments: vec!["0123456789abcdef0123456789abcdef".into()],
        }),
        proto::Response::AttachmentRefused {
            refusal: proto::AttachmentRefusal::NotFound
        }
    ));
}
