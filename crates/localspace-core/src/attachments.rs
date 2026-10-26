//! What people attach to their messages, kept on this computer with the
//! chat and deleted with it (docs/DECISIONS.md, 2026-10-18; plan 38, P1).
//!
//! A file is made ready when it is added (a picture: see [`crate::picture`];
//! a text file is kept as UTF-8, see [`crate::text_file`]) and only that
//! copy is kept, named by its hash: under the person's own
//! folder, in `pending` until it goes with a message, then in the chat's
//! folder. A person reaches only their own folder, so an id from someone
//! else's chat is simply not there. What was added and never sent is
//! cleared when Core next starts.
//!
//! ```text
//! <data>/attachments/<person>/pending/<id>.<png|jpg|txt>   added, not yet sent
//! <data>/attachments/<person>/<chat>/<id>.<png|jpg|txt>    sent with a message
//! <data>/attachments/<person>/.../<id>.json            what it is (name, size)
//! ```

use std::path::{Path, PathBuf};

use localspace_proto as proto;

use crate::{picture, text_file};

/// The most attachments (pictures and text files together) one message takes.
pub const MOST_PER_MESSAGE: usize = 4;

const PENDING: &str = "pending";

pub struct Attachments {
    root: PathBuf,
}

/// A folder name for a person or a chat: their id as it is when it is plain,
/// its hash when it holds anything a path should not.
fn folder(name: &str) -> String {
    let plain = !name.is_empty()
        && name.len() <= 64
        && name
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_');
    if plain {
        name.to_string()
    } else {
        blake3::hash(name.as_bytes()).to_hex()[..32].to_string()
    }
}

/// An id as Core names one: 32 lower-case hex digits, and nothing else, so
/// that an id from a request can never reach outside its folder.
fn is_id(id: &str) -> bool {
    id.len() == 32
        && id
            .chars()
            .all(|c| c.is_ascii_hexdigit() && !c.is_ascii_uppercase())
}

fn extension(mime: &str) -> &'static str {
    match mime {
        "image/jpeg" => "jpg",
        "text/plain" => "txt",
        _ => "png",
    }
}

/// A text file is at most 2 MB: a book's worth, more than any model here
/// reads at once (docs/DECISIONS.md, 2026-10-26).
pub const TEXT_MAX_BYTES: usize = 2 * 1024 * 1024;

/// Pictures are told by their first bytes, or by their name when those are
/// not readable; everything else is tried as text.
fn is_picture(name: &str, bytes: &[u8]) -> bool {
    let png = bytes.starts_with(&[0x89, b'P', b'N', b'G']);
    let jpeg = bytes.starts_with(&[0xFF, 0xD8, 0xFF]);
    let webp = bytes.len() >= 12 && &bytes[..4] == b"RIFF" && &bytes[8..12] == b"WEBP";
    let named = name
        .rsplit_once('.')
        .map(|(_, ext)| ext.to_ascii_lowercase())
        .is_some_and(|ext| matches!(ext.as_str(), "png" | "jpg" | "jpeg" | "webp"));
    png || jpeg || webp || named
}

impl Attachments {
    /// Kept under `root`; what was added and never sent is cleared now.
    pub fn new(root: PathBuf) -> Attachments {
        let kept = Attachments { root };
        kept.clear_pending();
        kept
    }

    fn person(&self, user: &str) -> PathBuf {
        self.root.join(folder(user))
    }

    fn clear_pending(&self) {
        let Ok(people) = std::fs::read_dir(&self.root) else {
            return;
        };
        for person in people.flatten() {
            let pending = person.path().join(PENDING);
            if pending.is_dir()
                && let Err(e) = std::fs::remove_dir_all(&pending)
            {
                tracing::warn!("could not clear {}: {e}", pending.display());
            }
        }
    }

