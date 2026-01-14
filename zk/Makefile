# Makefile for building and testing all ZK examples
# Usage:
#   make all        - Build and test all examples
#   make build      - Build all Solana programs
#   make test-rust  - Run all Rust tests
#   make test-ts    - Run all TypeScript tests
#   make clean      - Clean all build artifacts

SHELL := /bin/bash

# ZK example directories
ZK_EXAMPLES := zk-nullifier zk-id zk-merkle-proof

.PHONY: all build deploy test-rust test-ts clean setup help $(ZK_EXAMPLES)

help:
	@echo "ZK Examples Makefile"
	@echo ""
	@echo "Usage:"
	@echo "  make all        - Build and test all examples (Rust + TypeScript)"
	@echo "  make build      - Build all Solana programs"
	@echo "  make deploy     - Deploy all programs to validator (builds first)"
	@echo "  make test-rust  - Run all Rust tests (cargo test-sbf)"
	@echo "  make test-ts    - Run all TypeScript tests (deploys first)"
	@echo "  make setup      - Setup circuits for all examples"
	@echo "  make clean      - Clean all build artifacts"
	@echo ""
	@echo "Individual examples:"
	@echo "  make zk-nullifier      - Build and test zk-nullifier"
	@echo "  make zk-id             - Build and test zk-id"
	@echo "  make zk-merkle-proof   - Build and test zk-merkle-proof"

all: build test-rust test-ts
	@echo "All examples built and tested successfully!"

build:
	@echo "Building all ZK examples..."
	@for dir in $(ZK_EXAMPLES); do \
		echo "Building $$dir..."; \
		cd $$dir && cargo build-sbf && cd ..; \
		if [ $$? -ne 0 ]; then \
			echo "Failed to build $$dir"; \
			exit 1; \
		fi; \
		echo "$$dir built successfully"; \
	done
	@echo "All programs built successfully!"

deploy: build
	@echo "Deploying all ZK programs..."
	@for dir in $(ZK_EXAMPLES); do \
		echo "Deploying $$dir..."; \
		if [ -f "$$dir/target/deploy/"*.so ]; then \
			solana program deploy $$dir/target/deploy/*.so; \
			if [ $$? -ne 0 ]; then \
				echo "Failed to deploy $$dir"; \
				exit 1; \
			fi; \
			echo "$$dir deployed successfully"; \
		else \
			echo "No .so file found for $$dir, skipping deploy"; \
		fi; \
	done
	@echo "All programs deployed!"

test-rust:
	@echo "Running Rust tests for all ZK examples..."
	@for dir in $(ZK_EXAMPLES); do \
		echo "Testing $$dir (Rust)..."; \
		cd $$dir && cargo test-sbf && cd ..; \
		if [ $$? -ne 0 ]; then \
			echo "Rust tests failed for $$dir"; \
			exit 1; \
		fi; \
		echo "$$dir Rust tests passed"; \
	done
	@echo "All Rust tests passed!"

test-ts: deploy
	@echo "Running TypeScript tests for all ZK examples..."
	@for dir in $(ZK_EXAMPLES); do \
		echo "Testing $$dir (TypeScript)..."; \
		if [ -f "$$dir/package.json" ]; then \
			cd $$dir && npm run test:ts && cd ..; \
			if [ $$? -ne 0 ]; then \
				echo "TypeScript tests failed for $$dir"; \
				exit 1; \
			fi; \
			echo "$$dir TypeScript tests passed"; \
		else \
			echo "No package.json found in $$dir, skipping TS tests"; \
		fi; \
	done
	@echo "All TypeScript tests passed!"

setup:
	@echo "Setting up circuits for all ZK examples..."
	@for dir in $(ZK_EXAMPLES); do \
		echo "Setting up $$dir..."; \
		if [ -f "$$dir/scripts/setup.sh" ]; then \
			cd $$dir && ./scripts/setup.sh && cd ..; \
			if [ $$? -ne 0 ]; then \
				echo "Setup failed for $$dir"; \
				exit 1; \
			fi; \
			echo "$$dir setup completed"; \
		else \
			echo "No setup script found in $$dir"; \
		fi; \
	done
	@echo "All setups completed!"

clean:
	@echo "Cleaning all ZK examples..."
	@for dir in $(ZK_EXAMPLES); do \
		echo "Cleaning $$dir..."; \
		cd $$dir && cargo clean && cd ..; \
		if [ -d "$$dir/build" ]; then \
			rm -rf $$dir/build; \
		fi; \
		if [ -d "$$dir/node_modules" ]; then \
			rm -rf $$dir/node_modules; \
		fi; \
	done
	@echo "All examples cleaned!"

# Individual example targets
zk-nullifier:
	@echo "Building, deploying, and testing zk-nullifier..."
	@cd zk-nullifier && cargo build-sbf && cargo test-sbf
	@solana program deploy zk-nullifier/target/deploy/zk_nullifier.so
	@if [ -f "zk-nullifier/package.json" ]; then \
		cd zk-nullifier && npm run test:ts; \
	fi
	@echo "zk-nullifier completed!"

zk-id:
	@echo "Building, deploying, and testing zk-id..."
	@cd zk-id && cargo build-sbf && cargo test-sbf
	@solana program deploy zk-id/target/deploy/zk_id.so
	@if [ -f "zk-id/package.json" ]; then \
		cd zk-id && npm run test:ts; \
	fi
	@echo "zk-id completed!"

zk-merkle-proof:
	@echo "Building, deploying, and testing zk-merkle-proof..."
	@cd zk-merkle-proof && cargo build-sbf && cargo test-sbf
	@solana program deploy zk-merkle-proof/target/deploy/zk_merkle_proof.so
	@if [ -f "zk-merkle-proof/package.json" ]; then \
		cd zk-merkle-proof && npm run test:ts; \
	fi
	@echo "zk-merkle-proof completed!"
