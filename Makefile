.PHONY: build-debug
build-debug:
	(cargo build --bin cli ; cp target/debug/cli boner-cli)


.PHONY: fmt
fmt:
	(find . -type f -name "*.rs" -not -path "*target*" -exec rustfmt --edition 2021 {} \;)