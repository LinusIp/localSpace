//! The model's answer, read as it is written on a connection Core holds.
//!
//! An answer comes as server-sent events over HTTP/1.1. Until 2026-09-23
//! `ureq` read them, and it could do neither of two things a person needs:
//! stop part-way, and tell a model that has gone quiet from one that is still
//! writing. Its time limits are totals, so a slow model's long answer was cut
//! off while it was being written ("reading the model's stream: timeout:
//! global", a tester's 14B at three words a second), and a Stop waited for
//! the answer to end. Here the socket is Core's, read in waits of a fifth of
//! a second: [`Stop`] ends the answer within one and shuts the socket, which
//! also tells the engine to stop writing; the limits are silences
//! ([`Silence`]), not totals.
//!
//! HTTP/1.1 is spoken for this one purpose: a POST to a model's endpoint and
//! its answer read as it comes, chunked or not. The events are read line by
//! line from buffered bytes ([`read_sse`]), so a character whose bytes
//! arrive in separate reads is decoded whole.

use crate::model::{ChatReply, read_sse};
use anyhow::{Context, Result, bail};
use std::cell::Cell;
use std::io::{self, BufRead, BufReader, Read, Write};
use std::net::{Shutdown, TcpStream, ToSocketAddrs};
use std::rc::Rc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

/// How long connecting to the model may take: it is on this computer, or on
/// the organisation's network.
const CONNECT: Duration = Duration::from_secs(10);
/// How long sending the question may take.
const SEND: Duration = Duration::from_secs(30);
/// What a response's head may come to, and what a refusal's body is read to.
const HEAD_BYTES: usize = 64 * 1024;
const REFUSAL_BYTES: u64 = 64 * 1024;
/// How long one wait for the model's next bytes lasts before Stop and the
/// silence are looked at again. On Windows a socket shut from another thread
/// does not wake a read already waiting on it; this does, everywhere.
const POLL: Duration = Duration::from_millis(200);

/// How long a model may be silent before its answer is taken to have stopped.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Silence {
    /// Before the first word, while the engine reads the prompt: long on a
    /// slow computer with a long conversation.
    pub before_first_word: Duration,
    pub between_words: Duration,
}

impl Silence {
    /// Ruled on 2026-09-23 (docs/DECISIONS.md, the answers to the estimates).
    pub const ANSWER: Silence = Silence {
        before_first_word: Duration::from_secs(120),
        between_words: Duration::from_secs(60),
    };
}

/// Why an answer ended before the model finished it. What came of it is kept
/// by whoever was reading it.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum Cut {
    #[error("stopped")]
    Stopped,
    #[error("the model wrote nothing for {0} s before its first word")]
    SilentBeforeFirstWord(u64),
    #[error("the model wrote nothing for {0} s after its last word")]
    SilentBetweenWords(u64),
    #[error("the connection to the model ended before the answer did")]
    Lost,
}

/// Stops an answer from another thread. Every reader looks at it at each
/// piece, and the reader here between its short waits on a silent model; a
/// socket held here is shut, so that the engine stops writing.
#[derive(Clone, Default)]
pub struct Stop {
    inner: Arc<StopState>,
}

#[derive(Default)]
struct StopState {
    asked: AtomicBool,
    socket: Mutex<Option<TcpStream>>,
}

impl Stop {
    pub fn stop(&self) {
        self.inner.asked.store(true, Ordering::SeqCst);
        if let Some(socket) = self.inner.socket.lock().unwrap().as_ref() {
            let _ = socket.shutdown(Shutdown::Both);
        }
    }

    pub fn asked(&self) -> bool {
        self.inner.asked.load(Ordering::SeqCst)
    }

    /// Hold `socket`, so that a stop shuts it; one asked for already shuts
    /// it now.
    fn hold(&self, socket: &TcpStream) -> io::Result<()> {
        let mut held = self.inner.socket.lock().unwrap();
        *held = Some(socket.try_clone()?);
        if self.asked() {
            let _ = socket.shutdown(Shutdown::Both);
        }
        Ok(())
    }

    fn let_go(&self) {
        self.inner.socket.lock().unwrap().take();
    }
}

