use std::{
    path::{Path, PathBuf},
    process::Stdio,
    sync::Arc,
    time::Duration,
};

use anyhow::{anyhow, Context, Result};
use clap::Parser;
use futures::future;
use hyper::{Client, Request, StatusCode};
use hyperlocal::{UnixClientExt, UnixConnector, Uri};
use indicatif::{ProgressBar, ProgressStyle};
use rand::{prelude::StdRng, Rng, SeedableRng};
use tokio::{fs, net::TcpStream, process::Command, time::sleep};

const VM_ADDR_FMT: &str = "10.0.ID.2:50051";

const ACCEPT: &str = "Accept";
const CONTENT_TYPE: &str = "Content-Type";
const APPLICATION_JSON: &str = "application/json";

#[derive(Clone, Parser)]
#[clap(author, version, about, long_about = None)]
#[clap(propagate_version = true)]
struct Cmd {
    /// The name of the benchmark to create snapshots for.
    #[clap(short = 'b', long = "bench")]
    bench: String,

    /// Number of uVMs to create snapshots for.
    #[clap(short = 'n', long = "num-uvms")]
    num_uvms: u64,

    /// Guest memory size for the uVMs to be snapshotted.
    #[clap(short = 'm', long = "vm-mem")]
    vm_mem: u64,

    /// Number of VCPUs of the guest uVMs to be snapshotted.
    #[clap(long = "vcpu-count", default_value = "1")]
    vcpu_count: u64,

    /// Directory where snapshot files must be stored.
    #[clap(short = 'p', long = "store")]
    store_path: PathBuf,

    /// Cleanup (configs, logs, metrics) after creating the snapshots.
    #[clap(short = 'r', long = "cleanup")]
    cleanup: bool,

    /// Directory where the rootfs images of the uVMs are stored.
    #[clap(long = "rootfs-dir", env = "ROOTFS_DIR")]
    rootfs_dir: PathBuf,

    /// Path to the firecracker binary.
    #[clap(long = "fc-bin", env = "FC_BIN")]
    fc_bin: PathBuf,

    /// Path to the Linux kernel image.
    #[clap(long = "kernel-image-path", env = "KERNEL_IMG_PATH")]
    kernel_image_path: PathBuf,
}

async fn create_dirs(store: impl AsRef<Path>) -> Result<()> {
    let store = store.as_ref().to_str().expect("non-utf8 store path");
    let _ = future::try_join(
        fs::create_dir_all(PathBuf::from_iter(&[store, "logs"])),
        fs::create_dir_all(PathBuf::from_iter(&[store, "metrics"])),
    )
    .await
    .with_context(|| "failed to create directory trees for logs & metrics")?;
    Ok(())
}

async fn cleanup_dirs(store: impl AsRef<Path>) -> Result<()> {
    let store = store.as_ref().to_str().expect("non-utf8 store path");
    let _ = future::try_join(
        fs::remove_dir_all(PathBuf::from_iter(&[store, "logs"])),
        fs::remove_dir_all(PathBuf::from_iter(&[store, "metrics"])),
    )
    .await
    .with_context(|| "failed to cleanup logs & metrics")?;
    Ok(())
}

/// Attempt to `connect(2)` to the given `address_port` for a couple of minutes (i.e., every
/// ~500ms, up to 240 retries).
async fn wait_port(address_port: &str, rng: &mut StdRng) -> Result<()> {
    let mut retries = 240;
    while retries > 0 {
        if TcpStream::connect(address_port).await.is_ok() {
            return Ok(());
        }
        sleep(Duration::from_millis(rng.gen_range(400..600))).await;
        retries -= 1;
    }
    Err(anyhow!("failed to connect to '{address_port}'"))
}

/// Truncate the logging and metrics files
async fn truncate_files(
    id: u64,
    bench: &str,
    store: impl AsRef<Path>,
) -> Result<(PathBuf, PathBuf)> {
    let common_basename = format!("fc-{}-{id:02X}", bench);

    let mut logs = store.as_ref().to_path_buf();
    logs.push("logs");
    logs.push(&common_basename);
    logs.set_extension("log");

    let mut metrics = store.as_ref().to_path_buf();
    metrics.push("metrics");
    metrics.push(&common_basename);
    metrics.set_extension("metrics");

    let _ = future::try_join(
        fs::OpenOptions::new()
            .write(true)
            .create(true)
            .truncate(true)
            .open(logs.as_path()),
        fs::OpenOptions::new()
            .write(true)
            .create(true)
            .truncate(true)
            .open(metrics.as_path()),
    )
    .await?;

    Ok((logs, metrics))
}

