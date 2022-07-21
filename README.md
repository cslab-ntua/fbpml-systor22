<!-- # fbpml -->
```text
                          _    _                        _  
                         | |  | |                      | | 
                         | |  | |      _    _  _  _    | | 
                         |/   |/ \_  |/ \_ / |/ |/ |   |/  
                         |__/  \_/   |__/    |  |  |_/ |__/
                         |\         /|                     
                         |/         \|                     
```

## Citing our Work

If you find any part of this work relevant or useful, please consider citing
[our paper at SYSTOR '22](https://dl.acm.org/doi/abs/10.1145/3534056.3534938):

```bibtex
@inproceedings{10.1145/3534056.3534938,
  author = {Katsakioris, Christos and Alverti, Chloe and Karakostas, Vasileios and Nikas, Konstantinos and Goumas, Georgios and Koziris, Nectarios},
  title = {FaaS in the Age of (sub-)μs I/O: A Performance Analysis of Snapshotting},
  year = {2022},
  isbn = {9781450393805},
  publisher = {Association for Computing Machinery},
  address = {New York, NY, USA},
  url = {https://doi.org/10.1145/3534056.3534938},
  doi = {10.1145/3534056.3534938},
  booktitle = {Proceedings of the 15th ACM International Conference on Systems and Storage},
  pages = {13–25},
  numpages = {13},
  keywords = {storage, persistent memory, snapshots, FaaS, serverless, virtualization},
  location = {Haifa, Israel},
  series = {SYSTOR '22}
}
```

---

# Brief Instructions

> **Warning**:
> This project currently takes advantage of a feature (rule overriding) that is
> present only in GNU make.

> **Note**:
> Check and modify the [`config`](config) file to configure some of the global
> variables used throughout the project.
> Sadly, some of the configuration may still reside inside the scripts themselves
> for the time being.

## Install `fforget`

[`fforget`](https://github.com/ckatsak/fforget-bin) allows hinting Linux to evict
specific files from the page cache, and also makes sure they are indeed evicted.

To install it from GitHub through `cargo`:

```console
$ cargo install --git https://github.com/ckatsak/fforget-bin
```

Alternatively, you can [download it](https://github.com/ckatsak/fforget-bin/releases/tag/v0.0.3)
and build it locally.

## Build the Client CLIs

Containerized build based on Debian and [Rust 1.60](https://rustup.rs/):

```console
$ make client
```

or using a local toolchain (assuming it can be found in `$PATH`):

```console
$ make client-local
```

Check the built-in help menu for their use:

```console
$ fbpml-client --help
```

```console
$ fbpml-multiclient --help
```

## Build `cp_2M`

To build it (using the local C compiler):

```console
$ make cp_2M
```

## Build `build-snapshots-rs`

Containerized build based on Debian and [Rust 1.60](https://rustup.rs/):

```console
$ make build-snapshots-rs
```

or using the local Rust installation:

```console
$ make build-snapshots-rs-local
```

Check the [Makefile](GNUmakefile) or the built-in help menu for its use:

```console
$ scripts/build-snapshots-rs/target/release/build-snapshots --help
```

Mind that it picks up environment variables from the [config file](config).

---

The three steps above, combined, also constitute the default target (`cp_2M` +
`client` + `build-snapshots-rs`):

```console
$ make
```

## Build the OCI Images

To build the required OCI images locally:

```console
$ make benches
```

## Network Interfaces

To configure the tap interfaces to be used by the MicroVMs, as well as the
required iptables rules, you can use the [`host_net.sh`](scripts/host_net.sh)
script.

For example, to set up 16 such interfaces (with "IDs" [0-15] inclusive):

```console
# for i in $(seq 0 1 15); do ./scripts/host_net.sh "$i" 'enp1s0f0'; done
```

Later, when you finish, you can use the [`host_net_cleanup.sh`](scripts/host_net_cleanup.sh)
script to clean them up.

## Setup MinIO

Download the MinIO server binary:

```console
$ wget https://dl.min.io/server/minio/release/linux-amd64/minio
$ chmod +x minio
```

Then, run it binding it to `0.0.0.0:59000` (so that it is reachable from all
MicroVMs' interfaces).

```console
$ MINIO_ROOT_USER=minioroot MINIO_ROOT_PASSWORD=minioroot \
> "$MINIO_SERVER" server --address ':59000' /tmp/minio-data
```

Each benchmark expects the MinIO server to be reachable through its tap interface
and at the specific port.
This is why:
- we provide the `--address` option, to let the server bind to `INADDR_ANY`;
- all tap interfaces must have been created earlier (frankly, I'm not sure if this
is a strict requirement).

In addition, all benchmarks expect to be able to communicate with the MinIO server
with these specific credentials (environment variables `MINIO_ROOT_USER` and
`MINIO_ROOT_PASSWORD`, which correspond to the `access_key` and the `secret_key`,
respectively).
These are all hardcoded in benchmarks' source code.

The given directory is its drive, where (some of the) functions' input will reside.
On our experiments, we point it to a directory on a tmpfs mount, so that all
data remain in-memory.

Subsequently, we upload the input files through the web UI (at `http://<HOST-IP>:59000`,
which redirects to an ephemeral console port).

Alternatively, after you download minio server & client and properly set the correct
paths in the [`config` file](config), you can use the [`setup_minio.sh`](./scripts/setup_minio.sh)
script, which automates the whole process (although mind to use `numactl` or `taskset`
to pin it to the correct NUMA node).

Benchmarks have their input file names hardcoded too, and assume they all live
in the (same) `fbpml` bucket.

## Single MicroVM

> **Warning**:
> This section has been deprecated; see the next (Multiple MicroVMs)
> section, where you may run single-uVM experiments by setting `MANY=1`.

## Multiple MicroVMs

Building all rootfs images (1.54GiB each) and snapshots to run _all_ benchmarks
may require quite a lot of storage capacity (also depending on guest memory size).

Benchmarks that perform I/O from Minio have an additional constraint.
For now, a uVM's clock is synced when the snapshot is created, but not when the
snapshot is loaded (i.e., right before serving a function invocation).
However, [Minio does not accept requests from clients with >15min skew time](https://github.com/minio/minio-java/issues/701#issuecomment-442085617).
As a result, for now, a snapshot is really only usable for a 15min window after
creating it (for benchmarks that use Minio).
I may fix it some time in the future.

For these reasons, you may prefer to build/run them one by one.

### Build the rootfs Images

```console
# make MANY=16 multi-rootfs/$BENCHMARK_NAME
```

To build them all at once:

```console
$ make multi-rootfs
```

### Build the Snapshots

```console
$ make MANY=16 multi-snapshot/$BENCHMARK_NAME
```

To build them all at once:

```console
$ make multi-snapshots
```

### Run

In case you want the rootfs to live in-memory (and maybe the snapshots too, to
move them around quickly before each run), first:

```console
# mount -t tmpfs tmpfs /nvme/ckatsak/fbpml/rootfs/
# mount -t tmpfs tmpfs /nvme/ckatsak/fbpml/snapshot/
```

> **Note**:
> In this case, also mind to modify [`run_multi.sh`](run_multi.sh) to not
> attempt to `fforget` the rootfs image (at `$ROOTFS_PATH/$bench"/*.ext4`)
> between the runs, since it will always fail because of the tmpfs mount.

For each benchmark (really, for each `(benchmark, # uvms)` pair), after building
the rootfs images and the corresponding snapshots, run:

```console
$ ./run_multi.sh -b 'chameleon' --num-uvms 16 --outdir '/nvme/ckatsak/fbpml_outdir_n16' --runs 10 -p '/mnt/pmem0/ckatsak/fbpml_2304Mi' -n '/nvme/ckatsak/fbpml_2304Mi' -s '/opt/ckatsak/fbpml_2304Mi'
```

> **Note**:
> By omitting device path flags, runs on the respective devices can be skipped.

Then, you may (optionally) manually clean all directories where rootfs and
snapshots have been created or copied over, to make room for the next benchmark:

```console
$ rm -rf /mnt/pmem0/ckatsak/fbpml_2304Mi/* /nvme/ckatsak/fbpml_2304Mi/* /opt/ckatsak/fbpml_2304Mi/*
$ rm -rf rootfs/chameleon/* snapshot/chameleon/*
```

For more information, you may also check:

```console
$ ./run_multi.sh --help
```

> **Note**:
> You may find [`quick_run.sh`](quick_run.sh) useful too, as an example on how
> `run_multi.sh` is expected to be called.

> **Warning**:
> The [`physical_cores()`](run_multi.sh#L101-L117) function in `run_multi.sh`
> parses a NUMA node's physical cores' IDs from the output of `lscpu`. This has
> only been tested on a couple of machines. You probably have to verify that it
> works on yours as well, or you may need to modify it accordingly if it doesn't.

## Cleanup

Cleaning up probably needs some... cleaning up.
Nevertheless, for now:

- To clean only the autogenerated protobuf & gRPC Python code in [`benches/`](benches)
and `cp_2M`:

```console
$ make clean
```

- To (`cargo`-)clean only the [client CLIs](fbpml-rs):

```console
$ make clean-client
```

- To also clean the autogenerated protobuf & gRPC Python code in [`proto/`](proto),
in addition to the two targets above:

```console
$ make cleaner
```

- To also untag (thus possibly remove) every related OCI image, remove all produced
rootfs images, as well as every snapshot, in addition to the above (i.e., to
nuke all changes):

```console
$ make distclean
```
