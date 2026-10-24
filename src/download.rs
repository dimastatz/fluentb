//! Downloads model files: resumable over HTTP Range, verified with SHA-256, and only
//! renamed into place once the hash matches.
//!
//! Model downloads are the only network access fluentb makes besides the update
//! check. Requests are plain GETs: nothing about the user, their audio or their text
//! is ever sent.

use crate::models::{Entry, PARTIAL_SUFFIX};
use sha2::{Digest, Sha256};
use std::fmt::{self, Write as _};
use std::fs::{self, File, OpenOptions};
use std::io::{self, Read, Write};
use std::path::{Path, PathBuf};
use std::time::Duration;
use ureq::tls::{RootCerts, TlsConfig, TlsProvider};

/// Free space to leave on the disk after a download.
const SPACE_MARGIN: u64 = 100 * 1000 * 1000;
const CHUNK: usize = 64 * 1024;

/// Why a download failed.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DownloadError {
    NotEnoughSpace {
        needed: u64,
        available: u64,
    },
    /// No connection, a timeout, or the transfer stopped early. Worth retrying.
    Network(String),
    /// The server refused, e.g. 404. Not worth retrying.
    Http(u16),
    /// The file didn't match its published hash; the partial file was deleted.
    HashMismatch,
    Io(String),
}

impl DownloadError {
    /// Whether trying again could help.
    #[must_use]
    pub const fn is_transient(&self) -> bool {
        matches!(self, Self::Network(_) | Self::HashMismatch)
    }
}

impl fmt::Display for DownloadError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::NotEnoughSpace { needed, available } => write!(
                f,
                "not enough disk space: it needs {}, and {} is free",
                format_size(*needed),
                format_size(*available)
            ),
            Self::Network(reason) => write!(
                f,
                "couldn't reach the download server ({reason}). Check your internet connection"
            ),
            Self::Http(status) => write!(f, "the download server answered with HTTP {status}"),
            Self::HashMismatch => write!(f, "the downloaded file was corrupted"),
            Self::Io(reason) => write!(f, "couldn't save the file: {reason}"),
        }
    }
}

impl std::error::Error for DownloadError {}

/// An HTTP client for model downloads: OS certificate store, a connect and per-read
/// timeout so a stalled connection fails instead of hanging, and redirects followed
/// (Hugging Face serves files from a CDN).
#[must_use]
pub fn agent() -> ureq::Agent {
    ureq::Agent::config_builder()
        .tls_config(
            TlsConfig::builder()
                .provider(TlsProvider::NativeTls)
                .root_certs(RootCerts::PlatformVerifier)
                .build(),
        )
        .timeout_connect(Some(Duration::from_secs(15)))
        .timeout_per_read(Some(Duration::from_secs(60)))
        .build()
        .into()
}

/// One file to download.
#[derive(Debug, Clone, Copy)]
pub struct Job<'a> {
    pub url: &'a str,
    pub entry: &'a Entry,
    /// The folder the file is saved in.
    pub dir: &'a Path,
}

/// Downloads `job`, resuming a partial download, and returns the final path.
/// `free_space` reports the free bytes on the disk holding the folder; `progress`
/// receives the bytes downloaded so far.
pub fn fetch(
    agent: &ureq::Agent,
    job: Job,
    free_space: impl Fn(&Path) -> io::Result<u64>,
    mut progress: impl FnMut(u64),
) -> Result<PathBuf, DownloadError> {
    let Job { url, entry, dir } = job;
    fs::create_dir_all(dir).map_err(io_error)?;
    let target = dir.join(entry.model.file_name());
    let partial = dir.join(format!("{}{PARTIAL_SUFFIX}", entry.model.file_name()));
    let existing = fs::metadata(&partial).map_or(0, |metadata| metadata.len());
    let existing = if existing > entry.size {
        fs::remove_file(&partial).map_err(io_error)?;
        0
    } else {
        existing
    };

    let needed = entry.size - existing + SPACE_MARGIN;
    let available = free_space(dir).map_err(io_error)?;
    if available < needed {
        return Err(DownloadError::NotEnoughSpace { needed, available });
    }

    let hasher = hash_file(&partial, existing)?;
    let (hasher, done) = if existing == entry.size {
        (hasher, existing)
    } else {
        transfer(agent, url, &partial, existing, hasher, &mut progress)?
    };
    if done != entry.size {
        return Err(DownloadError::Network(format!(
            "the transfer stopped after {done} of {} bytes",
            entry.size
        )));
    }
    if hex(&hasher.finalize()) != entry.sha256 {
        fs::remove_file(&partial).map_err(io_error)?;
        return Err(DownloadError::HashMismatch);
    }
    fs::rename(&partial, &target).map_err(io_error)?;
    Ok(target)
}