async fn render_config(id: u64, args: &Cmd) -> Result<PathBuf> {
    let (logs, metrics) = truncate_files(id, &args.bench, &args.store_path)
        .await
        .with_context(|| "ID={id} failed to truncate logs & metrics files")?;

    let mut rootfs = PathBuf::from(args.rootfs_dir.as_path());
    rootfs.push(args.bench.as_str());
    rootfs.push(format!("{}-{id:02X}.ext4", args.bench));

    // The following probably does way too many allocations, but nvm cuz we're fast enough anyway.
    // Crate `tinytemplate` might be a better alternative, although I have not really evaluated it.
    let config = MICROVM_CONFIG_TEMPLATE
        .replace(
            "{KERNEL_IMG_PATH}",
            args.kernel_image_path
                .to_str()
                .expect("non-utf8 kernel image path"),
        )
        .replace(
            "{ROOTFS_PATH_ON_HOST}",
            rootfs.to_str().expect("non-utf8 rootfs path"),
        )
        .replace("{VM_MEM_MB}", args.vm_mem.to_string().as_str())
        .replace("{VCPU_COUNT}", args.vcpu_count.to_string().as_str())
        .replace("{LOG_PATH}", logs.to_str().expect("non-utf8 logs path"))
        .replace(
            "{METRICS_PATH}",
            metrics.to_str().expect("non-utf8 metrics path"),
        )
        .replace("{IDh}", format!("{id:02X}").as_str());

    let config_path = PathBuf::from(format!("/tmp/{}-{id:02X}.json", args.bench));
    fs::write(&config_path, config.as_bytes())
        .await
        .with_context(|| {
            format!(
                "ID={id} failed to write config to file {}",
                config_path.display()
            )
        })?;
    Ok(config_path)
}

/// Pause the uVM listening to socket `sock` using UDS client `ucli`.
async fn pause_uvm(sock: impl AsRef<Path>, ucli: &Client<UnixConnector>) -> Result<()> {
    const PAUSE_BODY: &str = r#"{"state":"Paused"}"#;

    let uri = Uri::new(&sock, "/vm");
    let req = Request::patch(uri)
        .header(ACCEPT, APPLICATION_JSON)
        .header(CONTENT_TYPE, APPLICATION_JSON)
        .body(PAUSE_BODY.into())
        .with_context(|| {
            format!(
                "failed to construct HTTP PATCH request for '{}' to pause the uVM",
                sock.as_ref().display()
            )
        })?;

    let resp = ucli.request(req).await.with_context(|| {
        format!(
            "failed to  HTTP PATCH /vm  @  '{}'",
            sock.as_ref().display()
        )
    })?;

    match resp.status() {
        StatusCode::NO_CONTENT => Ok(()),
        code => Err(anyhow!(
            "Endpoint '{}' responded with {code}",
            sock.as_ref().display()
        )),
    }
}

/// Resume the uVM listening to socket `sock` using UDS client `ucli`.
async fn resume_uvm(sock: impl AsRef<Path>, ucli: &Client<UnixConnector>) -> Result<()> {
    const RESUME_BODY: &str = r#"{"state":"Resumed"}"#;

    let uri = Uri::new(&sock, "/vm");
    let req = Request::patch(uri)
        .header(ACCEPT, APPLICATION_JSON)
        .header(CONTENT_TYPE, APPLICATION_JSON)
        .body(RESUME_BODY.into())
        .with_context(|| {
            format!(
                "failed to construct HTTP PATCH request for '{}' to pause the uVM",
                sock.as_ref().display()
            )
        })?;

    let resp = ucli.request(req).await.with_context(|| {
        format!(
            "failed to  HTTP PATCH /vm  @  '{}'",
            sock.as_ref().display()
        )
    })?;

    match resp.status() {
        StatusCode::NO_CONTENT => Ok(()),
        code => Err(anyhow!(
            "Endpoint '{}' responded with {code}",
            sock.as_ref().display()
        )),
    }
}

/// Create a snapshot for the uVM listening to socket `sock` using UDS client `ucli`.
async fn create_snapshot(
    id: u64,
    args: &Cmd,
    sock: impl AsRef<Path>,
    ucli: &Client<UnixConnector>,
) -> Result<()> {
    let mut sp = args.store_path.to_path_buf();
    sp.push(format!("snapshot-{id:02X}.file"));
    let mut mp = args.store_path.to_path_buf();
    mp.push(format!("memory-{id:02X}.file"));

    let restore_body = format!(
        r#"{{"snapshot_path":"{}","mem_file_path":"{}","snapshot_type":"Full"}}"#,
        sp.display(),
        mp.display()
    );

    let uri = Uri::new(&sock, "/snapshot/create");
    let req = Request::put(uri)
        .header(ACCEPT, APPLICATION_JSON)
        .header(CONTENT_TYPE, APPLICATION_JSON)
        .body(restore_body.into())
        .with_context(|| {
            format!(
                "failed to construct HTTP PUT request for '{}' to snapshot the uVM",
                sock.as_ref().display()
            )
        })?;

    let resp = ucli.request(req).await.with_context(|| {
        format!(
            "failed to  HTTP PUT /snapshot/create  @  '{}'",
            sock.as_ref().display()
        )
    })?;

    match resp.status() {
        StatusCode::NO_CONTENT => Ok(()),
        code => Err(anyhow!(
            "Endpoint '{}' responded with {code}",
            sock.as_ref().display()
        )),
    }
}

