use std::{
    os::unix::prelude::FileTypeExt,
    path::{Path, PathBuf},
    time::Duration,
};

use anyhow::{bail, Context, Result};
use clap::{Parser, Subcommand};

use hyper::{Client, StatusCode};
use hyperlocal::{UnixClientExt, Uri};
use tokio::time::Instant;

use fbpml::{one_arg_rpc, two_args_rpc, zero_args_rpc, Measurement};
use fbpml_rpc::ServiceResponse;

/// A CLI for the gRPC clients of the benchmarks supported  in fbpml.
#[derive(Parser)]
#[clap(author, version, about, long_about = None)]
#[clap(propagate_version = true)]
struct Cli {
    /// IP Address and TCP port of the gRPC server against which the requests will be issued, in
    /// the 'ADDRESS:PORT' format.
    #[clap(short = 'c', long = "server-addr")]
    address_port: String,

    #[clap(subcommand)]
    top_cmd: TopSubcommand,
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

#[derive(clap::Args)]
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

#[derive(Subcommand)]
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
    fn validate(&self) -> Result<()> {
        // Validate uVM's API socket's path
        if !self
            .api_sock_path
            .metadata()
            .with_context(|| format!("could not stat(2) '{}'", self.api_sock_path.display()))?
            .file_type()
            .is_socket()
        {
            bail!("'{}' is not a Unix socket", self.api_sock_path.display());
        }

        // Validate uVM's state and memory files
        let validate_file = |f: &Path| {
            if !f
                .metadata()
                .with_context(|| format!("could not stat(2) '{}'", f.display()))?
                .is_file()
            {
                bail!("'{}' should point to a plain (binary) file", f.display());
            }
            Ok(())
        };
        validate_file(&self.state_file)?;
        validate_file(&self.memory_file)?;

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

#[tokio::main]
async fn main() -> Result<()> {
    let mut cli = Cli::parse();

    // Prepend scheme to ADDR:PORT, to be ready for use in a URL.
    cli.address_port.insert_str(0, "http://");
    // Also clone it here, to keep the extra allocation out of the global timer.
    let addr_port = cli.address_port.clone();

    let m: Measurement = match &cli.top_cmd {
        TopSubcommand::Issue(bench_cmd) => {
            let global_start = Instant::now();
            let cold = bench_cmd.issue(addr_port).await?;
            let global_delay = Instant::now() - global_start;

            let warm = bench_cmd.issue(cli.address_port).await?;

            (global_delay, cold.into(), warm.into()).into()
        }

        TopSubcommand::Restore(rcmd) => {
            rcmd.validate()
                .with_context(|| "failed to validate arguments")?;

            let global_start = Instant::now();
            let restore = rcmd.restore().await?;
            let resume = rcmd.resume().await?;
            let cold = rcmd.bench.issue(addr_port).await?;
            let global = Instant::now() - global_start;

            let warm = rcmd.bench.issue(cli.address_port).await?;

            (global, restore, resume, cold.into(), warm.into()).into()
        }
    };

    println!("{m}");
    Ok(())
}
