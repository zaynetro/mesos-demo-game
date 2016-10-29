# Thanks http://stackoverflow.com/questions/39661138/optimising-cargo-build-times-in-docker
FROM ubuntu:xenial
RUN apt-get update
RUN apt-get install curl build-essential ca-certificates libssl-dev -y
RUN mkdir /rust
WORKDIR /rust
RUN curl https://sh.rustup.rs -s >> rustup.sh
RUN chmod 755 /rust/rustup.sh
RUN ./rustup.sh -y --default-toolchain nightly
ENV PATH=/root/.cargo/bin:$PATH
RUN mkdir /app
WORKDIR /app
COPY Cargo.toml /app/
COPY Cargo.lock /app/
COPY dummy.rs /app/
RUN cargo build --lib --release
COPY src/ /app/src/
RUN cargo build --release
CMD ["./target/release/demo-game"]
EXPOSE 3000
