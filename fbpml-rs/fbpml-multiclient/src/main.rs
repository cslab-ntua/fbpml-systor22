use std::{
    mem::MaybeUninit, net::ToSocketAddrs, os::unix::prelude::FileTypeExt, path::PathBuf,
    string::ToString, sync::Arc, time::Duration,
};

use anyhow::{bail, Context, Result};
use clap::{Parser, Subcommand};

use futures::future::try_join_all;
use hyper::{Client, StatusCode};
use hyperlocal::{UnixClientExt, Uri};
use rand::{prelude::StdRng, Rng, SeedableRng};
use tokio::{
    sync::Barrier,
    time::{sleep, Instant},
};

use fbpml::{one_arg_rpc, two_args_rpc, zero_args_rpc, Measurement};
use fbpml_rpc::ServiceResponse;

/// A CLI for the gRPC clients of the benchmarks supported in fbpml.
#[derive(Parser)]
#[clap(author, version, about, long_about = None)]
#[clap(propagate_version = true)]
struct Cli {
    /// IP Address and TCP port format of the gRPC server against which the requests will be
    /// issued. An 'ADDRESS:PORT' format is expected, which should also include the `ID` substring,
    /// to be replaced by MicroVM's ID.
    #[clap(short = 'c', long = "server-addr-fmt")]
    address_port_fmt: String,

    /// Number of MicroVMs to restore, resume and talk to in parallel.
    #[clap(short = 'n', long = "num-uvms")]
    num_uvms: usize,

    /// Number of warm invocations to issue (after the cold one) before the warm invocation that is
    /// actually going to be reported.
    #[clap(short = 'w', long = "pre-warm", required = false, default_value = "0")]
    pre_warm: usize,

    #[clap(subcommand)]
    top_cmd: TopSubcommand,
}

impl Cli {
    /// Parse the given `IP_ADDRESS:PORT` format, make sure it is valid, and return a `Vec<String>`
    /// that contains the addresses of all MicroVMs (their number must have been given as an
    /// argument as well) in the expected `IP_ADDRESS:PORT` format.
    fn validate_addrs(&self) -> Result<Vec<String>> {
        Ok((0..self.num_uvms)
            .map(|id| {
                // First make sure they all are valid SocketAddr
                self.address_port_fmt
                    .replace("ID", id.to_string().as_str())
                    .to_socket_addrs()
                    .with_context(|| {
                        format!(
                            "failed to construct a valid SocketAddr from '{}' for ID={id}",
                            self.address_port_fmt
                        )
                    })
            })
            .collect::<Result<Vec<_>>>()?
            .into_iter()
            .map(|sa| sa.into_iter().map(|s| s.to_string()).collect::<String>())
            .collect())
    }
}

#[derive(Subcommand)]
enum TopSubcommand {
    /// Issue two plain gRPC requests (a cold and a warm) to an already-running
    /// MicroVM.
    #[clap(subcommand)]
    Issue(BenchCmd),

    /// Restore a MicroVM from a snapshot (based on the configuration that is
    /// being provided) before issuing the two gRPC requests (a cold and a
    /// warm) to it.
    Restore(RestoreCmd),
}

#[derive(clap::Args, Clone)]
struct RestoreCmd {
    /// Path to the Unix domain socket of the Firecracker instance to be restored.
    #[clap(short = 'x', long = "api-sock")]
    api_sock_path: PathBuf,

    /// Path to the state file of the MicroVM snapshot to be restored.
    #[clap(short = 's', long)]
    state_file: PathBuf,

    /// Path to the memory file of the MicroVM snapshot to be restored.
    #[clap(short = 'm', long)]
    memory_file: PathBuf,

    #[clap(subcommand)]
    bench: BenchCmd,
}

#[derive(Subcommand, Clone, Copy)]
enum BenchCmd {
    /// Run the `chameleon` benchmark.
    Chameleon { arg1: u64, arg2: u64 },