/// POST `body` to `url` (`http://host:port/path`) and read the answer's
/// events as they come, each piece of text to `on_delta` as it arrives.
/// An answer that ends early is an error whose cause is a [`Cut`].
pub fn post_and_read(
    url: &str,
    key: Option<&str>,
    body: &serde_json::Value,
    silence: Silence,
    stop: &Stop,
    on_delta: &mut dyn FnMut(&str),
) -> Result<ChatReply> {
    let (host, port, path) = split_url(url)?;
    let address = (host.as_str(), port)
        .to_socket_addrs()
        .with_context(|| format!("finding {host}"))?
        .next()
        .with_context(|| format!("{host} has no address"))?;
    let socket = TcpStream::connect_timeout(&address, CONNECT)
        .with_context(|| format!("connecting to {host}:{port}"))?;
    stop.hold(&socket)?;
    let answer = exchange(
        &socket, &host, port, &path, key, body, silence, stop, on_delta,
    );
    stop.let_go();
    answer
}

#[allow(clippy::too_many_arguments)]
fn exchange(
    socket: &TcpStream,
    host: &str,
    port: u16,
    path: &str,
    key: Option<&str>,
    body: &serde_json::Value,
    silence: Silence,
    stop: &Stop,
    on_delta: &mut dyn FnMut(&str),
) -> Result<ChatReply> {
    let payload = serde_json::to_vec(body)?;
    let mut head = format!(
        "POST {path} HTTP/1.1\r\nHost: {host}:{port}\r\nContent-Type: application/json\r\n\
         Accept: text/event-stream\r\nContent-Length: {}\r\nConnection: close\r\n",
        payload.len()
    );
    if let Some(key) = key {
        head.push_str(&format!("Authorization: Bearer {key}\r\n"));
    }
    head.push_str("\r\n");
    socket.set_nodelay(true)?;
    socket.set_write_timeout(Some(SEND))?;
    let mut writer = socket;
    let sent = writer
        .write_all(head.as_bytes())
        .and_then(|()| writer.write_all(&payload))
        .and_then(|()| writer.flush());
    if let Err(e) = sent {
        return Err(cut_or(anyhow::Error::new(e), stop, false, silence));
    }

    socket.set_read_timeout(Some(POLL))?;
    let words = Rc::new(Cell::new(false));
    let mut reader = BufReader::new(Watched {
        socket,
        stop,
        silence,
        words: words.clone(),
        began: Instant::now(),
        last: Instant::now(),
    });
    let head = match read_head(&mut reader) {
        Ok(head) => head,
        Err(e) => return Err(cut_or(anyhow::Error::new(e), stop, false, silence)),
    };
    if head.status != 200 {
        let mut refusal = Vec::new();
        let _ = Body::new(reader, head.framing)
            .take(REFUSAL_BYTES)
            .read_to_end(&mut refusal);
        bail!(
            "the model answered {}: {}",
            head.status,
            refusal_message(&refusal)
        );
    }

    // After the first word the model is writing: from then on the shorter
    // silence is the limit.
    let mut on_word = |piece: &str| {
        words.set(true);
        on_delta(piece);
    };
    let lines = BufReader::new(Body::new(reader, head.framing));
    read_sse(lines, &mut on_word, stop).map_err(|e| cut_or(e, stop, words.get(), silence))
}

/// The socket, read in short waits: between two of them a stop ends the
/// read, and so does a silence longer than allowed, measured from the
/// question until the first word and from the last bytes after it.
struct Watched<'a> {
    socket: &'a TcpStream,
    stop: &'a Stop,
    silence: Silence,
    words: Rc<Cell<bool>>,
    began: Instant,
    last: Instant,
}

impl Read for Watched<'_> {
    fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        loop {
            // Not `Interrupted`: the standard readers try again on that.
            if self.stop.asked() {
                return Err(io::Error::new(io::ErrorKind::ConnectionAborted, "stopped"));
            }
            let mut socket = self.socket;
            match socket.read(buf) {
                Ok(n) => {
                    self.last = Instant::now();
                    return Ok(n);
                }
                Err(e)
                    if matches!(
                        e.kind(),
                        io::ErrorKind::TimedOut | io::ErrorKind::WouldBlock
                    ) =>
                {
                    let silent = if self.words.get() {
                        self.last.elapsed() >= self.silence.between_words
                    } else {
                        self.began.elapsed() >= self.silence.before_first_word
                    };
                    if silent {
                        return Err(io::Error::new(io::ErrorKind::TimedOut, "silence"));
                    }
                }
                Err(e) => return Err(e),
            }
        }
    }
}

