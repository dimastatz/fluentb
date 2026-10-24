//! The background thread that downloads models one at a time.

use super::MainRequest;
use crate::download::{self, percent, Job};
use crate::facade::ModelStatus;
use crate::models::{self, Model};
use crate::platform::bridge::Signal;
use std::path::PathBuf;
use std::sync::mpsc;
use std::thread;
use std::time::Duration;
use tao::event_loop::EventLoopProxy;

/// Waits between attempts after a network failure.
const RETRY_DELAYS: [Duration; 3] = [
    Duration::from_secs(2),
    Duration::from_secs(10),
    Duration::from_secs(30),
];

/// Starts the download thread. Send it models to fetch; it reports progress and
/// results to the worker through `signals` and to the menu through `proxy`.
pub fn spawn(
    dir: Option<PathBuf>,
    signals: mpsc::Sender<Signal>,
    proxy: EventLoopProxy<MainRequest>,
) -> mpsc::Sender<Model> {
    let (jobs, queue) = mpsc::channel::<Model>();
    thread::spawn(move || {
        let agent = download::agent();
        queue.iter().for_each(|model| {
            let Some(entry) = models::entry(model) else {
                return;
            };
            let kind = model.kind();
            let mut last = None;
            let report = |status: ModelStatus| {
                let _ = signals.send(Signal::Model(model, status));
            };
            let Some(dir) = dir.as_deref() else {
                report(ModelStatus::Failed(
                    "couldn't find the models folder (is HOME set?)".into(),
                ));
                return;
            };
            let url = model.download_url();
            let result = download::fetch_with_retries(
                &agent,
                Job {
                    url: &url,
                    entry,
                    dir,
                },
                |path| fs4::available_space(path),
                |done| {
                    let now = percent(done, entry.size);
                    if last != Some(now) {
                        last = Some(now);
                        report(ModelStatus::Downloading { percent: now });
                        let _ = proxy.send_event(MainRequest::Download(Some((kind, now))));
                    }
                },
                &RETRY_DELAYS,
                thread::sleep,
            );
            let _ = proxy.send_event(MainRequest::Download(None));
            report(result.map_or_else(
                |error| ModelStatus::Failed(error.to_string()),
                |_| ModelStatus::Ready,
            ));
        });
    });
    jobs
}
