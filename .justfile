# show help
_help:
    just -l

# run cargo deny
[group("qa")]
audit:
    cargo deny check

# run clippy
[group("qa")]
clippy:
    cargo clippy

# run tests
[group("qa")]
test:
    cargo nextest run

# run all QA suite
[group("qa")]
qa: audit clippy test

# install to ~/bin
[group("install")]
install: qa
    cargo build --release
    cp -v target/release/lumos ~/bin/
