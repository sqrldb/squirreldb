# SquirrelDB Release Makefile

VERSION := $(shell grep '^version' Cargo.toml | head -1 | sed 's/.*"\(.*\)"/\1/')
DOCKER_REGISTRY ?= ghcr.io/anthropics
DOCKER_IMAGE := $(DOCKER_REGISTRY)/squirreldb

# Platforms for cross-compilation
PLATFORMS := x86_64-unknown-linux-gnu aarch64-unknown-linux-gnu x86_64-apple-darwin aarch64-apple-darwin

.PHONY: all version check test lint build build-all docker docker-push release clean help

all: check test build

version:
	@echo $(VERSION)

# === Quality Checks ===

check: lint test
	@echo "All checks passed"

lint:
	cargo fmt --check
	cargo clippy -- -D warnings

test:
	cargo test --all-features

bench:
	cargo bench

# === Build ===

build:
	cargo build --release

build-debug:
	cargo build

# Cross-compile for all platforms (requires cross: cargo install cross)
build-all: $(addprefix build-,$(PLATFORMS))

build-%:
	cross build --release --target $*
	mkdir -p dist/$*
	cp target/$*/release/sqrld dist/$*/
	cp target/$*/release/sqrl dist/$*/

# === Admin UI (WASM) ===

admin-ui:
	trunk build --release src/admin/index.html
	mkdir -p target/admin
	cp -r dist/admin/* target/admin/

# === Docker ===

docker:
	docker build -t $(DOCKER_IMAGE):$(VERSION) -t $(DOCKER_IMAGE):latest .

docker-push: docker
	docker push $(DOCKER_IMAGE):$(VERSION)
	docker push $(DOCKER_IMAGE):latest

# Multi-arch Docker build
docker-multiarch:
	docker buildx build \
		--platform linux/amd64,linux/arm64 \
		-t $(DOCKER_IMAGE):$(VERSION) \
		-t $(DOCKER_IMAGE):latest \
		--push .

# === Package ===

dist: build-all
	mkdir -p dist/release
	@for platform in $(PLATFORMS); do \
		tar -czvf dist/release/squirreldb-$(VERSION)-$$platform.tar.gz \
			-C dist/$$platform sqrld sqrl; \
	done
	@echo "Release archives created in dist/release/"

checksums:
	cd dist/release && sha256sum *.tar.gz > SHA256SUMS

# === Publish ===

publish-types:
	cd crates/types && cargo publish

publish-client: publish-types
	cd crates/client && cargo publish

publish-sqrl: publish-client
	cd crates/sqrl && cargo publish

publish-sqrld: publish-client
	cd crates/sqrld && cargo publish

publish-crates: publish-types publish-client publish-sqrl publish-sqrld
	@echo "All crates published to crates.io"

publish-crates-dry:
	cd crates/types && cargo publish --dry-run
	cd crates/client && cargo publish --dry-run
	cd crates/sqrl && cargo publish --dry-run
	cd crates/sqrld && cargo publish --dry-run
	@echo "Dry run complete. Run 'make publish-crates' to publish."

# === Release (full workflow) ===

release: check dist docker checksums
	@echo ""
	@echo "=== Release $(VERSION) ready ==="
	@echo ""
	@echo "Artifacts:"
	@ls -la dist/release/
	@echo ""
	@echo "Docker image: $(DOCKER_IMAGE):$(VERSION)"
	@echo ""
	@echo "Next steps:"
	@echo "  1. git tag v$(VERSION)"
	@echo "  2. git push origin v$(VERSION)"
	@echo "  3. make docker-push"
	@echo "  4. make publish-crates-force"
	@echo "  5. Create GitHub release with dist/release/* artifacts"

# === Utilities ===

clean:
	cargo clean
	rm -rf dist

deps:
	rustup target add $(PLATFORMS)
	cargo install cross
	cargo install trunk

# === Help ===

help:
	@echo "SquirrelDB Release Makefile"
	@echo ""
	@echo "Usage: make [target]"
	@echo ""
	@echo "Targets:"
	@echo "  version          Show current version"
	@echo "  check            Run lint and tests"
	@echo "  lint             Run cargo fmt and clippy"
	@echo "  test             Run all tests"
	@echo "  bench            Run benchmarks"
	@echo "  build            Build release binary (current platform)"
	@echo "  build-all        Cross-compile for all platforms"
	@echo "  admin-ui         Build WASM admin UI with trunk"
	@echo "  docker           Build Docker image"
	@echo "  docker-push      Push Docker image to registry"
	@echo "  docker-multiarch Build and push multi-arch Docker image"
	@echo "  dist             Create release archives for all platforms"
	@echo "  checksums        Generate SHA256 checksums"
	@echo "  publish-crates   Publish all crates to crates.io"
	@echo "  publish-crates-dry  Dry-run publish to crates.io"
	@echo "  release          Full release workflow (check, dist, docker)"
	@echo "  clean            Clean build artifacts"
	@echo "  deps             Install build dependencies"
	@echo "  help             Show this help"
	@echo ""
	@echo "Environment:"
	@echo "  DOCKER_REGISTRY  Docker registry (default: ghcr.io/anthropics)"
	@echo ""
	@echo "Current version: $(VERSION)"
