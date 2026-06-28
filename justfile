list:
    @just --list

check-example:
    cargo run \
        --package=cli \
        --features=server \
        -- \
        check  \
        --host="http://localhost" \
        --port="8081" \
        --main="example/main.typ"

lint:
    cargo clippy --workspace --features=server --features=jar
    taplo check
    cargo fmt --check
