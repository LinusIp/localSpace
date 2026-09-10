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
        proto::Response::Transcript { messages } => messages,
        other => panic!("expected the transcript, got {other:?}"),
    }
}

#[test]
fn a_message_lands_in_the_current_conversation_and_names_it() {
    let dir = tempfile::tempdir().unwrap();
    let mut core = core_in(dir.path());
    let (list, current) = conversations(&mut core);
    assert_eq!(list.len(), 1, "a fresh environment has one conversation");
    assert_eq!(list[0].id, current);
    assert_eq!(list[0].title, "New chat");

    // No model is loaded: the turn still records the message and the reply.
    core.handle(proto::Request::SendMessage {
        text: "What is on the board right now?".into(),
    });
    let (list, _) = conversations(&mut core);
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
        });
        first = conversations(&mut core).1;

        match core.handle(proto::Request::NewConversation) {
            proto::Response::Conversations { current, list } => {
                second = current;
                assert_eq!(list.len(), 2);
            }
            other => panic!("{other:?}"),
        }
        assert!(
            transcript(&mut core).is_empty(),
            "a new conversation starts empty"
        );
        core.handle(proto::Request::SendMessage {
            text: "second conversation".into(),
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
