use std::ffi::OsStr;
use std::io;
use std::mem::size_of;
use std::process::Stdio;
use std::sync::Arc;

use mina_p2p_messages::v2::{
    CurrencyFeeStableV1, NonZeroCurvePoint, SnarkWorkerWorkerRpcsVersionedGetWorkV2TResponse,
    SnarkWorkerWorkerRpcsVersionedGetWorkV2TResponseA0, TransactionSnarkWorkTStableV2Proofs,
};
use mina_p2p_messages::{
    binprot::{
        self,
        macros::{BinProtRead, BinProtWrite},
        BinProtRead, BinProtWrite,
    },
    string::CharString,
};

use node::core::channels::{mpsc, oneshot};
use node::event_source::Event;
use node::external_snark_worker::{
    ExternalSnarkWorkerError, ExternalSnarkWorkerEvent, ExternalSnarkWorkerService,
    ExternalSnarkWorkerWorkError, SnarkWorkSpec,
};

use tokio::io::{AsyncBufReadExt, AsyncRead, AsyncReadExt, AsyncWrite, AsyncWriteExt, BufReader};
use tokio::process::Command;

use super::NodeService;

/// Error generated by external snarker controller.
#[derive(Debug, thiserror::Error)]
enum SnarkerError {
    /// Binprot decoding error while communicating with worker.
    #[error(transparent)]
    BinprotError(#[from] binprot::Error),
    /// I/O error while communicating with worker.
    #[error(transparent)]
    IOError(#[from] io::Error),
    /// Nix-generated error when sending a signal.
    #[error(transparent)]
    NixError(#[from] nix::Error),
    /// Trying to communicate with non-running worker.
    #[error("external snark worker is not running")]
    NotRunning,
    /// Trying to send job while working on one.
    #[error("external snark worker is busy")]
    Busy,
    /// Protocol logic is broken. Means redux-side logic error.
    #[error("communication is broken: {_0}")]
    Broken(String),
}

impl From<SnarkerError> for ExternalSnarkWorkerError {
    fn from(source: SnarkerError) -> Self {
        match source {
            SnarkerError::BinprotError(err) => {
                ExternalSnarkWorkerError::BinprotError(err.to_string())
            }
            SnarkerError::IOError(err) => ExternalSnarkWorkerError::IOError(err.to_string()),
            SnarkerError::NixError(err) => {
                ExternalSnarkWorkerError::Error(format!("nix error: {err}"))
            }
            SnarkerError::NotRunning => ExternalSnarkWorkerError::NotRunning,
            SnarkerError::Busy => ExternalSnarkWorkerError::Busy,
            SnarkerError::Broken(err) => ExternalSnarkWorkerError::Broken(err),
        }
    }
}

impl From<SnarkerError> for ExternalSnarkWorkerEvent {
    fn from(source: SnarkerError) -> Self {
        ExternalSnarkWorkerEvent::Error(source.into())
    }
}

/// Writes binprot-encoded element, prefixed with 8-bytes le size.
async fn write_binprot<T: BinProtWrite, W: AsyncWrite + Unpin>(
    spec: T,
    mut w: W,
) -> Result<(), SnarkerError> {
    let mut buf = Vec::new();
    spec.binprot_write(&mut buf)?;
    let len = (buf.len() as u64).to_le_bytes();
    w.write_all(&len).await?;
    w.write_all(&buf).await?;
    Ok(())
}

/// Reads binprot-encoded element, prefixed with 8-bytes le size.
async fn read_binprot<T, R>(mut r: R) -> Result<T, SnarkerError>
where
    T: BinProtRead,
    R: AsyncRead + Unpin,
{
    let mut len_buf = [0; size_of::<u64>()];
    r.read_exact(&mut len_buf).await?;
    let len = u64::from_le_bytes(len_buf);
    node::core::log::debug!(node::core::log::system_time(); "reading {len} bytes...");

    let mut buf = Vec::with_capacity(len as usize);
    let mut r = r.take(len);
    r.read_to_end(&mut buf).await?;

    let mut read = buf.as_slice();
    let result = T::binprot_read(&mut read)?;
    node::core::log::debug!(node::core::log::system_time(); "succesfully read {len} bytes");
    Ok(result)
}

/// Facade for external worker process.
pub struct ExternalSnarkWorkerFacade {
    data_chan: mpsc::Sender<SnarkWorkSpec>,
    cancel_chan: mpsc::Sender<()>,
    kill_chan: oneshot::Sender<()>,
}

/// External worker input.
#[derive(Debug, BinProtWrite)]
pub enum ExternalSnarkWorkerRequest {
    /// Queries worker for readiness, expected reply is `true`.
    AwaitReadiness,
    /// Commands worker to start specified snark job, expected reply is `ExternalSnarkWorkerResult`[ExternalSnarkWorkerResult].
    PerformJob(SnarkWorkerWorkerRpcsVersionedGetWorkV2TResponse),
}

/// External worker output, when requested to produce a snark.
#[derive(BinProtRead)]
pub enum ExternalSnarkWorkerResult {
    /// Positive response, `Some(snark)` when a snark is produced, and `None` when the job is cancelled.
    Ok(Option<TransactionSnarkWorkTStableV2Proofs>),
    /// Negative response, with description of the error occurred.
    Err(CharString),
}

impl ExternalSnarkWorkerRequest {
    fn await_readiness() -> Self {
        Self::AwaitReadiness
    }

    fn perform_job(
        job: SnarkWorkSpec,
        proover: NonZeroCurvePoint,
        fee: CurrencyFeeStableV1,
    ) -> Self {
        ExternalSnarkWorkerRequest::PerformJob(SnarkWorkerWorkerRpcsVersionedGetWorkV2TResponse(
            Some((
                SnarkWorkerWorkerRpcsVersionedGetWorkV2TResponseA0 {
                    instances: job,
                    fee,
                },
                proover,
            )),
        ))
    }
}

async fn stderr_reader<R: AsyncRead + Unpin>(r: R) -> Result<(), SnarkerError> {
    use node::core::log::inner::*;
    #[derive(Debug, serde::Deserialize)]
    struct SnarkerMessage {
        //timestamp: String,
        level: String,
        message: String,
        //metadata: serde_json::Value,
    }
    let mut buf_reader = BufReader::new(r);
    let mut line = String::new();
    while buf_reader.read_line(&mut line).await? > 0 {
        let t = node::core::log::system_time();
        match serde_json::from_str::<SnarkerMessage>(&line) {
            Ok(entry) => match entry.level.parse() {
                Ok(Level::INFO) => {
                    node::core::log::info!(t; source = "external snark worker", message = entry.message)
                }
                Ok(Level::WARN) => {
                    node::core::log::warn!(t; source = "external snark worker", message = entry.message)
                }
                Ok(Level::ERROR) => {
                    node::core::log::error!(t; source = "external snark worker", message = entry.message)
                }
                _ => {
                    node::core::log::warn!(t; source = "external snark worker", message = entry.message)
                }
            },
            Err(_) => {
                node::core::log::warn!(t; source = "external snark worker", unformatted_message = line);
            }
        }
        line.clear();
    }
    Ok(())
}

macro_rules! send_event {
    ($channel:expr, $event:expr) => {
        _ = $channel.send(node::event_source::Event::ExternalSnarkWorker($event));
    };
}

impl ExternalSnarkWorkerFacade {
    fn start<P: AsRef<OsStr>>(
        path: P,
        public_key: NonZeroCurvePoint,
        fee: CurrencyFeeStableV1,
        event_sender: mpsc::UnboundedSender<Event>,
    ) -> Result<Self, SnarkerError> {
        let (data_chan, mut data_rx) = mpsc::channel(1);
        let (cancel_chan, mut cancel_rx) = mpsc::channel(1);
        let (kill_chan, kill_rx) = oneshot::channel();

        let metadata = std::fs::File::open(path.as_ref())?.metadata()?;
        if !metadata.is_file() {
            return Err(io::Error::new(io::ErrorKind::InvalidInput, "not a file").into());
        }
        let mut cmd = Command::new(path);

        // TODO(akoptelov) make the block return terminal errors instead of sending them down the channel and exit.
        std::thread::Builder::new()
            .name("external-snark-worker".into())
            .spawn(move || {
                let runtime = tokio::runtime::Builder::new_current_thread()
                    .enable_all()
                    .build()
                    .unwrap();
                runtime.block_on(async move {
                    let event_sender_clone = event_sender.clone();

                    let mut child = match cmd
                        .stdin(Stdio::piped())
                        .stdout(Stdio::piped())
                        .stderr(Stdio::piped())
                        .spawn()
                    {
                        Ok(v) => v,
                        Err(err) => {
                            send_event!(event_sender_clone, SnarkerError::from(err).into());
                            return;
                        }
                    };

                    let mut child_stdin = child.stdin.take().unwrap();
                    let mut child_stdout = child.stdout.take().unwrap();

                    if let Some(pid) = child.id() {
                        let pid = nix::unistd::Pid::from_raw(pid as i32);
                        tokio::spawn(async move {
                            // readiness
                            let request = ExternalSnarkWorkerRequest::await_readiness();
                            if let Err(err) = write_binprot(request, &mut child_stdin).await {
                                send_event!(event_sender_clone, err.into());
                                return;
                            }
                            let response = read_binprot(&mut child_stdout).await;
                            match response {
                                Ok(v) if v => {
                                    send_event!(
                                        event_sender_clone,
                                        ExternalSnarkWorkerEvent::Started
                                    );
                                }
                                Ok(_) => {
                                    send_event!(
                                        event_sender_clone,
                                        SnarkerError::Broken(
                                            "snarker responded `false` on readiness request".into()
                                        )
                                        .into()
                                    );
                                    return;
                                }
                                Err(err) => {
                                    send_event!(event_sender_clone, err.into());
                                    return;
                                }
                            }

                            loop {
                                let Some(spec) = data_rx.recv().await else {
                                    return;
                                };
                                let request = ExternalSnarkWorkerRequest::perform_job(
                                    spec,
                                    public_key.clone(),
                                    fee.clone(),
                                );
                                if let Err(err) = write_binprot(request, &mut child_stdin).await {
                                    send_event!(event_sender_clone, err.into());
                                    return;
                                }
                                let response = read_binprot(&mut child_stdout).await;
                                match response {
                                    Ok(result) => match result {
                                        ExternalSnarkWorkerResult::Ok(Some(v)) => {
                                            send_event!(event_sender_clone, Arc::new(v).into());
                                        }
                                        ExternalSnarkWorkerResult::Ok(None) => {
                                            send_event!(
                                                event_sender_clone,
                                                ExternalSnarkWorkerEvent::WorkCancelled
                                            );
                                        }
                                        ExternalSnarkWorkerResult::Err(err) => {
                                            send_event!(
                                                event_sender_clone,
                                                ExternalSnarkWorkerWorkError::Error(
                                                    err.to_string()
                                                )
                                                .into()
                                            );
                                        }
                                    },
                                    Err(err) => {
                                        send_event!(event_sender_clone, err.into());
                                    }
                                }
                            }
                        });

                        let event_sender_clone = event_sender.clone();
                        tokio::spawn(async move {
                            loop {
                                if cancel_rx.recv().await.is_none() {
                                    return;
                                }
                                println!("sending cancel signal to {pid}...");
                                if let Err(err) =
                                    nix::sys::signal::kill(pid, nix::sys::signal::Signal::SIGINT)
                                {
                                    send_event!(event_sender_clone, SnarkerError::from(err).into());
                                }
                            }
                        });

                        // snarker stderr reader
                        let child_stderr = BufReader::new(child.stderr.take().unwrap());
                        let event_sender_clone = event_sender.clone();
                        tokio::spawn(async move {
                            if let Err(err) = stderr_reader(child_stderr).await {
                                send_event!(event_sender_clone, err.into());
                            }
                        });

                        tokio::select! {
                            _ = kill_rx => {
                                if let Err(err) = child.kill().await {
                                    send_event!(event_sender, SnarkerError::from(err).into());
                                } else {
                                    send_event!(event_sender, ExternalSnarkWorkerEvent::Killed);
                                }
                            }
                            _ = child.wait() => {
                            }
                        };
                    }
                });
            })?;

        Ok(ExternalSnarkWorkerFacade {
            data_chan,
            cancel_chan,
            kill_chan,
        })
    }

    fn cancel(&mut self) -> Result<(), SnarkerError> {
        self.cancel_chan
            .try_send(())
            .map_err(|_| SnarkerError::Broken("already cancelled".into()))
    }

    fn submit(&mut self, spec: SnarkWorkSpec) -> Result<(), SnarkerError> {
        self.data_chan
            .try_send(spec)
            .map_err(|_| SnarkerError::Busy)
    }

    fn kill(self) -> Result<(), SnarkerError> {
        self.kill_chan
            .send(())
            .map_err(|_| SnarkerError::Broken("already sent kill".into()))
    }
}

impl ExternalSnarkWorkerService for NodeService {
    fn start<P: AsRef<OsStr>>(
        &mut self,
        path: P,
        public_key: NonZeroCurvePoint,
        fee: CurrencyFeeStableV1,
    ) -> Result<(), node::external_snark_worker::ExternalSnarkWorkerError> {
        if self.replayer.is_some() {
            return Ok(());
        }
        let cmd_sender =
            ExternalSnarkWorkerFacade::start(path, public_key, fee, self.event_sender.clone())?;
        self.snark_worker_sender = Some(cmd_sender);
        Ok(())
    }

    fn submit(
        &mut self,
        spec: SnarkWorkSpec,
    ) -> Result<(), node::external_snark_worker::ExternalSnarkWorkerError> {
        if self.replayer.is_some() {
            return Ok(());
        }
        self.snark_worker_sender
            .as_mut()
            .ok_or(SnarkerError::NotRunning)
            .and_then(|sender| sender.submit(spec))?;
        Ok(())
    }

    fn cancel(&mut self) -> Result<(), ExternalSnarkWorkerError> {
        if self.replayer.is_some() {
            return Ok(());
        }
        self.snark_worker_sender
            .as_mut()
            .ok_or(SnarkerError::NotRunning)
            .and_then(|sender| sender.cancel())?;
        Ok(())
    }

    fn kill(&mut self) -> Result<(), node::external_snark_worker::ExternalSnarkWorkerError> {
        if self.replayer.is_some() {
            return Ok(());
        }
        self.snark_worker_sender
            .take()
            .ok_or(SnarkerError::NotRunning)
            .and_then(|sender| sender.kill())?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use std::{env, ffi::OsString, path::Path, time::Duration};

    use mina_p2p_messages::binprot::BinProtRead;
    use mina_p2p_messages::v2::{
        CurrencyFeeStableV1, NonZeroCurvePoint, SnarkWorkerWorkerRpcsVersionedGetWorkV2TResponse,
        SnarkWorkerWorkerRpcsVersionedGetWorkV2TResponseA0,
    };
    use node::core::channels::mpsc;
    use node::core::log::inner::Level;
    use node::{
        event_source::Event,
        external_snark_worker::{ExternalSnarkWorkerEvent, SnarkWorkSpec},
    };

    use super::super::tracing;
    use super::ExternalSnarkWorkerFacade;

    macro_rules! expect_event {
        ($source:expr, $event:pat) => {
            let result = $source.recv().await.expect("failed to receive an event");
            let Event::ExternalSnarkWorker(result) = result else {
                panic!("unexpected event kind");
            };
            let $event = result else {
                panic!("unexpected snark worker event: {result:?}");
            };
        };
    }

    fn mina_exe_path() -> OsString {
        env::var_os("MINA_EXE_PATH")
            .or_else(|| {
                env::var_os("CARGO_MANIFEST_DIR")
                    .map(|dir| Path::new(&dir).join("bin/snark-worker").into_os_string())
            })
            .unwrap()
    }

    #[tokio::test]
    async fn test_kill() {
        let (event_tx, mut event_rx) = mpsc::unbounded_channel();
        let cmd_sender = ExternalSnarkWorkerFacade::start(
            mina_exe_path(),
            NonZeroCurvePoint::default(),
            CurrencyFeeStableV1(
                mina_p2p_messages::v2::UnsignedExtendedUInt64Int64ForVersionTagsStableV1(
                    10_u64.into(),
                ),
            ),
            event_tx,
        )
        .unwrap();

        expect_event!(event_rx, ExternalSnarkWorkerEvent::Started);

        cmd_sender.kill().expect("cannot kill worker");
        expect_event!(event_rx, ExternalSnarkWorkerEvent::Killed);
    }

    fn read_input<R: std::io::Read>(
        mut r: R,
    ) -> (NonZeroCurvePoint, CurrencyFeeStableV1, SnarkWorkSpec) {
        let SnarkWorkerWorkerRpcsVersionedGetWorkV2TResponse(Some((
            SnarkWorkerWorkerRpcsVersionedGetWorkV2TResponseA0 { instances, fee },
            public_key,
        ))) = SnarkWorkerWorkerRpcsVersionedGetWorkV2TResponse::binprot_read(&mut r)
            .expect("cannot read work spec")
        else {
            unreachable!("incorrect work spec");
        };

        (public_key, fee, instances)
    }

    #[tokio::test]
    async fn test_work() {
        tracing::initialize(Level::DEBUG);
        const DATA: &[u8] = include_bytes!("../../../tests/files/snark_spec/spec1.bin");
        let mut r = DATA;
        let (public_key, fee, instances) = read_input(&mut r);

        let (event_tx, mut event_rx) = mpsc::unbounded_channel();
        let mut cmd_sender =
            ExternalSnarkWorkerFacade::start(mina_exe_path(), public_key, fee, event_tx).unwrap();

        expect_event!(event_rx, ExternalSnarkWorkerEvent::Started);

        cmd_sender.submit(instances).unwrap();
        expect_event!(event_rx, ExternalSnarkWorkerEvent::WorkResult(_));

        cmd_sender.kill().expect("cannot kill worker");
        expect_event!(event_rx, ExternalSnarkWorkerEvent::Killed);
    }

    #[tokio::test]
    async fn test_cancel() {
        const DATA: &[u8] = include_bytes!("../../../tests/files/snark_spec/spec1.bin");
        let mut r = DATA;
        let (public_key, fee, instances) = read_input(&mut r);

        let (event_tx, mut event_rx) = mpsc::unbounded_channel();
        let mut cmd_sender =
            ExternalSnarkWorkerFacade::start(mina_exe_path(), public_key, fee, event_tx).unwrap();

        expect_event!(event_rx, ExternalSnarkWorkerEvent::Started);

        cmd_sender.submit(instances.clone()).unwrap();

        // ensure that for 5 seconds no feedback is received
        let _ = tokio::time::timeout(Duration::from_secs(5), event_rx.recv())
            .await
            .map(|event| {
                panic!("unexpected event received too early: {event:?}");
            });

        cmd_sender.cancel().unwrap();
        expect_event!(event_rx, ExternalSnarkWorkerEvent::WorkCancelled);

        cmd_sender.submit(instances).unwrap();
        expect_event!(event_rx, ExternalSnarkWorkerEvent::WorkResult(_));

        cmd_sender.kill().expect("cannot kill worker");
        expect_event!(event_rx, ExternalSnarkWorkerEvent::Killed);
    }

    #[tokio::test]
    async fn test_2x_cancel() {
        const DATA: &[u8] = include_bytes!("../../../tests/files/snark_spec/spec1.bin");
        let mut r = DATA;
        let (public_key, fee, instances) = read_input(&mut r);

        let (event_tx, mut event_rx) = mpsc::unbounded_channel();
        let mut cmd_sender =
            ExternalSnarkWorkerFacade::start(mina_exe_path(), public_key, fee, event_tx).unwrap();

        expect_event!(event_rx, ExternalSnarkWorkerEvent::Started);

        cmd_sender.submit(instances.clone()).unwrap();

        // ensure that for 5 seconds no feedback is received
        let _ = tokio::time::timeout(Duration::from_secs(5), event_rx.recv())
            .await
            .map(|event| {
                panic!("unexpected event received too early: {event:?}");
            });

        cmd_sender.cancel().unwrap();
        expect_event!(event_rx, ExternalSnarkWorkerEvent::WorkCancelled);

        cmd_sender.submit(instances.clone()).unwrap();

        // ensure that for 5 seconds no feedback is received
        let _ = tokio::time::timeout(Duration::from_secs(5), event_rx.recv())
            .await
            .map(|event| {
                panic!("unexpected event received too early: {event:?}");
            });

        cmd_sender.cancel().unwrap();
        expect_event!(event_rx, ExternalSnarkWorkerEvent::WorkCancelled);

        cmd_sender.submit(instances).unwrap();
        expect_event!(event_rx, ExternalSnarkWorkerEvent::WorkResult(_));

        cmd_sender.kill().expect("cannot kill worker");
        expect_event!(event_rx, ExternalSnarkWorkerEvent::Killed);
    }
}