    /// Run the `cnn_serving` benchmark.
    CNNServing { arg: u64 },

    /// Run the `helloworld` benchmark.
    #[clap(name = "helloworld")]
    HelloWorld,

    /// Run the `image_rotate` benchmark.
    ImageRotate { arg: u64 },

    /// Run the `json_serdes` benchmark.
    JSONSerdes { arg: u64 },

    /// Run the `lr_serving` benchmark.
    LRServing { arg: u64 },

    /// Run the `lr_training` benchmark.
    LRTraining { arg: u64 },

    /// Run the `matmul_fb` benchmark.
    #[clap(name = "matmul-fb")]
    MatMulFb { arg1: u64, arg2: u64 },

    /// Run the `matmul_fbpml` benchmark.
    #[clap(name = "matmul-fbpml")]
    MatMulFbpml,

    /// Run the `pyaes` benchmark.
    #[clap(name = "pyaes")]
    PyAES,

    /// Run the `rnn_serving` benchmark.
    RNNServing { arg: u64 },

    /// Run the `video_processing` benchmark.
    VideoProcessing { arg: u64 },
}

impl BenchCmd {
    async fn issue(&self, address_port: String) -> Result<(Duration, ServiceResponse)> {
        use crate::BenchCmd::*;
        Ok(match self {
            HelloWorld | MatMulFbpml | PyAES => zero_args_rpc(address_port)
                .await
                .with_context(|| "could not issue the 'cold' zero-arguments request")?,

            CNNServing { arg }
            | ImageRotate { arg }
            | JSONSerdes { arg }
            | LRServing { arg }
            | LRTraining { arg }
            | VideoProcessing { arg } => one_arg_rpc(address_port, *arg)
                .await
                .with_context(|| "could not issue the 'cold' one-argument request")?,

            Chameleon { arg1, arg2 } | MatMulFb { arg1, arg2 } => {
                two_args_rpc(address_port, *arg1, *arg2)
                    .await
                    .with_context(|| "could not issue the 'cold' two-arguments request")?
            }

            RNNServing { arg: _ } => {
                bail!("Benchmark 'rnn_serving' is not implemented yet")
            }
        })
    }
}

impl RestoreCmd {
    fn validate(&mut self, id: usize) -> Result<()> {
        let hex_id = format!("{id:02X}");

        // Construct uVM's actual API socket path
        self.api_sock_path = self.api_sock_path.with_file_name(
            self.api_sock_path
                .file_name()
                .with_context(|| {
                    format!(
                        "failed to retrieve basename for '{}'",
                        self.api_sock_path.display()
                    )
                })?
                .to_string_lossy()
                .replace("IDh", &hex_id),
        );
        // Validate uVM's API socket path
        if !self
            .api_sock_path
            .metadata()
            .with_context(|| format!("could not stat(2) '{}'", self.api_sock_path.display()))?
            .file_type()
            .is_socket()
        {
            bail!("'{}' is not a Unix socket", self.api_sock_path.display());
        }

        // Construct & validate uVM's state and memory file paths
        let validate_file = |f: &mut PathBuf| {
            *f = f.with_file_name(
                f.file_name()
                    .with_context(|| format!("failed to retrieve basename for '{}'", f.display()))?
                    .to_string_lossy()
                    .replace("IDh", &hex_id),
            );

            if !f
                .metadata()
                .with_context(|| format!("could not stat(2) '{}'", f.display()))?
                .is_file()
            {
                bail!("'{}' should point to a plain (binary) file", f.display());
            }
            Ok(())
        };
        validate_file(&mut self.state_file)?;
        validate_file(&mut self.memory_file)?;

        Ok(())
    }

