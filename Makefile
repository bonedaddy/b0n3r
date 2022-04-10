.PHONY: build-debug
build-debug:
	(cargo build --bin cli ; cp target/debug/cli boner-cli)
.PHONY: build
build:
	(cargo build --release --bin cli ; cp target/release/cli boner-cli)


.PHONY: fmt
fmt:
	(find . -type f -name "*.rs" -not -path "*target*" -exec rustfmt --edition 2021 {} \;)