/// Like [`fetch`], retrying transient failures after each of `delays`. A later
/// attempt resumes where the previous one stopped.
pub fn fetch_with_retries(
    agent: &ureq::Agent,
    job: Job,
    free_space: impl Fn(&Path) -> io::Result<u64>,
    mut progress: impl FnMut(u64),
    delays: &[Duration],
    sleep: impl Fn(Duration),
) -> Result<PathBuf, DownloadError> {
    let first = fetch(agent, job, &free_space, &mut progress);
    delays
        .iter()
        .try_fold(first, |result, delay| match result {
            Err(error) if error.is_transient() => {
                sleep(*delay);
                Ok(fetch(agent, job, &free_space, &mut progress))
            }
            done => Err(done),
        })
        .unwrap_or_else(|done| done)
}

/// Streams the response body onto the end of `partial`. Returns the hasher and the
/// total size of `partial` afterwards.
fn transfer(
    agent: &ureq::Agent,
    url: &str,
    partial: &Path,
    existing: u64,
    hasher: Sha256,
    progress: &mut impl FnMut(u64),
) -> Result<(Sha256, u64), DownloadError> {
    let request = agent.get(url);
    let request = if existing > 0 {
        request.header("Range", format!("bytes={existing}-"))
    } else {
        request
    };
    let response = request.call().map_err(|error| match error {
        ureq::Error::StatusCode(status) => DownloadError::Http(status),
        other => DownloadError::Network(other.to_string()),
    })?;
    // A server that ignores the range sends the whole file again.
    let resumed = response.status().as_u16() == 206;
    let (hasher, start) = if resumed {
        (hasher, existing)
    } else {
        (Sha256::new(), 0)
    };
    let mut file = OpenOptions::new()
        .create(true)
        .write(true)
        .append(resumed)
        .truncate(!resumed)
        .open(partial)
        .map_err(io_error)?;
    let mut body = response.into_body().into_reader();
    let mut buffer = vec![0; CHUNK];
    let mut state = (hasher, start);
    loop {
        let read = body
            .read(&mut buffer)
            .map_err(|error| DownloadError::Network(error.to_string()))?;
        if read == 0 {
            break;
        }
        file.write_all(&buffer[..read]).map_err(io_error)?;
        state.0.update(&buffer[..read]);
        state.1 += read as u64;
        progress(state.1);
    }
    file.flush().map_err(io_error)?;
    Ok(state)
}

/// Hashes the first `length` bytes of `path` (an earlier partial download).
fn hash_file(path: &Path, length: u64) -> Result<Sha256, DownloadError> {
    if length == 0 {
        return Ok(Sha256::new());
    }
    let mut file = File::open(path).map_err(io_error)?.take(length);
    let mut buffer = vec![0; CHUNK];
    let mut hasher = Sha256::new();
    loop {
        let read = file.read(&mut buffer).map_err(io_error)?;
        if read == 0 {
            return Ok(hasher);
        }
        hasher.update(&buffer[..read]);
    }
}

fn hex(bytes: &[u8]) -> String {
    bytes.iter().fold(String::new(), |mut text, byte| {
        let _ = write!(text, "{byte:02x}");
        text
    })
}

/// Whole percent of `total` that `done` represents, capped at 100.
#[must_use]
pub fn percent(done: u64, total: u64) -> u8 {
    if total == 0 {
        return 100;
    }
    u8::try_from((done.min(total) * 100) / total).unwrap_or(100)
}

/// A size for people: `148 MB`, `2.5 GB` (decimal units, like Finder).
#[must_use]
pub fn format_size(bytes: u64) -> String {
    const MB: u64 = 1000 * 1000;
    const GB: u64 = 1000 * MB;
    if bytes >= GB {
        let tenths = (bytes + GB / 20) / (GB / 10);
        format!("{}.{} GB", tenths / 10, tenths % 10)
    } else {
        format!("{} MB", (bytes + MB / 2) / MB)
    }
}