/// What an error while reading means: a [`Cut`] where it is one, the error
/// itself otherwise (the model's own refusal, a stream that was not one).
fn cut_or(error: anyhow::Error, stop: &Stop, words: bool, silence: Silence) -> anyhow::Error {
    if stop.asked() {
        return Cut::Stopped.into();
    }
    if error.downcast_ref::<Cut>().is_some() {
        return error;
    }
    match error
        .chain()
        .find_map(|cause| cause.downcast_ref::<io::Error>())
        .map(io::Error::kind)
    {
        Some(io::ErrorKind::TimedOut | io::ErrorKind::WouldBlock) => {
            if words {
                Cut::SilentBetweenWords(silence.between_words.as_secs()).into()
            } else {
                Cut::SilentBeforeFirstWord(silence.before_first_word.as_secs()).into()
            }
        }
        Some(_) => Cut::Lost.into(),
        None => error,
    }
}

/// `http://host:port/path`: the host, the port and the path.
fn split_url(url: &str) -> Result<(String, u16, String)> {
    let Some(rest) = url.strip_prefix("http://") else {
        bail!("only a model reached over http:// is read here: {url}");
    };
    let (authority, path) = match rest.find('/') {
        Some(at) => (&rest[..at], &rest[at..]),
        None => (rest, "/"),
    };
    let (host, port) = match authority.rsplit_once(':') {
        Some((host, port)) => (
            host,
            port.parse::<u16>()
                .with_context(|| format!("the port of {url}"))?,
        ),
        None => (authority, 80),
    };
    if host.is_empty() {
        bail!("no host in {url}");
    }
    Ok((host.to_string(), port, path.to_string()))
}

/// How a response's body is delimited.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Framing {
    Chunked,
    Length(u64),
    UntilClosed,
}

struct Head {
    status: u16,
    framing: Framing,
}

/// The status line and the headers, up to the empty line.
fn read_head<R: BufRead>(reader: &mut R) -> io::Result<Head> {
    let mut read = 0usize;
    let mut next_line = |reader: &mut R| -> io::Result<String> {
        let mut line = Vec::new();
        let n = reader
            .by_ref()
            .take((HEAD_BYTES - read) as u64)
            .read_until(b'\n', &mut line)?;
        read += n;
        if n == 0 || !line.ends_with(b"\n") {
            return Err(io::Error::new(
                io::ErrorKind::UnexpectedEof,
                "the response's head ended early",
            ));
        }
        String::from_utf8(line)
            .map(|text| text.trim_end().to_string())
            .map_err(|_| io::Error::new(io::ErrorKind::InvalidData, "a head that is not text"))
    };
    let status_line = next_line(reader)?;
    let status = status_line
        .split_whitespace()
        .nth(1)
        .and_then(|code| code.parse::<u16>().ok())
        .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidData, "no status in the response"))?;
    let mut framing = Framing::UntilClosed;
    loop {
        let line = next_line(reader)?;
        if line.is_empty() {
            break;
        }
        let Some((name, value)) = line.split_once(':') else {
            continue;
        };
        let (name, value) = (name.trim(), value.trim());
        if name.eq_ignore_ascii_case("transfer-encoding")
            && value.to_ascii_lowercase().contains("chunked")
        {
            framing = Framing::Chunked;
        } else if name.eq_ignore_ascii_case("content-length") && framing != Framing::Chunked {
            let length = value.parse::<u64>().map_err(|_| {
                io::Error::new(io::ErrorKind::InvalidData, "a length that is not one")
            })?;
            framing = Framing::Length(length);
        }
    }
    Ok(Head { status, framing })
}

/// A response's body as the bytes it carries, whichever way it is framed.
struct Body<R> {
    inner: R,
    framing: Framing,
    /// Left of the current chunk, or of the whole body.
    left: u64,
    done: bool,
}

impl<R: BufRead> Body<R> {
    fn new(inner: R, framing: Framing) -> Body<R> {
        let left = match framing {
            Framing::Length(length) => length,
            Framing::Chunked | Framing::UntilClosed => 0,
        };
        Body {
            inner,
            framing,
            left,
            done: false,
        }
    }