    async fn restore(&self) -> Result<Duration> {
        let restore_body = format!(
            "{{\"snapshot_path\":\"{}\",\"mem_file_path\":\"{}\",\"enable_diff_snapshots\":false,\"resume_vm\":false}}",
            self.state_file.display(),
            self.memory_file.display()
        );

        let client = Client::unix();
        let uri = Uri::new(self.api_sock_path.as_path(), "/snapshot/load");
        let req = hyper::Request::put(uri)
            .header("Accept", "application/json")
            .header("Content-Type", "application/json")
            .body(restore_body.into())
            .with_context(|| {
                format!(
                    "failed to construct HTTP PUT request for {:?} to restore the MicroVM",
                    Uri::new(self.api_sock_path.as_path(), "/snapshot/load")
                )
            })?;

        let restore_start = Instant::now();
        let resp = client.request(req).await.with_context(|| {
            format!(
                "could not  HTTP PUT /snapshot/load  @ '{}'",
                self.api_sock_path.display()
            )
        })?;
        let restore_end = Instant::now();

        if resp.status() != StatusCode::NO_CONTENT {
            bail!(
                "Could not restore the MicroVM behind '{}' from '{}' and '{}': {}",
                self.api_sock_path.display(),
                self.state_file.display(),
                self.memory_file.display(),
                resp.status()
            );
        }

        Ok(restore_end - restore_start)
    }

    async fn resume(&self) -> Result<Duration> {
        const RESUME_BODY: &str = r#"{"state":"Resumed"}"#;

        let client = Client::unix();
        let uri = Uri::new(self.api_sock_path.as_path(), "/vm");
        let req = hyper::Request::patch(uri)
            .header("Accept", "application/json")
            .header("Content-Type", "application/json")
            .body(RESUME_BODY.into())
            .with_context(|| {
                format!(
                    "failed to construct HTTP PATCH request for {:?} to resume the MicroVM",
                    Uri::new(self.api_sock_path.as_path(), "/vm")
                )
            })?;

        let resume_start = Instant::now();
        let resp = client.request(req).await.with_context(|| {
            format!(
                "could not  HTTP PATCH /vm  @ '{}'",
                self.api_sock_path.display()
            )
        })?;
        let resume_end = Instant::now();

        if resp.status() != StatusCode::NO_CONTENT {
            bail!(
                "Could not resume the MicroVM behind '{}': {}",
                self.api_sock_path.display(),
                resp.status()
            );
        }

        Ok(resume_end - resume_start)
    }
}

/// A standalone worker task's routine in case the `issue` subcommand has been provided.
async fn task_issue(
    id: usize,
    address_port: String,
    bcmd: &BenchCmd,
    num_prewarm: usize,
    barrier: Arc<Barrier>,
) -> Result<(usize, Measurement)> {
    // Allocations (before the timer begins)
    let addr_port = address_port.clone();
    barrier.wait().await;

    // Issue the "cold" request (also timing it with the global timer)
    let global_start = Instant::now();
    let cold = bcmd.issue(address_port).await?;
    let global = Instant::now() - global_start;
    barrier.wait().await;

    // Asynchronously pre-warm in parallel, if necessary
    if num_prewarm > 0 {
        let mut rng: StdRng = SeedableRng::from_entropy();
        for i in 0..num_prewarm {
            let _ = bcmd
                .issue(addr_port.clone())
                .await
                .with_context(|| format!("ID={id} failed during pre-warming (round: {i})"))?;
            sleep(Duration::from_millis(rng.gen_range(20..120))).await;
        }
    }
    barrier.wait().await;

    // Issue the "warm" request
    let warm = bcmd.issue(addr_port).await?;
    barrier.wait().await;

    let m = (global, cold.into(), warm.into()).into();
    Ok((id, m))
}