    /// A file the person attaches: made ready, and kept until it is sent.
    pub fn add(
        &self,
        user: &str,
        name: &str,
        bytes: &[u8],
    ) -> Result<proto::Attachment, proto::AttachmentRefusal> {
        if !is_picture(name, bytes) {
            return self.add_text(user, name, bytes);
        }
        let ready = picture::prepare(bytes).map_err(|refused| match refused {
            picture::Refused::TooLarge { megabytes } => {
                proto::AttachmentRefusal::TooLarge { megabytes }
            }
            picture::Refused::TooManyPixels { width, height } => {
                proto::AttachmentRefusal::TooManyPixels { width, height }
            }
            picture::Refused::NotAPicture => proto::AttachmentRefusal::NotAPicture,
            picture::Refused::Unreadable(why) => {
                tracing::info!("an attached picture could not be read: {why}");
                proto::AttachmentRefusal::Unreadable
            }
        })?;
        let id = blake3::hash(&ready.bytes).to_hex()[..32].to_string();
        let attachment = proto::Attachment {
            id,
            kind: proto::AttachmentKind::Picture,
            name: name.chars().take(200).collect(),
            mime: ready.mime.to_string(),
            bytes: ready.bytes.len() as u64,
            width: ready.width,
            height: ready.height,
            text: None,
        };
        self.keep_pending(user, &attachment, &ready.bytes)?;
        Ok(attachment)
    }

    /// A text file: read in its encoding, kept as UTF-8.
    fn add_text(
        &self,
        user: &str,
        name: &str,
        bytes: &[u8],
    ) -> Result<proto::Attachment, proto::AttachmentRefusal> {
        if bytes.len() > TEXT_MAX_BYTES {
            return Err(proto::AttachmentRefusal::TextTooLarge {
                megabytes: (bytes.len() as u64).div_ceil(1024 * 1024),
            });
        }
        let decoded = text_file::decode(bytes).map_err(|why| match why {
            text_file::NotText::Empty => proto::AttachmentRefusal::EmptyFile,
            text_file::NotText::Binary => proto::AttachmentRefusal::NotText,
        })?;
        let kept = decoded.text.into_bytes();
        let text = String::from_utf8_lossy(&kept);
        let whole = text_file::read_within(&text, usize::MAX);
        let attachment = proto::Attachment {
            id: blake3::hash(&kept).to_hex()[..32].to_string(),
            kind: proto::AttachmentKind::Text,
            name: name.chars().take(200).collect(),
            mime: "text/plain".into(),
            bytes: kept.len() as u64,
            width: 0,
            height: 0,
            text: Some(proto::TextFile {
                encoding: decoded.encoding.name().into(),
                chars: whole.chars,
                words: whole.words,
                read_chars: whole.chars,
                read_words: whole.words,
                read_tokens: whole.tokens,
            }),
        };
        self.keep_pending(user, &attachment, &kept)?;
        Ok(attachment)
    }

    fn keep_pending(
        &self,
        user: &str,
        attachment: &proto::Attachment,
        bytes: &[u8],
    ) -> Result<(), proto::AttachmentRefusal> {
        let dir = self.person(user).join(PENDING);
        let written = std::fs::create_dir_all(&dir)
            .and_then(|_| std::fs::write(dir.join(file_of(attachment)), bytes))
            .and_then(|_| {
                std::fs::write(
                    dir.join(format!("{}.json", attachment.id)),
                    serde_json::to_vec(attachment).unwrap_or_default(),
                )
            });
        if let Err(e) = written {
            tracing::warn!("could not keep an attachment: {e}");
            return Err(proto::AttachmentRefusal::Unreadable);
        }
        Ok(())
    }

    /// The person's pending attachments named by `ids`, as they are, without
    /// taking them: so a message can be refused before anything moves.
    pub fn pending(&self, user: &str, ids: &[String]) -> Vec<proto::Attachment> {
        let pending = self.person(user).join(PENDING);
        ids.iter()
            .filter_map(|id| read_meta(&pending, id))
            .collect()
    }

    /// A kept text file of the chat, as text.
    pub fn text(
        &self,
        user: &str,
        conversation: &str,
        attachment: &proto::Attachment,
    ) -> Option<String> {
        if attachment.kind != proto::AttachmentKind::Text {
            return None;
        }
        self.bytes(user, conversation, attachment)
            .map(|bytes| String::from_utf8_lossy(&bytes).into_owned())
    }

    /// What is known of one of the chat's attachments changed (how much of a
    /// text file the model reads): its record is written again.
    pub fn update(&self, user: &str, conversation: &str, attachment: &proto::Attachment) {
        if !is_id(&attachment.id) {
            return;
        }
        let path = self
            .person(user)
            .join(folder(conversation))
            .join(format!("{}.json", attachment.id));
        if let Err(e) = std::fs::write(&path, serde_json::to_vec(attachment).unwrap_or_default()) {
            tracing::warn!("could not update an attachment's record: {e}");
        }
    }

