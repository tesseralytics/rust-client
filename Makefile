.PHONY: help fetch-spec test lint fmt doc check

help:  ## Show this help
	@grep -E '^[a-zA-Z_-]+:.*?## .*$$' $(MAKEFILE_LIST) | awk 'BEGIN {FS = ":.*?## "}; {printf "  \033[36m%-12s\033[0m %s\n", $$1, $$2}'

fetch-spec:  ## Refresh the vendored openapi.json from production
	curl -fsSL https://tesseralytics.dev/v1/openapi.json -o openapi.json

test:  ## Run the test suite (all features)
	cargo test --all-features

lint:  ## Clippy, warnings as errors
	cargo clippy --all-targets --all-features -- -D warnings

fmt:  ## Check formatting
	cargo fmt --all -- --check

doc:  ## Build rustdoc
	cargo doc --no-deps

check: lint fmt test  ## Lint + fmt + test