/// A standalone worker task's routine in case the `restore` subcommand has been provided.
async fn task_restore(
    id: usize,
    address_port: String,
    mut rcmd: RestoreCmd,
    num_prewarm: usize,
    barrier: Arc<Barrier>,
) -> Result<(usize, Measurement)> {
    // Validation, pre-processing and allocations (before the timer begins)
    rcmd.validate(id)
        .with_context(|| format!("failed to validate arguments for ID={id}"))?;
    let addr_port = address_port.clone();
    barrier.wait().await;

    // Start the global timer and restore the uVM from the snapshot
    let global_start = Instant::now();
    let restore = rcmd.restore().await?;
    barrier.wait().await;

    // Resume the uVM restored from the snapshot
    let resume = rcmd.resume().await?;
    barrier.wait().await;

    // Issue the "cold" request and stop the global timer
    let cold = rcmd.bench.issue(address_port).await?;
    let global = Instant::now() - global_start;
    barrier.wait().await;

    // Asynchronously pre-warm in parallel, if necessary
    if num_prewarm > 0 {
        let mut rng: StdRng = SeedableRng::from_entropy();
        for i in 0..num_prewarm {
            let _ = rcmd
                .bench
                .issue(addr_port.clone())
                .await
                .with_context(|| format!("ID={id} failed during pre-warming (round: {i})"))?;
            sleep(Duration::from_millis(rng.gen_range(20..120))).await;
        }
    }
    barrier.wait().await;

    // Issue the "warm" request
    let warm = rcmd.bench.issue(addr_port).await?;
    barrier.wait().await;

    let m = (global, restore, resume, cold.into(), warm.into()).into();
    Ok((id, m))
}

#[tokio::main]
async fn main() -> Result<()> {
    // Parse the command line arguments
    let cli = Cli::parse();
    // Format, validate & construct the IP addresses and ports based on the provided argument
    let mut addrs = cli
        .validate_addrs()
        .with_context(|| "failed to parse and validate the IP addresses given")?;

    // Prepend scheme to every `ADDR:PORT`, to be ready for use in a URL.
    addrs.iter_mut().for_each(|s| s.insert_str(0, "http://"));

    // (Un)initialize a Vec to add the resulting Measurements as they are received from the workers
    let mut measurements: Vec<MaybeUninit<Measurement>> = Vec::with_capacity(cli.num_uvms);
    // SAFETY: This `Vec` will not be read before it gets filled up with the (properly initialized)
    // `Measurement` structs that the worker tasks return upon their completion.
    unsafe { measurements.set_len(cli.num_uvms) };

    // Spawn the tasks that do the actual work (depending on the provided subcommand)
    let mut workers = Vec::with_capacity(cli.num_uvms);
    let barrier = Arc::new(Barrier::new(cli.num_uvms));
    for (id, addr) in addrs.into_iter().enumerate() {
        let barrier = barrier.clone();
        match &cli.top_cmd {
            &TopSubcommand::Issue(bcmd) => workers.push(tokio::spawn(async move {
                task_issue(id, addr, &bcmd, cli.pre_warm, barrier).await
            })),
            TopSubcommand::Restore(rcmd) => {
                let rcmd = rcmd.clone();
                workers.push(tokio::spawn(async move {
                    task_restore(id, addr, rcmd, cli.pre_warm, barrier).await
                }))
            }
        };
    }

    // Join all tasks
    for res in try_join_all(workers)
        .await
        .with_context(|| "could not join worker tasks")?
    {
        let (i, m) = res.with_context(|| "failed to retrieve workers' results after joining")?;
        measurements[i].write(m);
    }
    // SAFETY: This `Vec` was pre-allocated to be of the exact size (in # of `Measurement` structs)
    // as the number of MicroVMs, which is also the number of worker tasks, and thus must have been
    // fully initialized upon their completion; therefore, it is filled up with properly
    // initialized `Measurement` structs by now.
    let measurements = unsafe { std::mem::transmute::<_, Vec<Measurement>>(measurements) };

    // Print resulting Measurements to stdout
    for (id, measurement) in measurements.iter().enumerate() {
        println!("{},{}", id, measurement);
    }
    Ok(())
}