async fn snapshot_task(
    id: u64,
    args: Cmd,
    ucli: Client<UnixConnector>,
    mut rng: StdRng,
    pb: Arc<ProgressBar>,
) -> Result<()> {
    let address_port = VM_ADDR_FMT.replace("ID", id.to_string().as_str());

    // Create the path to the UDS and remove any present socket
    let sock = PathBuf::from(format!("/tmp/firecracker-{}-{id:02X}.socket", args.bench));
    if fs::metadata(sock.as_path()).await.is_ok() {
        fs::remove_file(sock.as_path())
            .await
            .with_context(|| format!("failed to remove socket at path '{}'", sock.display()))?;
    }

    // Setup any necessary configuration
    let config_path = render_config(id, &args)
        .await
        .with_context(|| "ID={id} failed to render configuration")?;

    // Spawn the uVM (replace all "_" in benchmark's name with "-" to be a valid Firecracker id)
    let mut fc = Command::new(&args.fc_bin)
        .arg("--id")
        .arg(format!("{}-{id:02X}", args.bench.replace('_', "-")))
        .arg("--config-file")
        .arg(&config_path)
        .arg("--api-sock")
        .arg(&sock)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .spawn()
        .with_context(|| format!("ID={id} failed to fork Firecracker"))?;

    // Wait until the gRPC server inside the uVM is responsive
    wait_port(&address_port, &mut rng)
        .await
        .with_context(|| format!("ID={id} failed to connect to the gRPC server"))?;

    // ¿FIXME?(ckatsak): Wait some more (with jitter)?
    sleep(Duration::from_millis(rng.gen_range(300..750))).await;

    // Pause it
    pause_uvm(&sock, &ucli)
        .await
        .with_context(|| format!("ID={id} failed to pause uVM"))?;

    // Create a snapshot from it
    create_snapshot(id, &args, &sock, &ucli)
        .await
        .with_context(|| format!("ID={id} failed to create snapshot for uVM"))?;

    // Resume it and poll the gRPC server inside it again
    resume_uvm(&sock, &ucli)
        .await
        .with_context(|| format!("ID={id} failed to resume uVM"))?;
    wait_port(&address_port, &mut rng)
        .await
        .with_context(|| format!("ID={id} failed to connect to the gRPC server"))?;
    pb.inc(1);

    // Kill & reap it
    fc.kill()
        .await
        .with_context(|| "ID={id} failed to kill & reap the Firecracker process")?;

    if args.cleanup {
        future::try_join(fs::remove_file(&sock), fs::remove_file(&config_path))
            .await
            .with_context(|| "ID={id} failed to cleanup uVM's config & socket")?;
    }
    Ok(())
}

#[tokio::main]
async fn main() -> Result<()> {
    let _ = dotenv::from_filename("config")
        .with_context(|| r#"failed to read environment variables from parents' "config" file"#)?;
    let cmd = Cmd::parse();

    let pb = ProgressBar::new(cmd.num_uvms);
    pb.set_style(
        ProgressStyle::with_template("{spinner} [{elapsed_precise}] {wide_bar} {pos}/{len}")
            .with_context(|| "error setting up the progress bar")?,
    );
    pb.inc(0);
    let pb = Arc::new(pb);

    create_dirs(&cmd.store_path).await?;

    future::try_join_all((0..cmd.num_uvms).map(|id| {
        let args = cmd.clone();
        let rng = SeedableRng::from_entropy();
        let pb = pb.clone();
        tokio::spawn(async move { snapshot_task(id, args, Client::unix(), rng, pb).await })
    }))
    .await
    .with_context(|| "failed to join worker tasks")?
    .into_iter()
    .collect::<Result<Vec<_>, _>>()
    .with_context(|| "at least one worker task failed")?;

    pb.finish_with_message("snapshots are ready!");

    if cmd.cleanup {
        cleanup_dirs(&cmd.store_path).await?;
    }
    Ok(())
}

const MICROVM_CONFIG_TEMPLATE: &str = r##"{
    "boot-source": {
        "kernel_image_path": "{KERNEL_IMG_PATH}",
        "boot_args": "8250.nr_uarts=0 reboot=k panic=1 pci=off ro noapic nomodules random.trust_cpu=on transparent_hugepage=always"
    },
    "drives": [
        {
            "drive_id": "rootfs",
            "path_on_host": "{ROOTFS_PATH_ON_HOST}",
            "is_root_device": true,
            "is_read_only": true
        }
    ],
    "machine-config": {
        "mem_size_mib": {VM_MEM_MB},
        "vcpu_count": {VCPU_COUNT},
        "smt": false
    },
    "logger": {
        "log_path": "{LOG_PATH}",
        "level": "Warning",
        "show_level": true,
        "show_log_origin": true
    },
    "metrics": {
        "metrics_path": "{METRICS_PATH}"
    },
    "network-interfaces": [
        {
            "iface_id": "eth0",
            "guest_mac": "AA:FC:00:00:05:{IDh}",
            "host_dev_name": "fcpmem01.{IDh}"
        }
    ]
}"##;
