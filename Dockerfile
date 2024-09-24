FROM rustlang/rust:nightly-bookworm as builder

RUN apt-get update && apt-get install -y protobuf-compiler

WORKDIR /tripa-build

COPY ./tripa /tripa-build/tripa

COPY ./message /tripa-build/message

WORKDIR /tripa-build/tripa
RUN cargo build --release

FROM debian:bookworm
RUN apt-get update && apt-get install -y libssl3 ca-certificates
COPY --from=builder /tripa-build/tripa/target/release/tripa /tripa/tripa

EXPOSE 3000
WORKDIR /tripa
CMD ["/tripa/tripa"]