    /// The size line of the next chunk: hexadecimal, maybe extensions.
    fn next_chunk(&mut self) -> io::Result<u64> {
        let mut line = Vec::new();
        self.inner
            .by_ref()
            .take(1024)
            .read_until(b'\n', &mut line)?;
        if !line.ends_with(b"\n") {
            return Err(io::Error::new(
                io::ErrorKind::UnexpectedEof,
                "the answer ended inside a chunk's size",
            ));
        }
        let text = std::str::from_utf8(&line).map_err(|_| {
            io::Error::new(io::ErrorKind::InvalidData, "a chunk size that is not text")
        })?;
        let size = text.split(';').next().unwrap_or("").trim();
        u64::from_str_radix(size, 16)
            .map_err(|_| io::Error::new(io::ErrorKind::InvalidData, "a chunk size that is not one"))
    }

    /// The line end after a chunk's bytes, or after the last chunk's
    /// trailers.
    fn line_end(&mut self) -> io::Result<()> {
        let mut end = [0u8; 2];
        self.inner.read_exact(&mut end)?;
        if &end != b"\r\n" {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                "a chunk that does not end where it says",
            ));
        }
        Ok(())
    }
}

impl<R: BufRead> Read for Body<R> {
    fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        if self.done || buf.is_empty() {
            return Ok(0);
        }
        match self.framing {
            Framing::UntilClosed => self.inner.read(buf),
            Framing::Length(_) | Framing::Chunked => {
                if self.left == 0 {
                    if self.framing != Framing::Chunked {
                        self.done = true;
                        return Ok(0);
                    }
                    let size = self.next_chunk()?;
                    if size == 0 {
                        // No trailers are sent to us; the empty line ends it.
                        self.line_end()?;
                        self.done = true;
                        return Ok(0);
                    }
                    self.left = size;
                }
                let most = buf
                    .len()
                    .min(usize::try_from(self.left).unwrap_or(usize::MAX));
                let n = self.inner.read(&mut buf[..most])?;
                if n == 0 {
                    return Err(io::Error::new(
                        io::ErrorKind::UnexpectedEof,
                        "the answer ended before its body did",
                    ));
                }
                self.left -= n as u64;
                if self.left == 0 && self.framing == Framing::Chunked {
                    self.line_end()?;
                }
                Ok(n)
            }
        }
    }
}

