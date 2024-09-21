FROM rustlang/rust:nightly as builder

RUN apt-get update && apt-get install -y protobuf-compiler

WORKDIR /tripa

COPY ./tripa ./

COPY ./message /message

RUN cargo build --release

EXPOSE 3000
CMD ["./target/release/tripa"]