fn io_error(error: impl fmt::Display) -> DownloadError {
    DownloadError::Io(error.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::{ModelSize, WhisperModel};
    use crate::models::Model;
    use std::io::{BufRead, BufReader};
    use std::net::TcpListener;
    use std::sync::{Arc, Mutex};
    use std::thread;

    /// How the test server answers a request, given the requested range start.
    #[derive(Clone, Copy)]
    enum Behaviour {
        /// Honour ranges; send everything.
        Serve,
        /// Honour ranges, but drop the connection after this many body bytes.
        CutAfter(usize),
        /// Ignore ranges and send the whole file.
        IgnoreRange,
        NotFound,
    }

    /// A one-file HTTP server on localhost. Returns its URL and the `Range` header of
    /// every request it received.
    fn serve(
        body: Vec<u8>,
        behaviours: Vec<Behaviour>,
    ) -> (String, Arc<Mutex<Vec<Option<String>>>>) {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let url = format!("http://{}/model.bin", listener.local_addr().unwrap());
        let ranges = Arc::new(Mutex::new(Vec::new()));
        let seen = Arc::clone(&ranges);
        thread::spawn(move || {
            behaviours
                .into_iter()
                .zip(listener.incoming())
                .for_each(|(behaviour, stream)| {
                    let mut stream = stream.unwrap();
                    let range = BufReader::new(&stream)
                        .lines()
                        .map(Result::unwrap)
                        .take_while(|line| !line.is_empty())
                        .find_map(|line| {
                            line.to_lowercase()
                                .strip_prefix("range: bytes=")
                                .map(|rest| rest.trim_end_matches('-').to_owned())
                        });
                    seen.lock().unwrap().push(range.clone());
                    let start: usize = range.map_or(0, |start| start.parse().unwrap());
                    let reply = |status: &str, part: &[u8]| {
                        format!(
                            "HTTP/1.1 {status}\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
                            part.len()
                        )
                    };
                    match behaviour {
                        Behaviour::NotFound => {
                            stream
                                .write_all(reply("404 Not Found", b"").as_bytes())
                                .unwrap();
                        }
                        Behaviour::IgnoreRange => {
                            stream.write_all(reply("200 OK", &body).as_bytes()).unwrap();
                            stream.write_all(&body).unwrap();
                        }
                        Behaviour::Serve | Behaviour::CutAfter(_) => {
                            let status = if start > 0 {
                                "206 Partial Content"
                            } else {
                                "200 OK"
                            };
                            let part = &body[start..];
                            stream.write_all(reply(status, part).as_bytes()).unwrap();
                            let sent = match behaviour {
                                Behaviour::CutAfter(limit) => &part[..limit],
                                _ => part,
                            };
                            stream.write_all(sent).unwrap();
                        }
                    }
                });
        });
        (url, ranges)
    }

    fn file(size: usize) -> Vec<u8> {
        (0..size).map(|i| u8::try_from(i % 251).unwrap()).collect()
    }

    fn entry_for(body: &[u8]) -> Entry {
        Entry {
            model: Model::Whisper(WhisperModel {
                size: ModelSize::Tiny,
                english_only: true,
            }),
            size: body.len() as u64,
            sha256: Box::leak(hex(&Sha256::digest(body)).into_boxed_str()),
        }
    }

    #[allow(clippy::unnecessary_wraps)] // Matches the `free_space` signature.
    fn plenty(_: &Path) -> io::Result<u64> {
        Ok(u64::MAX)
    }

    fn job<'a>(url: &'a str, entry: &'a Entry, dir: &'a Path) -> Job<'a> {
        Job { url, entry, dir }
    }

    fn run(url: &str, entry: &Entry, dir: &Path) -> Result<PathBuf, DownloadError> {
        fetch(&agent(), job(url, entry, dir), plenty, |_| {})
    }

    #[test]
    fn downloads_verifies_and_renames() {
        let body = file(300_000);
        let entry = entry_for(&body);
        let (url, ranges) = serve(body.clone(), vec![Behaviour::Serve]);
        let dir = tempfile::tempdir().unwrap();
        let mut seen = Vec::new();

        let path = fetch(&agent(), job(&url, &entry, dir.path()), plenty, |done| {
            seen.push(done);
        })
        .unwrap();

        assert_eq!(path, dir.path().join("ggml-tiny.en.bin"));
        assert_eq!(fs::read(&path).unwrap(), body);
        assert!(!dir.path().join("ggml-tiny.en.bin.part").exists());
        assert_eq!(seen.last(), Some(&300_000));
        assert!(seen.windows(2).all(|pair| pair[0] < pair[1]));
        assert_eq!(*ranges.lock().unwrap(), [None]);
    }

    #[test]
    fn resumes_a_partial_download() {
        let body = file(200_000);
        let entry = entry_for(&body);
        let (url, ranges) = serve(body.clone(), vec![Behaviour::Serve]);
        let dir = tempfile::tempdir().unwrap();
        fs::write(dir.path().join("ggml-tiny.en.bin.part"), &body[..120_000]).unwrap();

        let path = run(&url, &entry, dir.path()).unwrap();

        assert_eq!(fs::read(path).unwrap(), body);
        assert_eq!(*ranges.lock().unwrap(), [Some("120000".to_owned())]);
    }

    #[test]
    fn starts_over_when_the_server_ignores_the_range() {
        let body = file(100_000);
        let entry = entry_for(&body);
        let (url, _) = serve(body.clone(), vec![Behaviour::IgnoreRange]);
        let dir = tempfile::tempdir().unwrap();
        fs::write(dir.path().join("ggml-tiny.en.bin.part"), &body[..40_000]).unwrap();

        assert_eq!(
            fs::read(run(&url, &entry, dir.path()).unwrap()).unwrap(),
            body
        );
    }

    #[test]
    fn a_dropped_connection_is_retried_and_resumed() {
        let body = file(150_000);
        let entry = entry_for(&body);
        let (url, ranges) = serve(
            body.clone(),
            vec![Behaviour::CutAfter(50_000), Behaviour::Serve],
        );
        let dir = tempfile::tempdir().unwrap();
        let slept = Mutex::new(Vec::new());

        let path = fetch_with_retries(
            &agent(),
            job(&url, &entry, dir.path()),
            plenty,
            |_| {},
            &[Duration::from_secs(2), Duration::from_secs(8)],
            |delay| slept.lock().unwrap().push(delay),
        )
        .unwrap();

        assert_eq!(fs::read(path).unwrap(), body);
        assert_eq!(*slept.lock().unwrap(), [Duration::from_secs(2)]);
        assert_eq!(*ranges.lock().unwrap(), [None, Some("50000".to_owned())]);
    }

    #[test]
    fn a_corrupted_download_is_deleted() {
        let body = file(50_000);
        let entry = Entry {
            sha256: "0000000000000000000000000000000000000000000000000000000000000000",
            ..entry_for(&body)
        };
        let (url, _) = serve(body, vec![Behaviour::Serve]);
        let dir = tempfile::tempdir().unwrap();

        assert_eq!(
            run(&url, &entry, dir.path()),
            Err(DownloadError::HashMismatch)
        );
        assert!(!dir.path().join("ggml-tiny.en.bin.part").exists());
        assert!(!dir.path().join("ggml-tiny.en.bin").exists());
    }

    #[test]
    fn a_missing_file_is_not_retried() {
        let body = file(1_000);
        let entry = entry_for(&body);
        let (url, ranges) = serve(body, vec![Behaviour::NotFound, Behaviour::Serve]);
        let dir = tempfile::tempdir().unwrap();

        let result = fetch_with_retries(
            &agent(),
            job(&url, &entry, dir.path()),
            plenty,
            |_| {},
            &[Duration::ZERO],
            |_| {},
        );

        assert_eq!(result, Err(DownloadError::Http(404)));
        assert_eq!(ranges.lock().unwrap().len(), 1);
    }

    #[test]
    fn checks_disk_space_before_downloading() {
        let body = file(1_000);
        let entry = entry_for(&body);
        let dir = tempfile::tempdir().unwrap();

        let result = fetch(
            &agent(),
            job("http://127.0.0.1:9/never", &entry, dir.path()),
            |_| Ok(5),
            |_| {},
        );

        assert_eq!(
            result,
            Err(DownloadError::NotEnoughSpace {
                needed: 1_000 + SPACE_MARGIN,
                available: 5
            })
        );
    }

    #[test]
    fn an_unreachable_server_is_a_network_error() {
        let body = file(1_000);
        let entry = entry_for(&body);
        let dir = tempfile::tempdir().unwrap();
        let closed = TcpListener::bind("127.0.0.1:0").unwrap();
        let url = format!("http://{}/model.bin", closed.local_addr().unwrap());
        drop(closed);

        let error = run(&url, &entry, dir.path()).unwrap_err();

        assert!(matches!(error, DownloadError::Network(_)), "{error:?}");
        assert!(error.is_transient());
    }

    #[test]
    fn a_complete_partial_file_needs_no_request() {
        let body = file(10_000);
        let entry = entry_for(&body);
        let dir = tempfile::tempdir().unwrap();
        fs::write(dir.path().join("ggml-tiny.en.bin.part"), &body).unwrap();

        let path = run("http://127.0.0.1:9/unused", &entry, dir.path()).unwrap();

        assert_eq!(fs::read(path).unwrap(), body);
    }

    #[test]
    fn an_oversized_partial_file_is_discarded() {
        let body = file(10_000);
        let entry = entry_for(&body);
        let (url, ranges) = serve(body.clone(), vec![Behaviour::Serve]);
        let dir = tempfile::tempdir().unwrap();
        fs::write(dir.path().join("ggml-tiny.en.bin.part"), file(20_000)).unwrap();

        assert_eq!(
            fs::read(run(&url, &entry, dir.path()).unwrap()).unwrap(),
            body
        );
        assert_eq!(*ranges.lock().unwrap(), [None]);
    }

    /// The real thing: Hugging Face redirects to its CDN, over TLS, honouring Range.
    #[test]
    #[ignore = "downloads 78 MB from Hugging Face"]
    fn resumes_a_real_model_download() {
        let model = Model::Whisper(WhisperModel {
            size: ModelSize::Tiny,
            english_only: true,
        });
        let entry = crate::models::entry(model).unwrap();
        let url = model.download_url();
        let dir = tempfile::tempdir().unwrap();
        let start = agent()
            .get(&url)
            .header("Range", "bytes=0-9999999")
            .call()
            .unwrap()
            .into_body()
            .read_to_vec()
            .unwrap();
        assert_eq!(start.len(), 10_000_000);
        fs::write(dir.path().join("ggml-tiny.en.bin.part"), start).unwrap();

        let path = fetch(&agent(), job(&url, entry, dir.path()), plenty, |_| {}).unwrap();

        assert_eq!(fs::metadata(path).unwrap().len(), entry.size);
    }

    #[test]
    fn reports_progress_as_percent_and_sizes_for_people() {
        assert_eq!(percent(0, 200), 0);
        assert_eq!(percent(84, 200), 42);
        assert_eq!(percent(300, 200), 100);
        assert_eq!(percent(0, 0), 100);
        assert_eq!(format_size(147_964_211), "148 MB");
        assert_eq!(format_size(2_497_280_256), "2.5 GB");
        assert_eq!(format_size(2_645_244_467), "2.6 GB");
        assert_eq!(format_size(999_999_999), "1000 MB");
    }

    #[test]
    fn errors_explain_the_problem() {
        let cases = [
            (
                DownloadError::NotEnoughSpace {
                    needed: 2_600_000_000,
                    available: 1_000_000_000,
                },
                "not enough disk space: it needs 2.6 GB, and 1.0 GB is free",
            ),
            (
                DownloadError::Network("timed out".into()),
                "couldn't reach the download server (timed out). Check your internet connection",
            ),
            (
                DownloadError::Http(404),
                "the download server answered with HTTP 404",
            ),
            (
                DownloadError::HashMismatch,
                "the downloaded file was corrupted",
            ),
            (
                DownloadError::Io("disk full".into()),
                "couldn't save the file: disk full",
            ),
        ];

        cases
            .iter()
            .for_each(|(error, text)| assert_eq!(error.to_string(), *text));
        assert!(!DownloadError::Http(404).is_transient());
        assert!(DownloadError::HashMismatch.is_transient());
    }
}