/// A refusal's message, from the JSON body the engine sends with one.
fn refusal_message(body: &[u8]) -> String {
    let text = String::from_utf8_lossy(body);
    serde_json::from_str::<serde_json::Value>(&text)
        .ok()
        .and_then(|json| {
            json["error"]["message"]
                .as_str()
                .or_else(|| json["error"].as_str())
                .map(str::to_string)
        })
        .unwrap_or_else(|| text.trim().chars().take(200).collect())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::net::TcpListener;

    /// A server for one request: reads it, then writes `parts` with a short
    /// pause between them, so that each arrives in a read of its own, and
    /// then holds the connection for `hold` or closes it.
    fn serve(parts: Vec<Vec<u8>>, hold: Duration) -> String {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let port = listener.local_addr().unwrap().port();
        std::thread::spawn(move || {
            let (mut stream, _) = listener.accept().unwrap();
            stream.set_nodelay(true).unwrap();
            let mut reader = BufReader::new(stream.try_clone().unwrap());
            let mut length = 0usize;
            loop {
                let mut line = String::new();
                reader.read_line(&mut line).unwrap();
                if let Some(v) = line.to_ascii_lowercase().strip_prefix("content-length:") {
                    length = v.trim().parse().unwrap();
                }
                if line == "\r\n" {
                    break;
                }
            }
            let mut body = vec![0u8; length];
            reader.read_exact(&mut body).unwrap();
            for part in parts {
                if stream.write_all(&part).is_err() {
                    return;
                }
                stream.flush().unwrap();
                std::thread::sleep(Duration::from_millis(60));
            }
            std::thread::sleep(hold);
        });
        format!("http://127.0.0.1:{port}/v1/chat/completions")
    }

    const CHUNKED: &str =
        "HTTP/1.1 200 OK\r\nContent-Type: text/event-stream\r\nTransfer-Encoding: chunked\r\n\r\n";

    fn event(content: &str) -> String {
        format!(
            "data: {}\n\n",
            serde_json::json!({"choices": [{"index": 0, "delta": {"content": content}, "finish_reason": null}]})
        )
    }

    const DONE: &str = "data: {\"choices\":[{\"index\":0,\"delta\":{},\"finish_reason\":\"stop\"}],\"usage\":{\"prompt_tokens\":7,\"completion_tokens\":3}}\n\ndata: [DONE]\n\n";

    fn chunk(bytes: &[u8]) -> Vec<u8> {
        let mut out = format!("{:x}\r\n", bytes.len()).into_bytes();
        out.extend_from_slice(bytes);
        out.extend_from_slice(b"\r\n");
        out
    }

    const QUICK: Silence = Silence {
        before_first_word: Duration::from_secs(5),
        between_words: Duration::from_secs(5),
    };

    fn read(url: &str, silence: Silence, stop: &Stop) -> (Result<ChatReply>, String) {
        let mut pieces = String::new();
        let answer = post_and_read(
            url,
            Some("key"),
            &serde_json::json!({"stream": true}),
            silence,
            stop,
            &mut |piece| pieces.push_str(piece),
        );
        (answer, pieces)
    }

    fn cut_of(answer: &Result<ChatReply>) -> Option<Cut> {
        answer
            .as_ref()
            .err()
            .and_then(|e| e.downcast_ref::<Cut>().cloned())
    }

    /// Russian and Uzbek text splits characters between reads all the time:
    /// "П" is two bytes, and here its first byte ends one chunk and its
    /// second begins the next, each in a read of its own.
    #[test]
    fn a_character_split_between_two_reads_arrives_whole() {
        let whole = event("Привет, мир").into_bytes();
        let at = whole.iter().position(|&b| b == 0xD0).unwrap() + 1;
        let url = serve(
            vec![
                CHUNKED.as_bytes().to_vec(),
                chunk(&whole[..at]),
                chunk(&whole[at..]),
                chunk(DONE.as_bytes()),
                b"0\r\n\r\n".to_vec(),
            ],
            Duration::ZERO,
        );
        let (answer, pieces) = read(&url, QUICK, &Stop::default());
        let reply = answer.unwrap();
        assert_eq!(reply.text, "Привет, мир");
        assert_eq!(pieces, "Привет, мир");
        assert_eq!(reply.prompt_tokens, 7);
    }

    #[test]
    fn an_event_split_across_chunks_and_several_in_one_chunk_all_arrive() {
        let first = event("one ").into_bytes();
        let (a, rest) = first.split_at(5);
        let (b, c) = rest.split_at(rest.len() / 2);
        let several = [event("two "), event("three "), event("four")].concat();
        let url = serve(
            vec![
                CHUNKED.as_bytes().to_vec(),
                chunk(a),
                chunk(b),
                chunk(c),
                chunk(several.as_bytes()),
                chunk(DONE.as_bytes()),
                b"0\r\n\r\n".to_vec(),
            ],
            Duration::ZERO,
        );
        let (answer, pieces) = read(&url, QUICK, &Stop::default());
        assert_eq!(answer.unwrap().text, "one two three four");
        assert_eq!(pieces, "one two three four");
    }

    /// The engine sends its answer without chunks too, to the connection's
    /// end, as the test engine does.
    #[test]
    fn an_answer_read_to_the_connections_end_is_complete_with_its_end() {
        let head =
            "HTTP/1.1 200 OK\r\nContent-Type: text/event-stream\r\nConnection: close\r\n\r\n";
        let url = serve(
            vec![
                head.as_bytes().to_vec(),
                event("whole").into_bytes(),
                DONE.as_bytes().to_vec(),
            ],
            Duration::ZERO,
        );
        let (answer, _) = read(&url, QUICK, &Stop::default());
        assert_eq!(answer.unwrap().text, "whole");
    }

    /// The engine went away mid-answer: what came is kept by the reader, and
    /// the answer is said to have been cut, not finished.
    #[test]
    fn an_engine_that_goes_away_mid_answer_leaves_what_came_and_a_cut() {
        let url = serve(
            vec![
                CHUNKED.as_bytes().to_vec(),
                chunk(event("half ").as_bytes()),
                chunk(event("an answer").as_bytes()),
            ],
            Duration::ZERO,
        );
        let (answer, pieces) = read(&url, QUICK, &Stop::default());
        assert_eq!(pieces, "half an answer");
        assert_eq!(cut_of(&answer), Some(Cut::Lost), "{answer:?}");

        // And inside a chunk, not between two.
        let whole = chunk(event("cut inside").as_bytes());
        let url = serve(
            vec![
                CHUNKED.as_bytes().to_vec(),
                whole[..whole.len() - 6].to_vec(),
            ],
            Duration::ZERO,
        );
        let (answer, _) = read(&url, QUICK, &Stop::default());
        assert_eq!(cut_of(&answer), Some(Cut::Lost), "{answer:?}");
    }

    #[test]
    fn silence_before_the_first_word_ends_the_answer() {
        let url = serve(vec![CHUNKED.as_bytes().to_vec()], Duration::from_secs(5));
        let silence = Silence {
            before_first_word: Duration::from_millis(400),
            between_words: Duration::from_secs(5),
        };
        let began = Instant::now();
        let (answer, pieces) = read(&url, silence, &Stop::default());
        assert_eq!(
            cut_of(&answer),
            Some(Cut::SilentBeforeFirstWord(0)),
            "{answer:?}"
        );
        assert!(pieces.is_empty());
        assert!(
            began.elapsed() < Duration::from_secs(3),
            "{:?}",
            began.elapsed()
        );
    }

    #[test]
    fn silence_between_words_ends_the_answer_and_keeps_the_words() {
        let url = serve(
            vec![
                CHUNKED.as_bytes().to_vec(),
                chunk(event("so far").as_bytes()),
            ],
            Duration::from_secs(5),
        );
        let silence = Silence {
            before_first_word: Duration::from_secs(5),
            between_words: Duration::from_millis(400),
        };
        let (answer, pieces) = read(&url, silence, &Stop::default());
        assert_eq!(
            cut_of(&answer),
            Some(Cut::SilentBetweenWords(0)),
            "{answer:?}"
        );
        assert_eq!(pieces, "so far");
    }

    /// A stop ends a read that waits on a silent model at once, not when
    /// the silence runs out.
    #[test]
    fn a_stop_ends_the_answer_at_once_and_keeps_the_words() {
        let url = serve(
            vec![
                CHUNKED.as_bytes().to_vec(),
                chunk(event("before the stop").as_bytes()),
            ],
            Duration::from_secs(10),
        );
        let stop = Stop::default();
        let stopper = stop.clone();
        std::thread::spawn(move || {
            std::thread::sleep(Duration::from_millis(500));
            stopper.stop();
        });
        let began = Instant::now();
        let (answer, pieces) = read(&url, Silence::ANSWER, &stop);
        assert_eq!(cut_of(&answer), Some(Cut::Stopped), "{answer:?}");
        assert_eq!(pieces, "before the stop");
        assert!(
            began.elapsed() < Duration::from_secs(3),
            "{:?}",
            began.elapsed()
        );
    }

    #[test]
    fn a_refusal_is_the_models_own_words_and_not_a_cut() {
        let json = r#"{"error":{"code":500,"message":"on purpose","type":"server_error"}}"#;
        let refusal = format!(
            "HTTP/1.1 500 Internal Server Error\r\nContent-Type: application/json\r\nContent-Length: {}\r\n\r\n{json}",
            json.len()
        );
        let url = serve(vec![refusal.into_bytes()], Duration::ZERO);
        let (answer, _) = read(&url, QUICK, &Stop::default());
        let error = answer.unwrap_err();
        assert!(error.downcast_ref::<Cut>().is_none(), "{error:#}");
        assert!(
            format!("{error:#}").contains("500: on purpose"),
            "{error:#}"
        );
    }

    #[test]
    fn only_a_plain_address_is_taken() {
        assert_eq!(
            split_url("http://127.0.0.1:8080/v1/chat/completions").unwrap(),
            (
                "127.0.0.1".to_string(),
                8080,
                "/v1/chat/completions".to_string()
            )
        );
        assert_eq!(
            split_url("http://example.local/x").unwrap(),
            ("example.local".to_string(), 80, "/x".to_string())
        );
        assert!(split_url("https://example.com/v1").is_err());
        assert!(split_url("http://:80/").is_err());
    }
}