    /// The attachments named by `ids`, moved from the person's pending ones
    /// into the chat's folder as they go with a message.
    pub fn take(
        &self,
        user: &str,
        conversation: &str,
        ids: &[String],
    ) -> Result<Vec<proto::Attachment>, proto::AttachmentRefusal> {
        if ids.len() > MOST_PER_MESSAGE {
            return Err(proto::AttachmentRefusal::TooMany {
                most: MOST_PER_MESSAGE as u32,
            });
        }
        let pending = self.person(user).join(PENDING);
        let chat = self.person(user).join(folder(conversation));
        let mut taken = Vec::new();
        for id in ids {
            let attachment = read_meta(&pending, id).ok_or(proto::AttachmentRefusal::NotFound)?;
            taken.push(attachment);
        }
        std::fs::create_dir_all(&chat).map_err(|_| proto::AttachmentRefusal::NotFound)?;
        for a in &taken {
            for name in [file_of(a), format!("{}.json", a.id)] {
                if let Err(e) = std::fs::rename(pending.join(&name), chat.join(&name)) {
                    tracing::warn!("could not move an attachment into its chat: {e}");
                    return Err(proto::AttachmentRefusal::NotFound);
                }
            }
        }
        Ok(taken)
    }

    /// The kept bytes of one of the chat's attachments, for the prompt.
    pub fn bytes(
        &self,
        user: &str,
        conversation: &str,
        attachment: &proto::Attachment,
    ) -> Option<Vec<u8>> {
        if !is_id(&attachment.id) {
            return None;
        }
        std::fs::read(
            self.person(user)
                .join(folder(conversation))
                .join(file_of(attachment)),
        )
        .ok()
    }

    /// One of the person's own attachments, pending or sent, with its bytes:
    /// never anyone else's.
    pub fn find(&self, user: &str, id: &str) -> Option<(proto::Attachment, Vec<u8>)> {
        if !is_id(id) {
            return None;
        }
        let person = self.person(user);
        let dirs = std::fs::read_dir(&person).ok()?;
        for dir in dirs.flatten().map(|d| d.path()).filter(|p| p.is_dir()) {
            if let Some(a) = read_meta(&dir, id)
                && let Ok(bytes) = std::fs::read(dir.join(file_of(&a)))
            {
                return Some((a, bytes));
            }
        }
        None
    }

    /// A chat being deleted: its attachments go with it.
    pub fn forget(&self, user: &str, conversation: &str) {
        let chat = self.person(user).join(folder(conversation));
        if chat.is_dir()
            && let Err(e) = std::fs::remove_dir_all(&chat)
        {
            tracing::warn!("could not delete a chat's attachments: {e}");
        }
    }
}

fn file_of(a: &proto::Attachment) -> String {
    format!("{}.{}", a.id, extension(&a.mime))
}

fn read_meta(dir: &Path, id: &str) -> Option<proto::Attachment> {
    if !is_id(id) {
        return None;
    }
    let text = std::fs::read(dir.join(format!("{id}.json"))).ok()?;
    serde_json::from_slice(&text)
        .ok()
        .filter(|a: &proto::Attachment| a.id == id)
}

#[cfg(test)]
mod tests {
    use super::*;
    use image::{DynamicImage, ImageFormat, Rgb, RgbImage};
    use std::io::Cursor;

    fn png(width: u32, height: u32) -> Vec<u8> {
        let mut out = Vec::new();
        DynamicImage::ImageRgb8(RgbImage::from_pixel(width, height, Rgb([10, 20, 30])))
            .write_to(&mut Cursor::new(&mut out), ImageFormat::Png)
            .expect("a test picture");
        out
    }

    #[test]
    fn a_picture_is_kept_pending_then_with_its_chat_then_gone_with_it() {
        let dir = tempfile::tempdir().expect("a folder");
        let kept = Attachments::new(dir.path().to_path_buf());
        let a = kept.add("anna", "screen.png", &png(40, 30)).expect("taken");
        assert_eq!((a.width, a.height, a.mime.as_str()), (40, 30, "image/png"));
        assert!(kept.find("anna", &a.id).is_some(), "pending, and findable");

        let taken = kept
            .take("anna", "c_1", std::slice::from_ref(&a.id))
            .expect("sent");
        assert_eq!(taken, vec![a.clone()]);
        assert!(kept.bytes("anna", "c_1", &a).is_some());
        // Taken once: no longer pending.
        assert_eq!(
            kept.take("anna", "c_2", std::slice::from_ref(&a.id))
                .unwrap_err(),
            proto::AttachmentRefusal::NotFound
        );

        kept.forget("anna", "c_1");
        assert!(kept.find("anna", &a.id).is_none(), "deleted with the chat");
        assert!(!dir.path().join("anna").join("c_1").exists());
    }

