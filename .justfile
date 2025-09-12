# show help
_help:
    just -l

# run cargo machete
[group("qa")]
machete:
    cargo machete

# run cargo deny
[group("qa")]
audit:
    cargo deny check

# run clippy
[group("qa")]
clippy:
    cargo clippy --all-targets

# run tests
[group("qa")]
test:
    cargo nextest run --all-targets
    cargo test --doc

# run all QA suite
[group("qa")]
qa: machete audit clippy test

# install to ~/bin
[group("install")]
install: qa
    cargo build --release
    cp -v target/release/lumos ~/bin/
