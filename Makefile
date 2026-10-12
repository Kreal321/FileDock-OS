# FileDock OS build entry points. See specs/001-core-os-foundation/quickstart.md.

CARGO  ?= cargo
DOTNET ?= dotnet
PNPM   ?= pnpm
BUF    ?= buf
UV     ?= uv

DIST    ?= dist
RID     ?= linux-x64
CONFIG  ?= Release

MGMT_SLN := management/FileDock.Management.sln
MGMT_API := management/src/FileDock.Management.Api/FileDock.Management.Api.csproj
MGMT_CLI := management/src/FileDock.Management.Cli/FileDock.Management.Cli.csproj
MGMT_CORE := management/src/FileDock.Management.Core/FileDock.Management.Core.csproj

.PHONY: all proto core web management packages bundle \
        test test-core test-management test-web test-contracts test-e2e \
        lint clean

all: proto core management

# gRPC contracts: lint, then generate the Rust (tonic) and C# (Grpc.Tools) stubs.
proto:
	$(BUF) lint proto
	$(CARGO) build -p fd-api
	$(DOTNET) build $(MGMT_CORE)

# Core Service and the native S3 connector.
core:
	$(CARGO) build --release --workspace

# The SPA is built into the Management Service's wwwroot, then published with it.
web:
	$(PNPM) -C web install --frozen-lockfile
	$(PNPM) -C web build

management: web
	$(DOTNET) publish $(MGMT_API) -c $(CONFIG) -r $(RID) --self-contained -o $(DIST)/management/api
	$(DOTNET) publish $(MGMT_CLI) -c $(CONFIG) -r $(RID) --self-contained -o $(DIST)/management/cli

# filedock-core, filedock-management, and filedock-builtins .debs (packaging/debian, task T050).
packages: core management
	@test -x packaging/debian/build-packages.sh || \
		{ echo "packaging/debian/build-packages.sh does not exist yet (task T050)"; exit 1; }
	packaging/debian/build-packages.sh $(DIST)

bundle: packages
	sudo packaging/debian/build-bundle.sh $(DIST)

test: test-core test-management test-web test-contracts

test-core:
	$(CARGO) nextest run --workspace --no-tests=pass

# Microsoft.Testing.Platform runner (global.json). Exit code 8 means zero tests ran, which is
# expected until the test projects have tests.
test-management:
	$(DOTNET) test --solution $(MGMT_SLN) --ignore-exit-code 8

test-web:
	$(PNPM) -C web test

test-contracts:
	$(UV) run --project tests/contracts pytest tests/contracts

# VM acceptance: libvirt or FD_E2E_EXISTING_HOST; see phase3-testing.md for PVE.
test-e2e:
	$(UV) run --project tests/e2e pytest tests/e2e

lint:
	$(BUF) lint proto
	$(CARGO) fmt --all --check
	$(CARGO) clippy --workspace --all-targets -- -D warnings
	$(DOTNET) format $(MGMT_SLN) --verify-no-changes
	$(PNPM) -C web lint
	$(PNPM) -C web format:check

clean:
	$(CARGO) clean
	$(DOTNET) clean $(MGMT_SLN)
	rm -rf $(DIST) web/dist management/src/FileDock.Management.Api/wwwroot