    #[test]
    fn a_person_never_reaches_another_persons_attachment() {
        let dir = tempfile::tempdir().expect("a folder");
        let kept = Attachments::new(dir.path().to_path_buf());
        let a = kept.add("anna", "screen.png", &png(8, 8)).expect("taken");
        assert!(kept.find("bek", &a.id).is_none());
        assert_eq!(
            kept.take("bek", "c_1", std::slice::from_ref(&a.id))
                .unwrap_err(),
            proto::AttachmentRefusal::NotFound
        );
        // An id that is not one of Core's never reaches the disk.
        assert!(kept.find("anna", "../../anna/pending/x").is_none());
    }

    #[test]
    fn what_was_never_sent_is_cleared_at_the_next_start() {
        let dir = tempfile::tempdir().expect("a folder");
        let a = Attachments::new(dir.path().to_path_buf())
            .add("anna", "screen.png", &png(8, 8))
            .expect("taken");
        let again = Attachments::new(dir.path().to_path_buf());
        assert!(again.find("anna", &a.id).is_none());
    }

    #[test]
    fn more_than_four_with_one_message_are_refused() {
        let dir = tempfile::tempdir().expect("a folder");
        let kept = Attachments::new(dir.path().to_path_buf());
        let ids: Vec<String> = (0..5)
            .map(|i| kept.add("anna", "p.png", &png(8 + i, 8)).expect("taken").id)
            .collect();
        assert_eq!(
            kept.take("anna", "c_1", &ids).unwrap_err(),
            proto::AttachmentRefusal::TooMany { most: 4 }
        );
    }

    #[test]
    fn a_refused_file_says_why_in_parts() {
        let dir = tempfile::tempdir().expect("a folder");
        let kept = Attachments::new(dir.path().to_path_buf());
        let binary: Vec<u8> = (0..600u32).map(|i| (i * 7 % 256) as u8).collect();
        assert_eq!(
            kept.add("anna", "data.bin", &binary).unwrap_err(),
            proto::AttachmentRefusal::NotText
        );
        assert_eq!(
            kept.add("anna", "empty.txt", b" \n ").unwrap_err(),
            proto::AttachmentRefusal::EmptyFile
        );
        let large = vec![b'a'; TEXT_MAX_BYTES + 1];
        assert_eq!(
            kept.add("anna", "big.txt", &large).unwrap_err(),
            proto::AttachmentRefusal::TextTooLarge { megabytes: 3 }
        );
        // Named as a picture, it is read as one, and refused as one.
        assert_eq!(
            kept.add("anna", "notes.png", b"just words").unwrap_err(),
            proto::AttachmentRefusal::NotAPicture
        );
    }

    #[test]
    fn a_text_file_is_kept_as_utf8_with_how_it_was_read() {
        let dir = tempfile::tempdir().expect("a folder");
        let kept = Attachments::new(dir.path().to_path_buf());
        // "Счёт № 7" in Windows-1251.
        let file = kept
            .add("anna", "счёт.txt", b"\xd1\xf7\xb8\xf2 \xb9 7\n")
            .expect("taken");
        assert_eq!(file.kind, proto::AttachmentKind::Text);
        assert_eq!(file.mime, "text/plain");
        let facts = file.text.clone().expect("a text file's facts");
        assert_eq!(facts.encoding, "Windows-1251");
        assert_eq!((facts.chars, facts.words), (9, 3));
        assert_eq!(facts.read_chars, facts.chars, "all of it, until it is sent");
        let taken = kept.take("anna", "c_1", &[file.id.clone()]).expect("taken");
        assert_eq!(
            kept.text("anna", "c_1", &taken[0]).as_deref(),
            Some("Счёт № 7\n")
        );
        assert!(
            dir.path()
                .join("anna")
                .join("c_1")
                .join(format!("{}.txt", file.id))
                .is_file()
        );
    }

    #[test]
    fn people_and_chats_with_unusual_names_get_safe_folders() {
        assert_eq!(folder("anna"), "anna");
        assert_eq!(folder("c_1"), "c_1");
        let odd = folder("bek@example.com/../x");
        assert_eq!(odd.len(), 32);
        assert!(odd.chars().all(|c| c.is_ascii_hexdigit()));
    }
}
