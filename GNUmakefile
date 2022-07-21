#
# GNUmakefile
#
# ckatsak, Tue 01 Feb 2022 12:00:19 AM EET
#
# XXX: For now, this Makefile takes advantage of GNU make's implementation of
# overriding previous defined rules with the last one provided. Note that this
# is probably to be a GNU-specific behavior[1], so it may not work as expected
# in different make implementations.
#
# [1]: https://stackoverflow.com/a/61524093

# Before modifying the tag/version, make sure everything is in sync:
#     $ rg -ni '0.0.1' -g !'*.json'  -g !'*.csv' .
TAG := 0.0.2
DOCKER ?= docker

# FIXME(ckatsak): This is ugly, and should be set via the config file, but for
# now, this env var can modify the number of rootfs images and snapshots that
# are being built.
MANY ?= 64


BENCHES := $(wildcard benches/*)  # ( benches/pyaes benches/matmul_fb ... )
ROOTFS := $(BENCHES:benches/%=rootfs/%)  # (rootfs/pyaes rootfs/matmul_fb ...)
SNAPSHOTS := $(BENCHES:benches/%=snapshot/%)  # (snapshot/pyaes snapshot/matmul_fb ...)
MULTI_ROOTFS := $(BENCHES:benches/%=multi-rootfs/%)  # (multi-rootfs/pyaes ...)
MULTI_SNAPSHOTS := $(BENCHES:benches/%=multi-snapshot/%)  # ( ... )


.PHONY: all proto benches $(BENCHES) rootfs $(ROOTFS) snapshots $(SNAPSHOTS) \
	cp_2M multi-rootfs $(MULTI_ROOTFS) multi-snapshots $(MULTI_SNAPSHOTS)

all: cp_2M client build-snapshots-rs

###############################################################################

multi-snapshots: $(MULTI_SNAPSHOTS)
$(MULTI_SNAPSHOTS):
	$(RM) -rv "snapshot/$(shell basename $@)"
	mkdir -vp "snapshot/$(shell basename $@)"
	scripts/build-snapshots-rs/target/release/build-snapshots \
		--bench "$(shell basename $@)" \
		--num-uvms $(MANY) \
		--vm-mem 512 \
		--store "$(CURDIR)/snapshot/$(shell basename $@)" \
		--cleanup

# XXX: (Only) GNU make overrides previous rules with the last one provided[1].
multi-snapshot/rnn_serving:
	$(warning Skipping benchmark 'rnn_serving', since it cannot boot \
		properly from its rootfs yet)

###############################################################################

multi-rootfs: $(MULTI_ROOTFS)
$(MULTI_ROOTFS):
ifneq ($(shell id -u),0)
	$(error $(@) needs to run as root, not $(shell id -u))
endif
	$(RM) -rv "rootfs/$(shell basename $@)"
	mkdir -vp "rootfs/$(shell basename $@)"
	BENCH=$(shell basename $@) \
	      MANY=$(MANY) \
	      ALPINE_IMG_TAG="ckatsak/fbpml-$(shell basename $@):$(TAG)" \
	      scripts/build_rootfs_multi.sh

# XXX: (Only) GNU make overrides previous rules with the last one provided[1].
multi-rootfs/video_processing:
ifneq ($(shell id -u),0)
	$(error $(@) needs to run as root, not $(shell id -u))
endif
	$(RM) -rv "rootfs/$(shell basename $@)"
	mkdir -vp "rootfs/$(shell basename $@)"
	BENCH=$(shell basename $@) \
	      MANY=$(MANY) \
	      DEBIAN_IMG_TAG="ckatsak/fbpml-$(shell basename $@):$(TAG)" \
	      benches/video_processing/scripts/build_rootfs_multi.sh

###############################################################################

benches: $(BENCHES)
$(BENCHES): proto
	-ln $(wildcard $</*.py) $@
	$(DOCKER) build \
		--progress=plain \
		--no-cache \
		--pull \
		-f $@/Dockerfile \
		-t ckatsak/fbpml-$(shell basename $@):$(TAG) \
		$@

###############################################################################

proto:
	$(MAKE) -C $@

###############################################################################

cp_2M:
	$(MAKE) -C scripts/$@

###############################################################################

RUST_DEBIAN_IMG := rust:1.60-slim-bullseye
.PHONY: client client-release-debian client-local
client: client-release-debian
client-release-debian:
	$(DOCKER) run \
		--rm \
		--user "$(shell id -u):$(shell id -g)" \
		--volume "$(CURDIR):/src" \
		$(RUST_DEBIAN_IMG) \
		bash -c 'rustup component add rustfmt \
			&& cd /src/fbpml-rs \
			&& cargo build --release \
			&& strip -s /src/fbpml-rs/target/release/fbpml-client \
			&& strip -s /src/fbpml-rs/target/release/fbpml-multiclient'
client-local:
	cd fbpml-rs \
		&& cargo build --release \
		&& strip -s target/release/fbpml-client \
		&& strip -s target/release/fbpml-multiclient

###############################################################################

.PHONY: build-snapshots-rs build-snapshots-rs-local
build-snapshots-rs:
	$(DOCKER) run \
		--rm \
		--user "$(shell id -u):$(shell id -g)" \
		--volume "$(CURDIR):/src" \
		$(RUST_DEBIAN_IMG) \
		bash -c 'rustup component add rustfmt \
			&& cd /src/scripts/build-snapshots-rs \
			&& cargo build --release \
			&& strip -s /src/scripts/build-snapshots-rs/target/release/build-snapshots'
build-snapshots-rs-local:
	cd scripts/build-snapshots-rs \
		&& cargo build --release \
		&& strip -s target/release/build-snapshots \

###############################################################################

.PHONY: clean clean-client cleaner distclean

clean:
	$(RM) -v $(shell find benches \
		-iname 'functionbench_pmem_local_pb2*.py')
	$(MAKE) -C scripts/cp_2M clean
	cd scripts/build-snapshots-rs; cargo clean

clean-client:
	cd fbpml-rs; cargo clean

cleaner: clean clean-client
	$(MAKE) -C proto clean

distclean: cleaner
	$(RM) -rv snapshot rootfs
	$(RM) -v $(shell find -iname '*.ext4')
	-@for d in $(shell ls benches); do \
		$(DOCKER) rmi ckatsak/fbpml-$$d:$(TAG); \
	done

